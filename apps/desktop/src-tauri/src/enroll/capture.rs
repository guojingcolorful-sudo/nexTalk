//! Enrollment capture: the microphone, the guard checks, and the WAV (02-04 T4.1).
//!
//! Flow: [`CaptureSession::begin`] starts a [`CaptureBackend`], the realtime
//! callback copies each block into a bounded queue ([`CaptureSink`], T-02-19),
//! a drain thread consumes that queue continuously (publishing the newest peak
//! for the level meter), [`finish_capture`] stops the backend, drains, drops
//! the first [`STARTUP_DISCARD_MS`] (device startup click), measures the
//! speech/silence split with the 02-03 energy VAD, validates against
//! [`CaptureGuard`], and writes the 16 kHz mono PCM16 WAV under
//! `<root>/enroll/<sessionId>.wav` with owner-only permissions (T-02-16).
//!
//! The guard's Chinese messages are the UI copy; [`CaptureError::code`] is the
//! stable machine-readable twin for the frontend.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio::resample::{resample_mono, resample_mono_to_pcm16, OUTPUT_RATE_HZ};
use crate::pipeline::vad::{EnergyVad, VadEvent, FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE_HZ};

/// The local audio graph's rate. A device that insists on another rate is
/// resampled to this before the VAD runs.
pub const ENROLLMENT_SAMPLE_RATE_HZ: u32 = SAMPLE_RATE_HZ;

/// The shortest acceptable take (「请至少朗读 1 分钟」).
pub const MIN_RECORDING_SECS: u64 = 60;
/// The longest acceptable take; the UI stops the recorder at 3 minutes.
pub const MAX_RECORDING_SECS: u64 = 180;
/// voice_clone's hard upload limit — a sample over this cannot be trained.
pub const MAX_SAMPLE_BYTES: usize = 10 * 1024 * 1024;
/// More silence than this and the user gets the "check your microphone" copy.
pub const MAX_SILENCE_RATIO: f32 = 0.6;
/// The floor on actual voiced time inside a take.
pub const MIN_SPEECH_SECS: u64 = 10;
/// Device startup click / room tone trim — the first half-second is dropped.
pub const STARTUP_DISCARD_MS: u64 = 500;
/// Blocks the realtime callback may queue before it starts counting drops.
/// 256 × 480 samples ≈ 2.6 s at 48 kHz — far above normal scheduling jitter.
pub const CAPTURE_QUEUE_BLOCKS: usize = 256;
/// Canonical PCM16 RIFF header size — used to predict the file size in the
/// guard, before anything is written.
const WAV_HEADER_BYTES: usize = 44;

// ---------------------------------------------------------------------------
// errors (Chinese UI copy + stable codes)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum CaptureError {
    /// Recorded time is below the 1-minute floor.
    TooShort { seconds: f64, min_secs: u64 },
    /// Recorded time is above the 3-minute ceiling.
    TooLong { seconds: f64, max_secs: u64 },
    /// The take would upload more than voice_clone accepts.
    TooLarge { bytes: usize, max_bytes: usize },
    /// Almost nothing was voiced — the mic is muted or the wrong device.
    TooSilent { silence_ratio: f32 },
    /// Voiced, but not enough of it to train a voice from.
    TooLittleSpeech {
        speech_secs: f64,
        min_speech_secs: u64,
    },
    /// No input device / stream refused.
    Device(String),
    /// Writing the sample failed.
    Io(String),
    /// `stop` without a `start`.
    NoActiveTake,
    /// `start` while a take is already running.
    AlreadyRecording,
}

impl CaptureError {
    /// Machine-readable twin of [`CaptureError::message`].
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooShort { .. } => "too_short",
            Self::TooLong { .. } => "too_long",
            Self::TooLarge { .. } => "too_large",
            Self::TooSilent { .. } => "too_silent",
            Self::TooLittleSpeech { .. } => "too_little_speech",
            Self::Device(_) => "device",
            Self::Io(_) => "io",
            Self::NoActiveTake => "no_active_take",
            Self::AlreadyRecording => "already_recording",
        }
    }

    /// The locked Chinese UI copy.
    pub fn message(&self) -> String {
        match self {
            Self::TooShort { seconds, .. } => {
                format!("录音太短，请至少朗读 1 分钟（当前 {:.0} 秒）", seconds)
            }
            Self::TooLong { .. } => "录音太长了，请将时长控制在 3 分钟以内".to_string(),
            Self::TooLarge { .. } => "录音文件过大，请缩短录音后重试".to_string(),
            Self::TooSilent { .. } => "没有录到声音，请检查麦克风".to_string(),
            Self::TooLittleSpeech { .. } => "有效朗读不足，请完整朗读提示文本后重录".to_string(),
            Self::Device(detail) => format!("麦克风不可用：{detail}"),
            Self::Io(detail) => format!("保存录音失败：{detail}"),
            Self::NoActiveTake => "当前没有进行中的录音".to_string(),
            Self::AlreadyRecording => "已有录音进行中，请先停止".to_string(),
        }
    }
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for CaptureError {}

// ---------------------------------------------------------------------------
// guard + stats
// ---------------------------------------------------------------------------

/// The validation window. Defaults are the product tolerances; tests narrow
/// individual fields instead of inventing a second code path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureGuard {
    pub min_secs: u64,
    pub max_secs: u64,
    pub max_bytes: usize,
    pub max_silence_ratio: f32,
    pub min_speech_secs: u64,
}

impl Default for CaptureGuard {
    fn default() -> Self {
        Self {
            min_secs: MIN_RECORDING_SECS,
            max_secs: MAX_RECORDING_SECS,
            max_bytes: MAX_SAMPLE_BYTES,
            max_silence_ratio: MAX_SILENCE_RATIO,
            min_speech_secs: MIN_SPEECH_SECS,
        }
    }
}

/// What the take measured out to. `duration_s` is the *captured* length (the
/// on-screen timer, including the 500 ms that will be trimmed — a take stopped
/// at exactly 1:00 must not fail on the trim); `silence_ratio` and
/// `speech_secs` are measured on the kept audio, so the startup click never
/// counts as voice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureStats {
    pub duration_s: f64,
    pub bytes: usize,
    pub silence_ratio: f32,
    pub speech_secs: f64,
}

/// The guard, in order. First failure wins, so the message a user sees is the
/// most actionable one (a 15-second silent take is "too short", not "silent").
pub fn validate_capture(stats: &CaptureStats, guard: &CaptureGuard) -> Result<(), CaptureError> {
    const EPS: f64 = 1e-6;
    if stats.duration_s + EPS < guard.min_secs as f64 {
        return Err(CaptureError::TooShort {
            seconds: stats.duration_s,
            min_secs: guard.min_secs,
        });
    }
    if stats.duration_s > guard.max_secs as f64 + EPS {
        return Err(CaptureError::TooLong {
            seconds: stats.duration_s,
            max_secs: guard.max_secs,
        });
    }
    if stats.bytes > guard.max_bytes {
        return Err(CaptureError::TooLarge {
            bytes: stats.bytes,
            max_bytes: guard.max_bytes,
        });
    }
    if stats.silence_ratio > guard.max_silence_ratio {
        return Err(CaptureError::TooSilent {
            silence_ratio: stats.silence_ratio,
        });
    }
    if stats.speech_secs + EPS < guard.min_speech_secs as f64 {
        return Err(CaptureError::TooLittleSpeech {
            speech_secs: stats.speech_secs,
            min_speech_secs: guard.min_speech_secs,
        });
    }
    Ok(())
}

/// Drop the first `discard_ms` of a take (the device startup click).
pub fn discard_startup_ms(samples: Vec<f32>, sample_rate: u32, discard_ms: u64) -> Vec<f32> {
    if sample_rate == 0 {
        return Vec::new();
    }
    let discard = (sample_rate as u64 * discard_ms / 1000) as usize;
    if samples.len() <= discard {
        return Vec::new();
    }
    let mut samples = samples;
    samples.split_off(discard)
}

/// Voiced seconds + silence ratio, measured with the 02-03 energy VAD (there is
/// exactly one VAD in this codebase). The VAD's frame geometry is fixed at
/// 48 kHz, so a take from a device running another rate is converted first.
fn voice_stats(kept: &[f32], sample_rate: u32) -> (f64, f32) {
    let resampled: Vec<f32>;
    let samples: &[f32] = if sample_rate == ENROLLMENT_SAMPLE_RATE_HZ {
        kept
    } else {
        resampled = resample_mono(kept, sample_rate, ENROLLMENT_SAMPLE_RATE_HZ);
        &resampled
    };
    if samples.is_empty() {
        return (0.0, 1.0);
    }

    let mut vad = EnergyVad::default();
    let mut speech_ms: u64 = 0;
    let frames = samples.len() / FRAME_SAMPLES;
    for index in 0..frames {
        let frame = &samples[index * FRAME_SAMPLES..(index + 1) * FRAME_SAMPLES];
        for event in vad.push(frame, index as u64 * FRAME_MS) {
            if let VadEvent::SpeechEnd {
                speech_ms: run_ms, ..
            } = event
            {
                speech_ms += run_ms;
            }
        }
    }
    // A run still open at the end of the take is voiced time too.
    speech_ms += vad.speech_ms();

    let total_ms = samples.len() as u64 * 1000 / ENROLLMENT_SAMPLE_RATE_HZ as u64;
    let speech_ms = speech_ms.min(total_ms);
    let silence_ms = total_ms.saturating_sub(speech_ms);
    let ratio = if total_ms == 0 {
        1.0
    } else {
        silence_ms as f32 / total_ms as f32
    };
    (speech_ms as f64 / 1000.0, ratio)
}

/// Frames the 16 kHz export will contain — the rust `* rate_out / rate_in`
/// truncation [`resample_mono_to_pcm16`] performs.
fn expected_pcm16_len(frames_in: usize, in_rate: u32) -> usize {
    if in_rate == 0 {
        return 0;
    }
    frames_in * OUTPUT_RATE_HZ as usize / in_rate as usize
}

// ---------------------------------------------------------------------------
// realtime callback discipline (T-02-19)
// ---------------------------------------------------------------------------

/// The callback's whole job: copy one block into the bounded queue or count it
/// as dropped. No locking, no logging, no blocking.
#[derive(Clone)]
pub struct CaptureSink {
    sender: SyncSender<Vec<f32>>,
    overflows: Arc<AtomicU64>,
}

impl CaptureSink {
    pub fn new(sender: SyncSender<Vec<f32>>, overflows: Arc<AtomicU64>) -> Self {
        Self { sender, overflows }
    }

    /// Called from the audio callback. Never blocks: a full queue (the consumer
    /// stalled) and a closed queue (the take was cancelled) both mean "drop
    /// this block and count it".
    pub fn push(&self, samples: &[f32]) {
        match self.sender.try_send(samples.to_vec()) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.overflows.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn overflows(&self) -> u64 {
        self.overflows.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// backends
// ---------------------------------------------------------------------------

/// Where blocks come from. The real implementation is [`CpalCapture`];
/// [`ScriptedCapture`] feeds a fixed take so tests are deterministic and CI
/// needs no microphone.
pub trait CaptureBackend: Send {
    /// The rate the backend's blocks are at.
    fn sample_rate(&self) -> u32;
    /// Open the stream and hand back the consumer end of the block queue.
    fn start(&mut self) -> Result<Receiver<Vec<f32>>, CaptureError>;
    /// Stop the stream. Blocks already queued stay readable.
    fn stop(&mut self) -> Result<(), CaptureError>;
    /// Blocks dropped by the realtime discipline (diagnostics).
    fn overflows(&self) -> u64 {
        0
    }
}

/// CoreAudio capture through cpal: the default input device, its default rate.
pub struct CpalCapture {
    device: cpal::Device,
    config: cpal::SupportedStreamConfig,
    stream: Option<cpal::Stream>,
    overflows: Arc<AtomicU64>,
}

impl CpalCapture {
    pub fn new() -> Result<Self, CaptureError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| CaptureError::Device("没有可用的输入设备".to_string()))?;
        let config = device
            .default_input_config()
            .map_err(|error| CaptureError::Device(error.to_string()))?;
        Ok(Self {
            device,
            config,
            stream: None,
            overflows: Arc::new(AtomicU64::new(0)),
        })
    }
}

fn stream_error(error: cpal::Error) {
    // The device vanished mid-take; the guard's length check is what the user
    // sees, this is only for the developer console. No take data involved.
    eprintln!("[enroll] capture stream error: {error}");
}

fn downmix_f32(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    data.chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

impl CaptureBackend for CpalCapture {
    fn sample_rate(&self) -> u32 {
        self.config.sample_rate()
    }

    fn start(&mut self) -> Result<Receiver<Vec<f32>>, CaptureError> {
        if self.stream.is_some() {
            return Err(CaptureError::AlreadyRecording);
        }
        let (sender, receiver) = sync_channel::<Vec<f32>>(CAPTURE_QUEUE_BLOCKS);
        let sink = CaptureSink::new(sender, Arc::clone(&self.overflows));
        let config: cpal::StreamConfig = self.config.clone().into();
        let channels = config.channels.max(1) as usize;

        let stream = match self.config.sample_format() {
            cpal::SampleFormat::F32 => {
                let sink = sink.clone();
                self.device.build_input_stream(
                    config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if channels == 1 {
                            sink.push(data);
                        } else {
                            sink.push(&downmix_f32(data, channels));
                        }
                    },
                    stream_error,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let sink = sink.clone();
                self.device.build_input_stream(
                    config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let floats: Vec<f32> = data
                            .iter()
                            .map(|sample| *sample as f32 / 32_768.0)
                            .collect();
                        if channels == 1 {
                            sink.push(&floats);
                        } else {
                            sink.push(&downmix_f32(&floats, channels));
                        }
                    },
                    stream_error,
                    None,
                )
            }
            cpal::SampleFormat::U16 => {
                let sink = sink.clone();
                self.device.build_input_stream(
                    config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        let floats: Vec<f32> = data
                            .iter()
                            .map(|sample| (*sample as f32 - 32_768.0) / 32_768.0)
                            .collect();
                        if channels == 1 {
                            sink.push(&floats);
                        } else {
                            sink.push(&downmix_f32(&floats, channels));
                        }
                    },
                    stream_error,
                    None,
                )
            }
            other => return Err(CaptureError::Device(format!("不支持的采样格式 {other:?}"))),
        }
        .map_err(|error| CaptureError::Device(error.to_string()))?;

        stream
            .play()
            .map_err(|error| CaptureError::Device(error.to_string()))?;
        self.stream = Some(stream);
        Ok(receiver)
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.stream = None;
        Ok(())
    }

    fn overflows(&self) -> u64 {
        self.overflows.load(Ordering::Relaxed)
    }
}

/// A backend that plays back a fixed script of blocks. The queue is sized to
/// the script, so a scripted take is delivered in full, in order, and is
/// byte-for-byte reproducible between runs (T4.1 Test 7).
pub struct ScriptedCapture {
    sample_rate: u32,
    blocks: Option<Vec<Vec<f32>>>,
    overflows: Arc<AtomicU64>,
    producer: Option<std::thread::JoinHandle<()>>,
}

impl ScriptedCapture {
    pub fn new(sample_rate: u32, blocks: Vec<Vec<f32>>) -> Self {
        Self {
            sample_rate,
            blocks: Some(blocks),
            overflows: Arc::new(AtomicU64::new(0)),
            producer: None,
        }
    }
}

impl CaptureBackend for ScriptedCapture {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn start(&mut self) -> Result<Receiver<Vec<f32>>, CaptureError> {
        if self.producer.is_some() {
            return Err(CaptureError::AlreadyRecording);
        }
        let blocks = self.blocks.take().ok_or(CaptureError::AlreadyRecording)?;
        let (sender, receiver) = sync_channel(blocks.len().max(1));
        let overflows = Arc::clone(&self.overflows);
        let producer = std::thread::spawn(move || {
            for block in blocks {
                if sender.try_send(block).is_err() {
                    overflows.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        self.producer = Some(producer);
        Ok(receiver)
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        if let Some(producer) = self.producer.take() {
            let _ = producer.join();
        }
        Ok(())
    }

    fn overflows(&self) -> u64 {
        self.overflows.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// session + finish
// ---------------------------------------------------------------------------

/// A lock-free read of the most recent input peak (`|sample|`, 0.0–1.0), for
/// the wizard's level meter. Written by the drain thread only — never by the
/// realtime callback (T-02-19).
#[derive(Clone, Default)]
pub struct LevelHandle(Arc<AtomicU32>);

impl LevelHandle {
    /// The peak of the newest drained block (0.0 before the first block).
    pub fn latest(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    fn publish(&self, peak: f32) {
        self.0.store(peak.to_bits(), Ordering::Relaxed);
    }
}

/// One in-flight (or just-finished) take: the consumer thread draining the
/// block queue, plus the level the meter reads.
///
/// The drain is a thread, not a `try_iter` at stop time: the realtime queue
/// holds [`CAPTURE_QUEUE_BLOCKS`] blocks (≈ 2.6 s), so a take longer than that
/// would overflow, count drops, and fail the guard as "too short" — every real
/// 1–3 minute take must keep the queue moving (T4.3 fix).
pub struct CaptureSession {
    sample_rate: u32,
    drain: Option<std::thread::JoinHandle<Vec<f32>>>,
    level: LevelHandle,
}

impl CaptureSession {
    /// Start `backend`, then start draining its block queue continuously.
    pub fn begin(backend: &mut dyn CaptureBackend) -> Result<Self, CaptureError> {
        let sample_rate = backend.sample_rate();
        if sample_rate == 0 {
            return Err(CaptureError::Device("设备采样率无效".to_string()));
        }
        let receiver = backend.start()?;
        let level = LevelHandle::default();
        let thread_level = level.clone();
        let drain = std::thread::spawn(move || {
            let mut samples: Vec<f32> = Vec::new();
            while let Ok(block) = receiver.recv() {
                thread_level.publish(block.iter().fold(0.0f32, |peak, s| peak.max(s.abs())));
                samples.extend_from_slice(&block);
            }
            samples
        });
        Ok(Self {
            sample_rate,
            drain: Some(drain),
            level,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The level meter's handle (cheap clone; reads the newest block's peak).
    pub fn level(&self) -> LevelHandle {
        self.level.clone()
    }

    /// Every block the backend produced, in order. Call after the backend
    /// stopped; the drain thread ends when the queue disconnects.
    pub fn drain_samples(&mut self) -> Vec<f32> {
        match self.drain.take() {
            Some(drain) => drain.join().unwrap_or_default(),
            None => Vec::new(),
        }
    }
}

/// What a saved take measured out to.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureResult {
    /// `<root>/enroll/<sessionId>.wav`.
    pub path: PathBuf,
    /// The written (post-trim) duration.
    pub duration_s: f64,
    pub silence_ratio: f32,
    /// The written file's size in bytes.
    pub bytes: u64,
    /// Blocks the realtime discipline had to drop (0 in a healthy take).
    pub overflows: u64,
}

/// Stop, drain, trim, measure, validate, and write the sample WAV.
///
/// Rejected takes leave no file behind — the guard runs before the writer.
pub fn finish_capture(
    mut session: CaptureSession,
    backend: &mut dyn CaptureBackend,
    guard: &CaptureGuard,
    root: &Path,
    session_id: &str,
) -> Result<CaptureResult, CaptureError> {
    backend.stop()?;
    let rate = session.sample_rate();
    let captured = session.drain_samples();
    let captured_secs = captured.len() as f64 / rate as f64;

    let kept = discard_startup_ms(captured, rate, STARTUP_DISCARD_MS);
    let kept_secs = kept.len() as f64 / rate as f64;
    let (speech_secs, silence_ratio) = voice_stats(&kept, rate);

    let stats = CaptureStats {
        duration_s: captured_secs,
        bytes: WAV_HEADER_BYTES + expected_pcm16_len(kept.len(), rate) * 2,
        silence_ratio,
        speech_secs,
    };
    validate_capture(&stats, guard)?;

    let pcm16 = resample_mono_to_pcm16(&kept, rate, OUTPUT_RATE_HZ);
    let path = save_wav(root, session_id, &pcm16)?;
    let bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);

    Ok(CaptureResult {
        path,
        duration_s: kept_secs,
        silence_ratio,
        bytes,
        overflows: backend.overflows(),
    })
}

/// `<root>/enroll` — where every sample lives.
pub fn enrollment_dir(root: &Path) -> PathBuf {
    root.join("enroll")
}

/// `<root>/enroll/<session_id>.wav`.
pub fn sample_path(root: &Path, session_id: &str) -> PathBuf {
    enrollment_dir(root).join(format!("{session_id}.wav"))
}

/// Write a 16 kHz mono PCM16 WAV, owner-only (T-02-16: the sample is biometric
/// data and stays readable by this user alone).
fn save_wav(root: &Path, session_id: &str, pcm16: &[i16]) -> Result<PathBuf, CaptureError> {
    use std::os::unix::fs::PermissionsExt;

    let dir = enrollment_dir(root);
    std::fs::create_dir_all(&dir).map_err(|error| CaptureError::Io(error.to_string()))?;

    let path = sample_path(root, session_id);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: OUTPUT_RATE_HZ,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec)
        .map_err(|error| CaptureError::Io(error.to_string()))?;
    for sample in pcm16 {
        writer
            .write_sample(*sample)
            .map_err(|error| CaptureError::Io(error.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|error| CaptureError::Io(error.to_string()))?;

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| CaptureError::Io(error.to_string()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_frames(seconds: u64, amplitude: f32) -> Vec<f32> {
        let len = seconds as usize * ENROLLMENT_SAMPLE_RATE_HZ as usize;
        (0..len)
            .map(|n| {
                let t = n as f32 / ENROLLMENT_SAMPLE_RATE_HZ as f32;
                (2.0 * std::f32::consts::PI * 440.0 * t).sin() * amplitude
            })
            .collect()
    }

    #[test]
    fn the_guard_reports_the_first_failure_in_order() {
        let guard = CaptureGuard::default();
        // A 15-second silent take is "too short", not "silent": the user can
        // act on the first message.
        let silent_short = CaptureStats {
            duration_s: 15.0,
            bytes: 1000,
            silence_ratio: 1.0,
            speech_secs: 0.0,
        };
        assert!(matches!(
            validate_capture(&silent_short, &guard),
            Err(CaptureError::TooShort { .. })
        ));
    }

    #[test]
    fn the_startup_trim_cuts_exactly_the_first_half_second() {
        let samples = sine_frames(2, 0.3);
        let kept = discard_startup_ms(samples, ENROLLMENT_SAMPLE_RATE_HZ, STARTUP_DISCARD_MS);
        assert_eq!(kept.len(), 2 * ENROLLMENT_SAMPLE_RATE_HZ as usize - 24_000);
        // Too-short takes yield an empty slice, never a panic.
        assert!(discard_startup_ms(vec![0.0; 100], ENROLLMENT_SAMPLE_RATE_HZ, 500).is_empty());
    }

    #[test]
    fn voice_stats_measure_a_tone_and_a_room() {
        let (speech_s, ratio) = voice_stats(&sine_frames(2, 0.3), ENROLLMENT_SAMPLE_RATE_HZ);
        assert!((speech_s - 2.0).abs() < 0.05, "{speech_s}");
        assert!(ratio < 0.05, "{ratio}");

        let (speech_s, ratio) = voice_stats(&vec![0.0; 48_000 * 2], ENROLLMENT_SAMPLE_RATE_HZ);
        assert!(speech_s < 0.05, "{speech_s}");
        assert!((ratio - 1.0).abs() < 0.01, "{ratio}");

        // Empty input is all silence, not a panic.
        assert_eq!(voice_stats(&[], ENROLLMENT_SAMPLE_RATE_HZ), (0.0, 1.0));
    }

    #[test]
    fn dropped_blocks_are_counted_both_when_full_and_when_closed() {
        let (sender, receiver) = sync_channel::<Vec<f32>>(2);
        let sink = CaptureSink::new(sender, Arc::new(AtomicU64::new(0)));
        sink.push(&[0.0]);
        sink.push(&[0.0]);
        sink.push(&[0.0]); // full
        assert_eq!(sink.overflows(), 1);
        drop(receiver);
        sink.push(&[0.0]); // disconnected
        assert_eq!(sink.overflows(), 2);
    }

    #[test]
    fn the_level_handle_round_trips_a_peak() {
        let level = LevelHandle::default();
        assert_eq!(level.latest(), 0.0, "nothing published yet");
        level.publish(0.42);
        assert!((level.latest() - 0.42).abs() < 1e-6);
    }

    #[test]
    fn expected_lengths_match_the_resampler_truncation() {
        assert_eq!(expected_pcm16_len(48_000, 48_000), 16_000);
        assert_eq!(expected_pcm16_len(48_001, 48_000), 16_000);
        assert_eq!(expected_pcm16_len(3, 48_000), 1);
        assert_eq!(expected_pcm16_len(2, 48_000), 0);
        assert_eq!(expected_pcm16_len(100, 0), 0);
    }
}
