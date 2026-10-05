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

pub mod playout;

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
/// **02-05 T5.1 takes this over**: today the only implementations are the
/// no-op (production, until AEC lands) and a recorder in tests. Keeping the
/// call site alive now means turning AEC on is a substitution, not a rewrite
/// of the audio graph.
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
