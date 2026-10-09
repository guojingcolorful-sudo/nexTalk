//! The live capture chain: 48 kHz mic blocks → 10 ms frames → AEC → 16 kHz
//! mono PCM16 for the STT stage (02-05 T5.2).
//!
//! ```text
//! cpal callback ──▶ bounded queue ──▶ FrameAssembler ──▶ AudioProcessor
//!   (bounded.rs:      (drain worker)     480-sample        (process_capture
//!    copy-or-count)                        frames            _frame)
//!                                                              │
//!                          16 kHz mono PCM16 ◀── StreamingResampler (3:1)
//!                                                              │
//!                                                    讯飞 / Deepgram STT
//! ```
//!
//! Three boundaries this module owns, and why each is where it is:
//!
//! - **The callback never does any of this.** It copies into
//!   [`CaptureSink`](crate::audio::bounded::CaptureSink) or counts a drop; the
//!   chain's [`CaptureChain::poll`] does the framing, the AEC and the resample
//!   on the consumer's thread. That is what keeps a 512-frame device buffer
//!   from deadlocking a realtime thread.
//! - **The AEC runs at 48 kHz, before the resample.** The resampler is after
//!   the processor on purpose: AEC3's frame is defined at the processing rate,
//!   and resampling first would put the near and far ends on different clocks.
//! - **The chain never panics on a bad block.** A short or long block is
//!   padded/framed by [`FrameAssembler`]; a processor refusal is counted and
//!   reported through [`CaptureChain::drain_errors`], because a panic here
//!   ends the session and a silent drop ends the transcript.
//!
//! Both `CaptureError` and the counters are the diagnostic surface the UI and
//! the failure-case library read.

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::audio::aec::{AecError, SharedProcessor, FRAME_SAMPLES, PROCESSOR_RATE_HZ};
use crate::audio::bounded::CaptureSink;
use crate::audio::resample::{f32_to_pcm16, StreamingResampler, OUTPUT_RATE_HZ};
use crate::audio::routing::{RoutingPlan, StreamRole};

/// The STT feed's rate (16 kHz mono, PCM16 LE).
pub const STT_RATE_HZ: u32 = OUTPUT_RATE_HZ;
/// Default bound on the callback's queue (see [`crate::audio::bounded`]).
pub const CAPTURE_QUEUE_BLOCKS: usize = crate::audio::bounded::DEFAULT_QUEUE_BLOCKS;

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// What can go wrong between the microphone and the STT feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    /// No input device at all (headless CI, everything unplugged).
    NoInputDevice,
    /// The device exists but refused to open or to give a config.
    Device(String),
    /// The stream was built but the backend reported an asynchronous failure.
    Stream(String),
    /// The AEC block refused a frame.
    Processor(AecError),
    /// The resampler could not be built for the device's rate pair.
    Resampler(String),
    /// `poll`/`stop` on a chain that was never started (or was stopped).
    NotRunning,
}

impl CaptureError {
    /// Stable machine-readable twin of [`Self::message`] (frontend contract).
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoInputDevice => "no_input_device",
            Self::Device(_) => "device",
            Self::Stream(_) => "stream",
            Self::Processor(_) => "processor",
            Self::Resampler(_) => "resampler",
            Self::NotRunning => "not_running",
        }
    }

    /// The locked Chinese UI copy.
    pub fn message(&self) -> String {
        match self {
            Self::NoInputDevice => "没有可用的麦克风".to_string(),
            Self::Device(detail) => format!("麦克风打开失败：{detail}"),
            Self::Stream(detail) => format!("麦克风数据流中断：{detail}"),
            Self::Processor(error) => error.to_string(),
            Self::Resampler(detail) => format!("采样率转换失败：{detail}"),
            Self::NotRunning => "音频采集尚未启动".to_string(),
        }
    }
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for CaptureError {}

impl From<AecError> for CaptureError {
    fn from(error: AecError) -> Self {
        Self::Processor(error)
    }
}

// ---------------------------------------------------------------------------
// frame assembly
// ---------------------------------------------------------------------------

/// Turns arbitrary device blocks into the processor's fixed 10 ms frames.
///
/// A CoreAudio device buffer is whatever the hardware likes (512 frames is
/// common), which is not a multiple of 480. The assembler carries the
/// remainder, so no sample is ever dropped or repeated at a block seam.
#[derive(Debug, Clone)]
pub struct FrameAssembler {
    frame_samples: usize,
    pending: Vec<f32>,
}

impl FrameAssembler {
    pub fn new(frame_samples: usize) -> Self {
        Self {
            frame_samples: frame_samples.max(1),
            pending: Vec::new(),
        }
    }

    /// The production assembler: 480-sample frames (10 ms @ 48 kHz).
    pub fn for_processor() -> Self {
        Self::new(FRAME_SAMPLES)
    }

    /// Feed one device block; get back every complete frame it completed.
    pub fn push(&mut self, block: &[f32]) -> Vec<Vec<f32>> {
        self.pending.extend_from_slice(block);
        let frames = self.pending.len() / self.frame_samples;
        let mut out = Vec::with_capacity(frames);
        let mut offset = 0;
        for _ in 0..frames {
            out.push(self.pending[offset..offset + self.frame_samples].to_vec());
            offset += self.frame_samples;
        }
        if offset > 0 {
            self.pending.drain(..offset);
        }
        out
    }

    /// Samples held back waiting for the rest of their frame.
    pub fn pending_samples(&self) -> usize {
        self.pending.len()
    }

    /// Discard the partial frame (session stop — the samples belong to no
    /// session that still exists).
    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

// ---------------------------------------------------------------------------
// block sources
// ---------------------------------------------------------------------------

/// Where raw blocks and asynchronous errors come from. The real implementation
/// is [`CpalSource`]; [`ScriptedSource`] makes the chain testable with no
/// microphone and no CI device.
pub trait BlockSource: Send {
    /// The device's rate — the rate its blocks are at (the chain resamples to
    /// 16 kHz from here).
    fn sample_rate(&self) -> u32;

    /// Open the stream and hand over the consumer end of the bounded queue.
    fn start(&mut self) -> Result<Receiver<Vec<f32>>, CaptureError>;

    /// Stop the stream. Blocks already queued stay readable.
    fn stop(&mut self) -> Result<(), CaptureError>;

    /// The device's name, for the settings surface (T5.4's visibility rule).
    /// `None` for scripted sources.
    fn name(&self) -> Option<String> {
        None
    }

    /// Blocks the realtime discipline dropped (diagnostics).
    fn overflows(&self) -> u64 {
        0
    }

    /// Errors the backend reported asynchronously (cpal's error callback).
    /// Drained by [`CaptureChain::drain_errors`] — this is the "separate
    /// channel" that keeps a device failure from being a log line nobody reads.
    fn drain_errors(&mut self) -> Vec<CaptureError> {
        Vec::new()
    }
}

/// CoreAudio capture through cpal: the default input device, its default rate.
///
/// The device is opened once at [`BlockSource::start`]; a device that vanishes
/// mid-session surfaces as an asynchronous [`CaptureError::Stream`] and is
/// rebuilt by `audio::device` (T5.4), not here.
pub struct CpalSource {
    device: cpal::Device,
    config: cpal::SupportedStreamConfig,
    stream: Option<cpal::Stream>,
    /// The error callback's channel — same discipline as the audio blocks
    /// (WR-09): the `SyncSender` goes to the audio thread, the receiver stays
    /// here for [`BlockSource::drain_errors`].
    errors: SyncSender<CaptureError>,
    error_receiver: Receiver<CaptureError>,
    /// Reports the error callback had to drop (queue full / consumer gone).
    /// Counted and surfaced through `drain_errors`, never silent.
    dropped_errors: Arc<std::sync::atomic::AtomicU64>,
    sink: Option<CaptureSink>,
    overflows: Arc<std::sync::atomic::AtomicU64>,
}

/// How many asynchronous device errors may wait for the consumer. cpal reports
/// stream failures a handful of times per session (a vanishing device is one
/// event, not a stream of them), so this is far beyond real use — and the
/// callback has a place to count what does not fit.
pub const ERROR_QUEUE_ITEMS: usize = 16;

/// The error callback's body, as a free function so the discipline is testable
/// without a device (WR-09): hand the report to the bounded queue, or count it.
/// `try_send` on a full or closed queue is the whole failure story — the audio
/// thread waits for nothing.
fn report_device_error(
    errors: &SyncSender<CaptureError>,
    dropped: &std::sync::atomic::AtomicU64,
    message: String,
) {
    use std::sync::atomic::Ordering;

    if errors.try_send(CaptureError::Stream(message)).is_err() {
        dropped.fetch_add(1, Ordering::Relaxed);
    }
}

/// Drain the error queue and append one report per counted drop. The count is
/// cleared as it is reported (it is a fact about this session's callback, not
/// a lifetime figure — a stale count must not inflate a later session's
/// diagnostics).
fn drain_reported_errors(
    errors: &Receiver<CaptureError>,
    dropped: &std::sync::atomic::AtomicU64,
) -> Vec<CaptureError> {
    use std::sync::atomic::Ordering;

    let mut drained: Vec<CaptureError> = errors.try_iter().collect();
    let lost = dropped.swap(0, Ordering::Relaxed);
    if lost > 0 {
        drained.push(CaptureError::Stream(format!(
            "采集回调队列已满，{lost} 条设备错误被丢弃"
        )));
    }
    drained
}

impl CpalSource {
    /// The default input device, or [`CaptureError::NoInputDevice`].
    pub fn new() -> Result<Self, CaptureError> {
        Self::from_device(None)
    }

    /// A named device (T5.5 routes the loopback role here); `None` = default.
    pub fn from_device(name: Option<&str>) -> Result<Self, CaptureError> {
        let host = cpal::default_host();
        let device = match name {
            Some(name) => host
                .input_devices()
                .map_err(|error| CaptureError::Device(error.to_string()))?
                .find(|device| device_name(device).as_deref() == Some(name))
                .ok_or_else(|| CaptureError::Device(format!("找不到输入设备 {name}")))?,
            None => host
                .default_input_device()
                .ok_or(CaptureError::NoInputDevice)?,
        };
        let config = device
            .default_input_config()
            .map_err(|error| CaptureError::Device(error.to_string()))?;
        let (errors, error_receiver) = std::sync::mpsc::sync_channel(ERROR_QUEUE_ITEMS);
        Ok(Self {
            device,
            config,
            stream: None,
            errors,
            error_receiver,
            dropped_errors: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            sink: None,
            overflows: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        })
    }
}

/// The device's human-readable name, or `None` when the backend cannot
/// describe it.
///
/// Deliberately **not** `device.to_string()`: cpal 0.18's `Display` impl calls
/// `description()` and maps its error to `fmt::Error`, which `ToString` turns
/// into a panic — so asking a half-disconnected device for its name could take
/// the process down. A device that cannot be described is skipped, not fatal.
///
/// The returned string is system-provided and untrusted (T-02-22): it is shown
/// in the settings page and compared for routing, and never spliced into a
/// shell command or a filesystem path.
pub fn device_name(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|description| description.name().to_string())
}

fn downmix_f32(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    data.chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

impl BlockSource for CpalSource {
    fn sample_rate(&self) -> u32 {
        self.config.sample_rate()
    }

    fn name(&self) -> Option<String> {
        device_name(&self.device)
    }

    fn start(&mut self) -> Result<Receiver<Vec<f32>>, CaptureError> {
        if self.stream.is_some() {
            return Err(CaptureError::Device("采集流已启动".to_string()));
        }
        let (sink, receiver) = CaptureSink::bounded(CAPTURE_QUEUE_BLOCKS);
        self.sink = Some(sink.clone());
        self.overflows = Arc::new(std::sync::atomic::AtomicU64::new(0));

        let config: cpal::StreamConfig = self.config.clone().into();
        let channels = config.channels.max(1) as usize;
        let errors = self.errors.clone();
        let error_drops = Arc::clone(&self.dropped_errors);
        let on_error = move |error: cpal::Error| {
            // The callback must not log (no allocation on the audio thread):
            // hand the error to the chain's own bounded channel instead — the
            // same discipline as the audio blocks, so a full queue counts a
            // drop rather than blocking the audio thread (WR-09).
            report_device_error(&errors, &error_drops, error.to_string());
        };

        let stream = match self.config.sample_format() {
            cpal::SampleFormat::F32 => {
                let sink = sink.clone();
                self.device.build_input_stream(
                    config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if channels == 1 {
                            // A borrowed device buffer: the one permitted copy.
                            sink.push(data);
                        } else {
                            // The downmix builds its own buffer — move it in
                            // rather than copying it a second time (WR-09).
                            sink.push_owned(downmix_f32(data, channels));
                        }
                    },
                    on_error,
                    Some(std::time::Duration::from_secs(5)),
                )
            }
            cpal::SampleFormat::I16 => {
                let sink = sink.clone();
                self.device.build_input_stream(
                    config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        // The conversion already builds the block — move it in.
                        let samples: Vec<f32> = data
                            .chunks(channels)
                            .map(|frame| {
                                frame.iter().map(|s| *s as f32 / 32_768.0).sum::<f32>()
                                    / frame.len() as f32
                            })
                            .collect();
                        sink.push_owned(samples);
                    },
                    on_error,
                    Some(std::time::Duration::from_secs(5)),
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
        // Dropping the stream closes the device. The sink stays: it holds the
        // overflow counter, which has to outlive the session so the diagnostics
        // can still say why the audio ended (and so a late callback counts a
        // drop instead of blocking or panicking).
        self.stream = None;
        Ok(())
    }

    fn overflows(&self) -> u64 {
        self.sink.as_ref().map(|sink| sink.overflows()).unwrap_or(0)
    }

    fn drain_errors(&mut self) -> Vec<CaptureError> {
        drain_reported_errors(&self.error_receiver, &self.dropped_errors)
    }
}

/// A scripted device: the test (and the no-device fallback) knows exactly which
/// blocks arrive and which errors the backend reports.
pub struct ScriptedSource {
    sample_rate: u32,
    blocks: VecDeque<Vec<f32>>,
    capacity: usize,
    sink: Option<CaptureSink>,
    /// Pushed at `start` — i.e. "these arrived while the consumer was busy".
    burst: bool,
    errors: VecDeque<CaptureError>,
    started: bool,
}

impl ScriptedSource {
    /// `blocks` are handed to the queue on `start`.
    pub fn new(sample_rate: u32, blocks: Vec<Vec<f32>>) -> Self {
        Self {
            sample_rate,
            blocks: blocks.into(),
            capacity: CAPTURE_QUEUE_BLOCKS,
            sink: None,
            burst: true,
            errors: VecDeque::new(),
            started: false,
        }
    }

    /// A narrower queue, so a test can overflow it without allocating 2.6 s of
    /// audio.
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity.max(1);
        self
    }

    /// Deliver blocks on `start` (default) or wait for [`Self::deliver`].
    pub fn delivered_on_start(mut self, burst: bool) -> Self {
        self.burst = burst;
        self
    }

    /// Queue one backend error, as the real error callback would.
    pub fn report_error(mut self, error: CaptureError) -> Self {
        self.errors.push_back(error);
        self
    }

    /// Push the next scripted block through the realtime sink (the
    /// no-device path's way of feeding the chain).
    pub fn deliver(&mut self) -> bool {
        let Some(block) = self.blocks.pop_front() else {
            return false;
        };
        if let Some(sink) = &self.sink {
            sink.push(&block);
        }
        true
    }

    /// Deliver every remaining block at once.
    pub fn deliver_all(&mut self) {
        while self.deliver() {}
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    /// The sink the scripted device writes through (also what the caller holds
    /// to simulate a stalled consumer).
    pub fn sink(&self) -> Option<CaptureSink> {
        self.sink.clone()
    }
}

impl BlockSource for ScriptedSource {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn start(&mut self) -> Result<Receiver<Vec<f32>>, CaptureError> {
        let (sink, receiver) = CaptureSink::bounded(self.capacity);
        self.sink = Some(sink);
        self.started = true;
        if self.burst {
            self.deliver_all();
        }
        Ok(receiver)
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        // The sink stays for the same reason as `CpalSource`'s: the overflow
        // counter is a lifetime figure, not a live-stream one.
        self.started = false;
        Ok(())
    }

    fn overflows(&self) -> u64 {
        self.sink.as_ref().map(|sink| sink.overflows()).unwrap_or(0)
    }

    fn drain_errors(&mut self) -> Vec<CaptureError> {
        self.errors.drain(..).collect()
    }
}

// ---------------------------------------------------------------------------
// the chain
// ---------------------------------------------------------------------------

/// Run one frame through the shared AEC, supplying a silent far-end reference
/// when the playout chain has not mirrored one.
///
/// The far-end reference for "nothing is playing" **is** silence, and the
/// capture path must not stall waiting for another component to tick: at
/// session start, in a muted session, or on a machine with no output device
/// there is no playout chain at all. The substitution is deliberate and
/// narrow — only [`AecError::RenderOrderViolated`] takes this path; every
/// other refusal is still the caller's structured error (T-02-23).
fn process_with_aec(processor: &SharedProcessor, frame: &[f32]) -> Result<Vec<f32>, AecError> {
    match processor.process_capture_frame(frame) {
        Err(AecError::RenderOrderViolated) => {
            processor.process_render_frame(&vec![0.0f32; frame.len()])?;
            processor.process_capture_frame(frame)
        }
        other => other,
    }
}

/// One drain of the capture chain: both consumers' feeds, from one pass.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CaptureOutput {
    /// AEC-cleaned 10 ms frames at [`GRAPH_RATE_HZ`] — what the segmenter's VAD
    /// consumes (the frame geometry `pipeline/vad.rs` assumes).
    pub frames: Vec<Vec<f32>>,
    /// The STT feed: 16 kHz mono PCM16.
    pub pcm16: Vec<i16>,
}

impl CaptureOutput {
    /// Nothing was captured this drain.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty() && self.pcm16.is_empty()
    }

    /// Milliseconds of audio captured this drain (at the graph rate — the
    /// frames are the authoritative count, the resampler only holds a
    /// sub-chunk remainder back).
    pub fn duration_ms(&self) -> u64 {
        let samples: usize = self.frames.iter().map(Vec::len).sum();
        (samples as u64) * 1_000 / GRAPH_RATE_HZ as u64
    }
}

/// Counters the diagnostics panel and the failure-case library read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CaptureStats {
    /// Device blocks pulled off the queue.
    pub blocks_received: u64,
    /// 10 ms frames that made it through the AEC.
    pub frames_processed: u64,
    /// Frames the processor refused (counted, never silent).
    pub frames_dropped: u64,
    /// Blocks the realtime discipline dropped (queue full / consumer stalled).
    pub overflows: u64,
    /// 16 kHz samples handed to the STT feed.
    pub output_samples: u64,
}

/// The session's capture chain. One per session, rebuilt when the device
/// changes (T5.4).
pub struct CaptureChain {
    source: Box<dyn BlockSource>,
    processor: SharedProcessor,
    assembler: FrameAssembler,
    resampler: StreamingResampler,
    queue: Option<Receiver<Vec<f32>>>,
    errors: VecDeque<CaptureError>,
    stats: CaptureStats,
    running: bool,
}

impl CaptureChain {
    /// Open the source and build the chain for its rate.
    ///
    /// `processor` is the session's shared AEC (the playout chain mirrors the
    /// render side into the same instance — two processors would mean two
    /// delay estimates and an echo canceller that cancels nothing).
    pub fn start(
        mut source: Box<dyn BlockSource>,
        processor: SharedProcessor,
    ) -> Result<Self, CaptureError> {
        let rate = source.sample_rate();
        let resampler = StreamingResampler::new(rate, STT_RATE_HZ, 480)
            .map_err(|error| CaptureError::Resampler(error.to_string()))?;
        let queue = source.start()?;
        Ok(Self {
            source,
            processor,
            assembler: FrameAssembler::for_processor(),
            resampler,
            queue: Some(queue),
            errors: VecDeque::new(),
            stats: CaptureStats::default(),
            running: true,
        })
    }

    /// The rate the device delivers at (before the 3:1 resample).
    pub fn device_rate_hz(&self) -> u32 {
        self.source.sample_rate()
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn stats(&self) -> CaptureStats {
        self.stats
    }

    /// The device's name when the source has one (settings surface, T5.4).
    pub fn device_name(&self) -> Option<String> {
        self.source.name()
    }

    /// Drain everything queued, run the chain once, and return both of its
    /// consumers' feeds.
    ///
    /// Both come out of one pass on purpose: the segmenter's VAD wants the
    /// AEC-cleaned 48 kHz frames, the STT stage wants 16 kHz PCM16, and running
    /// the chain twice would double-count the frame statistics and split the
    /// resampler's carry-over in two.
    ///
    /// Returns `Err` only when the chain itself cannot run; per-frame failures
    /// are counted in [`CaptureStats::frames_dropped`] and reported through
    /// [`Self::drain_errors`].
    pub fn poll(&mut self) -> Result<CaptureOutput, CaptureError> {
        if !self.running {
            return Err(CaptureError::NotRunning);
        }
        // Scope the receiver borrow: everything below mutates other fields.
        let blocks: Vec<Vec<f32>> = match &self.queue {
            Some(queue) => queue.try_iter().collect(),
            None => return Err(CaptureError::NotRunning),
        };
        self.stats.blocks_received += blocks.len() as u64;

        let mut frames: Vec<Vec<f32>> = Vec::new();
        let mut resampled: Vec<f32> = Vec::new();
        for block in blocks {
            for frame in self.assembler.push(&block) {
                match process_with_aec(&self.processor, &frame) {
                    Ok(cleaned) => {
                        self.stats.frames_processed += 1;
                        resampled.extend(self.resampler.process(&cleaned));
                        frames.push(cleaned);
                    }
                    Err(error) => {
                        // A refused frame is a real event: count it and keep the
                        // chain alive (a panic here would end the session).
                        self.stats.frames_dropped += 1;
                        self.errors.push_back(CaptureError::Processor(error));
                    }
                }
            }
        }

        // Device-side errors queued by the backend's error callback.
        for error in self.source.drain_errors() {
            self.errors.push_back(error);
        }

        let pcm16 = f32_to_pcm16(&resampled);
        self.stats.output_samples += pcm16.len() as u64;
        Ok(CaptureOutput { frames, pcm16 })
    }

    /// Frames still held by the assembler (a partial 10 ms frame).
    pub fn pending_frames(&self) -> usize {
        self.assembler.pending_samples()
    }

    /// Errors observed since the last call — device, processor and resampler
    /// failures all land here instead of a log line nobody reads.
    pub fn drain_errors(&mut self) -> Vec<CaptureError> {
        self.errors.drain(..).collect()
    }

    /// Flush the resampler's carry-over (the tail of the last take).
    pub fn flush(&mut self) -> Vec<i16> {
        f32_to_pcm16(&self.resampler.flush())
    }

    /// Stop the source and release the queue. Idempotent.
    pub fn stop(&mut self) -> Result<(), CaptureError> {
        if !self.running {
            return Ok(());
        }
        self.stats.overflows = self.source.overflows();
        self.source.stop()?;
        // Dropping the receiver also releases the queue: a late callback push
        // then counts an overflow instead of blocking.
        self.queue = None;
        self.assembler.clear();
        self.running = false;
        Ok(())
    }

    /// The caller's window into "why did the audio end".
    ///
    /// Overflows are a **lifetime** counter: the value recorded at [`Self::stop`]
    /// is what the diagnostics read afterwards, so a source whose stream is gone
    /// cannot take the number with it.
    pub fn take_source_overflows(&mut self) -> u64 {
        let live = self.source.overflows();
        self.stats.overflows = self.stats.overflows.max(live);
        self.stats.overflows
    }
}

impl std::fmt::Debug for CaptureChain {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CaptureChain")
            .field("device_rate_hz", &self.device_rate_hz())
            .field("running", &self.running)
            .field("stats", &self.stats)
            .finish()
    }
}

/// The graph rate (48 kHz), re-exported so the capture side does not have to
/// import from two modules for one constant.
pub const GRAPH_RATE_HZ: u32 = PROCESSOR_RATE_HZ;

// ---------------------------------------------------------------------------
// per-role assembly (02-05 T5.5 — the Phase 3 seam)
// ---------------------------------------------------------------------------

/// Which line a capture chain feeds.
///
/// Both are capture and both end up as 16 kHz PCM16, which is exactly why the
/// distinction has to be a type: the user's line is the one that gets cloned,
/// and the interviewer's line is the one that must never be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureLine {
    /// The user's own voice → the STT line that feeds the clone.
    User,
    /// The system's playback → the interviewer's STT 副线. Transcribed and
    /// shown; **never** written to disk or to the JSONL trace (T-02-24).
    Interviewer,
}

impl CaptureLine {
    /// The line a role feeds, or `None` for a role that does not capture.
    pub fn of(role: StreamRole) -> Option<Self> {
        match role {
            StreamRole::UserMic => Some(Self::User),
            StreamRole::Loopback => Some(Self::Interviewer),
            StreamRole::Output => None,
        }
    }

    pub fn role(self) -> StreamRole {
        match self {
            Self::User => StreamRole::UserMic,
            Self::Interviewer => StreamRole::Loopback,
        }
    }

    /// A stable machine-readable code. Never localized.
    pub fn code(self) -> &'static str {
        match self {
            Self::User => "user_line",
            Self::Interviewer => "interviewer_line",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::User => "用户线",
            Self::Interviewer => "面试官线（副线）",
        }
    }

    /// Is this the sub-line? The 副线 is consumed and dropped, never retained.
    pub fn is_sub_line(self) -> bool {
        self == Self::Interviewer
    }
}

impl CpalSource {
    /// The device the routing plan resolved for `role`.
    ///
    /// Takes the resolved handle rather than a name: [`RoutingPlan::resolve`]
    /// already matched the configured name (tolerantly, with a readable error
    /// when it is not there), and looking the name up a second time could pick
    /// a different device if the machine's list changed in between.
    ///
    /// **Phase 3 takeover point.** This phase lands the assembly only: today
    /// `role` is [`StreamRole::UserMic`] against the system default microphone,
    /// and a loopback role resolves nothing unless the profile names a device.
    /// Phase 3 is where the loopback becomes real — the guided BlackHole 2ch
    /// install (`audio::routing::LOOPBACK_DEVICE_NAME`), the "回采未启用" banner
    /// when it is missing, and the interviewer STT sub-line that consumes this
    /// chain's output. Nothing below has to change for that: the role, the
    /// plan and the line are already separated here, so Phase 3 supplies a
    /// profile and a device, not a rewrite.
    ///
    /// The device name is system-provided and untrusted (T-02-22): it is used
    /// for display and passed to cpal, never interpolated into a shell command
    /// or a path.
    pub fn for_role(role: StreamRole, plan: &RoutingPlan) -> Result<Self, CaptureError> {
        let handle = plan.device(role).ok_or_else(|| {
            CaptureError::Device(format!(
                "{}未启用：在设置中指定设备后才会启用采集",
                role.label()
            ))
        })?;
        let backend = handle
            .backend()
            .ok_or_else(|| CaptureError::Device(format!("{}不是可用的系统设备", role.label())))?;
        Self::from_backend(backend.clone())
    }

    /// Wrap a device cpal already handed us (the routing plan's own handle).
    fn from_backend(device: cpal::Device) -> Result<Self, CaptureError> {
        let config = device
            .default_input_config()
            .map_err(|error| CaptureError::Device(error.to_string()))?;
        let (errors, error_receiver) = std::sync::mpsc::sync_channel(ERROR_QUEUE_ITEMS);
        Ok(Self {
            device,
            config,
            stream: None,
            errors,
            error_receiver,
            dropped_errors: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            sink: None,
            overflows: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WR-09: the error callback obeys the sample path's contract — hand the
    /// report on or count it, but never block and never grow. A full queue
    /// (the consumer stopped draining) and a closed queue both mean "count
    /// it"; the audio thread waits for nothing.
    #[test]
    fn a_full_error_channel_counts_drops_and_never_blocks() {
        use std::sync::atomic::Ordering;
        use std::sync::mpsc::sync_channel;

        let (sender, receiver) = sync_channel::<CaptureError>(1);
        let drops = std::sync::atomic::AtomicU64::new(0);
        report_device_error(&sender, &drops, "第一条".to_string());
        report_device_error(&sender, &drops, "第二条".to_string());
        assert_eq!(
            drops.load(Ordering::Relaxed),
            1,
            "the second report is counted, not waited for"
        );
        assert_eq!(receiver.try_iter().count(), 1, "the queue kept its bound");

        drop(receiver);
        report_device_error(&sender, &drops, "第三条".to_string());
        assert_eq!(
            drops.load(Ordering::Relaxed),
            2,
            "a dead reader counts too"
        );
    }

    /// The drop count has a reader: it is reported through the same
    /// `drain_errors` surface the device errors use, and cleared as it is
    /// reported so a later session cannot inherit a stale count.
    #[test]
    fn drain_errors_reports_and_clears_the_dropped_reports() {
        use std::sync::atomic::Ordering;
        use std::sync::mpsc::sync_channel;

        let (sender, receiver) = sync_channel::<CaptureError>(1);
        let drops = std::sync::atomic::AtomicU64::new(0);
        report_device_error(&sender, &drops, "设备已断开".to_string());
        report_device_error(&sender, &drops, "设备已断开".to_string());

        let drained = drain_reported_errors(&receiver, &drops);
        assert_eq!(drained.len(), 2, "the error plus the drop report");
        assert_eq!(drained[0], CaptureError::Stream("设备已断开".to_string()));
        assert!(
            matches!(&drained[1], CaptureError::Stream(text) if text.contains('1')),
            "the drop report names how many were lost: {drained:?}"
        );
        assert_eq!(drops.load(Ordering::Relaxed), 0, "reported and cleared");
        assert!(drain_reported_errors(&receiver, &drops).is_empty());
    }
}
