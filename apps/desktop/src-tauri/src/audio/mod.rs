//! Audio I/O (02-03 T3.3; the cpal device lands in 02-05).
//!
//! This module owns the boundary between "the cascade produced synthesised
//! audio" and "the operating system played it". Two contracts live here:
//!
//! - [`PlayoutSink`] — where the cascade hands a chunk. The cascade holds one
//!   and never learns how it is played; [`playout::PlayoutQueue`] is the
//!   epoch-guarded implementation, 02-05 attaches the real CoreAudio device.
//! - [`RenderReference`] — the mirror of what was rendered, which the AEC needs
//!   as its far-end reference signal. 02-05 T5.1 wires it to
//!   `webrtc-audio-processing`; 02-03 ships the interface and drives it so the
//!   audio graph does not have to change shape later.
//!
//! `PlayoutSink` is defined here rather than in `pipeline::cascade` (where the
//! first draft put it) because the audio layer is its natural owner; the
//! cascade module re-exports it so existing imports keep working.
//!
//! 02-04 added two more residents: [`resample`] (the sample-rate boundary) and
//! [`play_pcm_blocking`] — a deliberately minimal "play this PCM through the
//! default output device" used by the enrollment preview.

pub mod aec;
pub mod bounded;
pub mod capture;
pub mod device;
pub mod playout;
pub mod resample;
pub mod routing;

use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::pipeline::stages::{AudioChunk, MarkHandle};

/// Where synthesised audio goes. 02-05 implements the consuming side with the
/// cpal ring; tests substitute a scripted sink that records `(epoch, pcm)`.
///
/// `epoch` travels with every chunk, so a sink can tell one session generation
/// or one barge-in generation from the next and drop stale audio (T3.3).
pub trait PlayoutSink: Send {
    /// Install the latency rig handle — the sink marks
    /// [`Stage::PlaybackFirstSample`](crate::pipeline::budget::Stage::PlaybackFirstSample)
    /// when a segment's first sample enters the playout timeline (02-01 contract).
    fn set_marks(&mut self, marks: MarkHandle);
    /// Hand one synthesised chunk to the playout chain.
    fn play(&mut self, epoch: u64, segment_id: u64, chunk: &AudioChunk);
}

/// The playback mirror the echo canceller consumes as its far-end reference.
///
/// **02-05 T5.1 wired the real implementation**:
/// [`aec::SharedProcessor`] is the processor the capture chain also uses, so
/// the mirror and the capture side share one delay estimate. The no-op stays
/// for the paths that deliberately run without AEC (tests, enrollment).
pub trait RenderReference: Send {
    /// Mirror one rendered block: the exact samples the device just consumed,
    /// at the rate they were rendered at.
    fn push_reference(&mut self, samples: &[f32], sample_rate_hz: u32);
}

/// The production stand-in until 02-05 attaches the AEC: renders are not
/// mirrored anywhere yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopRenderReference;

impl RenderReference for NoopRenderReference {
    fn push_reference(&mut self, _samples: &[f32], _sample_rate_hz: u32) {}
}

// ---------------------------------------------------------------------------
// Blocking PCM playback (02-04 T4.1/T4.4 — 02-05 replaces this)
// ---------------------------------------------------------------------------

/// What can go wrong on the way to the speakers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackError {
    /// No output device at all (headless CI, all devices gone).
    NoOutputDevice,
    /// The device's sample format is outside the two the minimal path handles.
    UnsupportedFormat(String),
    /// The device refused the stream.
    Device(String),
}

impl fmt::Display for PlaybackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOutputDevice => write!(formatter, "没有可用的输出设备"),
            Self::UnsupportedFormat(format) => {
                write!(formatter, "输出设备不支持该音频格式（{format}）")
            }
            Self::Device(detail) => write!(formatter, "播放失败：{detail}"),
        }
    }
}

impl std::error::Error for PlaybackError {}

/// Where a rendered PCM block goes. The enrollment preview and the tests hold
/// one of these; 02-05 binds the production implementation to the playout ring.
pub trait PcmPlayback: Send {
    fn play(&mut self, pcm16: &[i16], sample_rate_hz: u32) -> Result<(), PlaybackError>;
}

/// The default output device, written straight from the callback.
pub struct CpalPlayback;

/// Play one PCM16 block through the default output device and wait for it.
///
/// **Deliberately minimal** (02-04): open the default device, resample to its
/// rate if needed, write the block from the callback, sleep the block's
/// duration, drop the stream. The real playout ring with the epoch guard and
/// AEC reference (see [`PlayoutSink`]) lands in 02-05 — the enrollment preview
/// only ever plays one short block, so the simple path is the honest one.
pub fn play_pcm_blocking(pcm16: &[i16], sample_rate_hz: u32) -> Result<(), PlaybackError> {
    CpalPlayback.play(pcm16, sample_rate_hz)
}

struct PlaybackState {
    samples: Vec<f32>,
    cursor: Mutex<usize>,
}

impl PcmPlayback for CpalPlayback {
    fn play(&mut self, pcm16: &[i16], sample_rate_hz: u32) -> Result<(), PlaybackError> {
        if pcm16.is_empty() {
            return Ok(());
        }
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(PlaybackError::NoOutputDevice)?;
        let supported = device
            .default_output_config()
            .map_err(|error| PlaybackError::Device(error.to_string()))?;
        let device_rate = supported.sample_rate();
        let channels = supported.channels().max(1) as usize;
        let sample_format = supported.sample_format();

        // 火山 synthesises at 24 kHz mono; CoreAudio usually runs 44.1/48 kHz.
        let samples = if device_rate == sample_rate_hz {
            resample::pcm16_to_f32(pcm16)
        } else {
            resample::resample_mono(&resample::pcm16_to_f32(pcm16), sample_rate_hz, device_rate)
        };
        let frames = samples.len();
        if frames == 0 {
            return Ok(());
        }

        let state = Arc::new(PlaybackState {
            samples,
            cursor: Mutex::new(0),
        });
        let config: cpal::StreamConfig = supported.into();
        let on_error = |error: cpal::Error| eprintln!("[playback] stream error: {error}");
        // cpal's `None` means "wait indefinitely". T5.4's device layer makes
        // the bounded wait the house rule, and this path follows it: a device
        // that never answers must surface as an error, not as a hung preview.
        let timeout = Some(Duration::from_secs(5));

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let state = Arc::clone(&state);
                device.build_output_stream(
                    config,
                    move |data: &mut [f32], _| {
                        let Ok(mut cursor) = state.cursor.lock() else {
                            data.fill(0.0);
                            return;
                        };
                        for frame in data.chunks_mut(channels) {
                            let sample = state.samples.get(*cursor).copied().unwrap_or(0.0);
                            *cursor += 1;
                            for slot in frame.iter_mut() {
                                *slot = sample;
                            }
                        }
                    },
                    on_error,
                    timeout,
                )
            }
            cpal::SampleFormat::I16 => {
                let state = Arc::clone(&state);
                device.build_output_stream(
                    config,
                    move |data: &mut [i16], _| {
                        let Ok(mut cursor) = state.cursor.lock() else {
                            data.fill(0);
                            return;
                        };
                        for frame in data.chunks_mut(channels) {
                            let sample = state.samples.get(*cursor).copied().unwrap_or(0.0);
                            *cursor += 1;
                            let pcm = (sample * i16::MAX as f32) as i16;
                            for slot in frame.iter_mut() {
                                *slot = pcm;
                            }
                        }
                    },
                    on_error,
                    timeout,
                )
            }
            other => return Err(PlaybackError::UnsupportedFormat(format!("{other:?}"))),
        }
        .map_err(|error| PlaybackError::Device(error.to_string()))?;

        stream
            .play()
            .map_err(|error| PlaybackError::Device(error.to_string()))?;
        // Wait out the block plus a small tail so the device drains before the
        // stream is dropped.
        let seconds = frames as f64 / device_rate as f64;
        std::thread::sleep(Duration::from_secs_f64(seconds + 0.15));
        drop(stream);
        Ok(())
    }
}
