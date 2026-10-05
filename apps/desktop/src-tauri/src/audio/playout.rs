//! The barge-in playout queue (02-03 T3.3) — the last thing between the
//! cloned voice and the speaker, and the one place 抢话 must never glitch.
//!
//! Three mechanisms do the work, each with its own contract:
//!
//! 1. **The epoch guard.** Every chunk carries the generation it was produced
//!    for. [`PlayoutQueue::interrupt`] advances the generation **before** it
//!    clears the buffer: reversed, an enqueue racing the interrupt would keep
//!    the old epoch and survive into the new generation as "the old sentence
//!    continuing". [`PlayoutQueue::push`] refuses old-generation audio.
//! 2. **The fade.** An interrupt keeps at most [`FADE_OUT_MS`] of the in-flight
//!    tail, ramped to *exact* silence — a hard cut is an audible click, and a
//!    click is more noticeable than the interruption itself.
//! 3. **The minimum-speech gate** ([`should_interrupt`]). Before the AEC lands
//!    (02-05) the microphone hears the speakers; without a gate, the tail of
//!    the user's own answer would interrupt itself, sentence after sentence.
//!
//! The queue also owns the write cursor: [`PlayoutQueue::first_sample_position`]
//! fixes each sentence's place in the output timeline at enqueue, so "the new
//! sentence starts after the old one's final sample" (no overlap) and "the
//! first sample of this sentence" (the 02-01 `PlaybackFirstSample` mark) are
//! facts about one timeline rather than two clocks.
//!
//! Time is always injected ([`TimeSource`]); nothing here sleeps or reads wall
//! time, so tests stay deterministic.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::audio::{PlayoutSink, RenderReference};
use crate::pipeline::budget::Stage;
use crate::pipeline::stages::{AudioChunk, MarkHandle};
use crate::sim::source::{RealClock, TimeSource};

/// How much unplayed audio the queue holds before it starts dropping the
/// oldest chunk. 3 s is far more than any vendor burst and far less than a
/// memory leak (D-06: bounded by construction).
pub const DEFAULT_CAPACITY_MS: u64 = 3_000;
/// Length of the ramp that turns an interrupt into silence instead of a click.
pub const FADE_OUT_MS: u64 = 5;
/// Speech milliseconds required before barge-in may cut the sentence
/// (research correction 3: the 300–350 ms window; the VAD's onset is 0 ms, so
/// the gate lives here, not in the detector).
pub const MIN_INTERRUPT_SPEECH_MS: u64 = 320;

/// Tunables, named so the failure-case library can cite them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayoutConfig {
    /// Ceiling on buffered audio before the oldest chunk is dropped.
    pub capacity_ms: u64,
    /// Ramp length applied to whatever survives an interrupt.
    pub fade_out_ms: u64,
    /// Gate for [`should_interrupt`].
    pub min_interrupt_speech_ms: u64,
}

impl Default for PlayoutConfig {
    fn default() -> Self {
        Self {
            capacity_ms: DEFAULT_CAPACITY_MS,
            fade_out_ms: FADE_OUT_MS,
            min_interrupt_speech_ms: MIN_INTERRUPT_SPEECH_MS,
        }
    }
}

/// The barge-in gate: has the user been speaking long enough to mean it?
pub fn should_interrupt(speech_ms: u64, config: &PlayoutConfig) -> bool {
    speech_ms >= config.min_interrupt_speech_ms
}

/// A chunk built for a generation that is already over. Refused, not queued —
/// and counted ([`PlayoutStats::stale_chunks`]), because a stale chunk arriving
/// late is a real event worth seeing in diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleChunk {
    /// The generation the chunk was produced for.
    pub chunk_epoch: u64,
    /// The generation the queue is on now.
    pub current_epoch: u64,
}

/// What one interrupt did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InterruptOutcome {
    /// The generation the queue moved to.
    pub epoch: u64,
    /// Injected-clock stamp of the interruption.
    pub at_ms: u64,
    /// How many queued chunks the interrupt retired.
    pub dropped_chunks: usize,
    /// Unplayed milliseconds that will never be heard.
    pub dropped_ms: u64,
    /// Samples of the surviving fade ramp.
    pub faded_samples: usize,
}

/// Counters the console and the failure-case library read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayoutStats {
    /// Chunks refused because their generation was over.
    pub stale_chunks: u64,
    /// Chunks retired by backpressure (the oldest unplayed audio).
    pub dropped_chunks: u64,
    /// Milliseconds retired by backpressure.
    pub dropped_ms: u64,
    /// Chunks admitted to the timeline.
    pub accepted_chunks: u64,
    /// Interrupts served.
    pub interrupts: u64,
}

/// One queued chunk plus the read cursor inside it (the device may stop
/// mid-chunk; the remainder is what still counts).
struct QueuedChunk {
    epoch: u64,
    segment_id: u64,
    sample_rate_hz: u32,
    pcm: Vec<f32>,
    cursor: usize,
}

impl QueuedChunk {
    fn remaining(&self) -> usize {
        self.pcm.len() - self.cursor
    }

    fn remaining_ms(&self) -> u64 {
        duration_ms(self.remaining(), self.sample_rate_hz)
    }
}

struct Inner {
    queued: VecDeque<QueuedChunk>,
    /// Unplayed milliseconds, maintained incrementally (the queue's own unit).
    buffered_ms: u64,
    /// The write cursor: samples played *plus* samples queued ahead of the
    /// device. Dropped audio is subtracted — it never reaches the speaker.
    cursor: u64,
    /// Samples actually handed to the device.
    played: u64,
    /// `(segment_id, cursor)` of each sentence's first sample, fixed at enqueue.
    first_positions: Vec<(u64, u64)>,
    stats: PlayoutStats,
    last_interrupt_at_ms: Option<u64>,
    marks: MarkHandle,
}

/// Bounded, epoch-guarded, barge-in-safe playout buffer.
///
/// Cheaply clonable: every clone shares the same buffer, cursor and epoch, so
/// the audio thread, the cascade and the session lifecycle all address one
/// queue (the 02-05 device plays a clone while the cascade fills another).
#[derive(Clone)]
pub struct PlayoutQueue {
    inner: Arc<Mutex<Inner>>,
    epoch: Arc<AtomicU64>,
    config: PlayoutConfig,
    clock: Arc<dyn TimeSource + Send + Sync>,
}

impl PlayoutQueue {
    /// Default config, real monotonic clock.
    pub fn new() -> Self {
        Self::with_config(PlayoutConfig::default())
    }

    pub fn with_config(config: PlayoutConfig) -> Self {
        Self::with_clock(config, Arc::new(RealClock::new()))
    }

    /// Injected-clock constructor (tests; also lets 02-05 share the session's
    /// clock with the device).
    pub fn with_clock(config: PlayoutConfig, clock: Arc<dyn TimeSource + Send + Sync>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                queued: VecDeque::new(),
                buffered_ms: 0,
                cursor: 0,
                played: 0,
                first_positions: Vec::new(),
                stats: PlayoutStats::default(),
                last_interrupt_at_ms: None,
                marks: MarkHandle::disabled(),
            })),
            epoch: Arc::new(AtomicU64::new(0)),
            config,
            clock,
        }
    }

    pub fn config(&self) -> PlayoutConfig {
        self.config
    }

    /// The current generation. Audio stamped with anything else is refused.
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// Unplayed milliseconds.
    pub fn buffered_ms(&self) -> u64 {
        self.lock().buffered_ms
    }

    /// Number of chunks waiting for the device.
    pub fn buffered_chunks(&self) -> usize {
        self.lock().queued.len()
    }

    /// The write cursor: samples committed to the playout timeline (played
    /// plus queued). This — not a wall clock — is what the no-overlap invariant
    /// compares two sentences against.
    pub fn rendered_samples(&self) -> u64 {
        self.lock().cursor
    }

    /// Samples actually handed to the device.
    pub fn played_samples(&self) -> u64 {
        self.lock().played
    }

    /// Where `segment_id`'s first sample sits in the output timeline. Recorded
    /// at enqueue, so two sentences can be compared before either plays.
    pub fn first_sample_position(&self, segment_id: u64) -> Option<u64> {
        self.lock()
            .first_positions
            .iter()
            .find(|(id, _)| *id == segment_id)
            .map(|(_, position)| *position)
    }

    pub fn stats(&self) -> PlayoutStats {
        self.lock().stats
    }

    pub fn last_interrupt_at_ms(&self) -> Option<u64> {
        self.lock().last_interrupt_at_ms
    }

    /// Enqueue one synthesised chunk. Refused (with the two generations) when
    /// `epoch` is over. Never blocks: over capacity, the *oldest* unplayed
    /// chunk is dropped and counted, because a producer that stalls on a full
    /// queue is a producer that misses the 2 s budget.
    pub fn push(
        &mut self,
        epoch: u64,
        segment_id: u64,
        chunk: &AudioChunk,
    ) -> Result<(), StaleChunk> {
        let current = self.epoch();
        if epoch != current {
            self.lock().stats.stale_chunks += 1;
            return Err(StaleChunk {
                chunk_epoch: epoch,
                current_epoch: current,
            });
        }
        if chunk.pcm.is_empty() {
            return Ok(());
        }

        let mut inner = self.lock();
        // The sentence's place in the output timeline is fixed the moment it is
        // accepted — that is what makes the overlap check possible before
        // playback, and it is the last-stage boundary the 02-01 rig records.
        let position = inner.cursor;
        if !inner
            .first_positions
            .iter()
            .any(|(id, _)| *id == segment_id)
        {
            inner.first_positions.push((segment_id, position));
            inner.marks.mark(Stage::PlaybackFirstSample);
        }

        let len = chunk.pcm.len();
        inner.cursor += len as u64;
        inner.buffered_ms += duration_ms(len, chunk.sample_rate_hz);
        inner.stats.accepted_chunks += 1;
        inner.queued.push_back(QueuedChunk {
            epoch,
            segment_id,
            sample_rate_hz: chunk.sample_rate_hz,
            pcm: chunk.pcm.clone(),
            cursor: 0,
        });

        // Backpressure: drop the oldest chunk that has not started playing.
        // A partially played head is never dropped — cutting it mid-sample is
        // the glitch this queue exists to prevent.
        while inner.buffered_ms > self.config.capacity_ms && inner.queued.len() > 1 {
            let Some(index) = inner.queued.iter().position(|queued| queued.cursor == 0) else {
                break;
            };
            let dropped = inner
                .queued
                .remove(index)
                .expect("index came from position");
            let dropped_ms = duration_ms(dropped.pcm.len(), dropped.sample_rate_hz);
            inner.cursor -= dropped.pcm.len() as u64;
            inner.buffered_ms -= dropped_ms;
            inner.stats.dropped_chunks += 1;
            inner.stats.dropped_ms += dropped_ms;
        }
        Ok(())
    }

    /// Barge-in: advance the generation, then clear. See the module docs for
    /// why that order is not a detail.
    pub fn interrupt(&self) -> InterruptOutcome {
        self.interrupt_observed(|| {})
    }

    /// [`Self::interrupt`] with a probe that runs *between* the two steps —
    /// the seam the ordering test observes (production callers use `interrupt`).
    pub fn interrupt_observed(&self, after_epoch_bump: impl FnOnce()) -> InterruptOutcome {
        // 1. The generation moves first: a chunk still in flight from the old
        //    generation is refused the moment it arrives.
        let epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        after_epoch_bump();

        let mut inner = self.lock();
        let at_ms = self.clock.elapsed_ms();

        let buffered_ms: u64 = inner.queued.iter().map(|chunk| chunk.remaining_ms()).sum();
        let buffered_samples: u64 = inner
            .queued
            .iter()
            .map(|chunk| chunk.remaining() as u64)
            .sum();

        // 2. Ramp the in-flight tail to exact silence: the cut is heard as a
        //    stop, not as a click.
        let fade: Option<QueuedChunk> = inner.queued.front().and_then(|head| {
            let tail = &head.pcm[head.cursor..];
            if tail.is_empty() {
                return None;
            }
            let fade_samples =
                samples_for_ms(self.config.fade_out_ms, head.sample_rate_hz).min(tail.len());
            let pcm: Vec<f32> = tail[..fade_samples]
                .iter()
                .enumerate()
                .map(|(index, sample)| {
                    let gain = 1.0 - (index as f32 + 1.0) / fade_samples as f32;
                    sample * gain
                })
                .collect();
            Some(QueuedChunk {
                epoch: head.epoch,
                segment_id: head.segment_id,
                sample_rate_hz: head.sample_rate_hz,
                pcm,
                cursor: 0,
            })
        });
        let fade_samples = fade.as_ref().map_or(0, |chunk| chunk.pcm.len());
        let fade_ms = fade.as_ref().map_or(0, |chunk| {
            duration_ms(chunk.pcm.len(), chunk.sample_rate_hz)
        });

        // 3. Everything else is gone.
        let dropped_chunks = inner.queued.len();
        inner.queued.clear();
        let dropped_ms = buffered_ms.saturating_sub(fade_ms);
        let dropped_samples = buffered_samples.saturating_sub(fade_samples as u64);
        inner.cursor -= dropped_samples;
        inner.buffered_ms = fade_ms;
        if let Some(fade) = fade {
            inner.queued.push_back(fade);
        }
        inner.stats.dropped_chunks += dropped_chunks as u64;
        inner.stats.dropped_ms += dropped_ms;
        inner.stats.interrupts += 1;
        inner.last_interrupt_at_ms = Some(at_ms);

        InterruptOutcome {
            epoch,
            at_ms,
            dropped_chunks,
            dropped_ms,
            faded_samples: fade_samples,
        }
    }

    /// Copy up to `out.len()` samples into `out`. Returns how many were written
    /// (0 when idle). The AEC reference is *not* fed — see `render_mirrored`.
    pub fn render(&self, out: &mut [f32]) -> usize {
        self.render_inner(out, None)
    }

    /// [`Self::render`] while mirroring every rendered block into the echo
    /// canceller's far-end reference (02-05 T5.1 consumes this).
    pub fn render_mirrored(&self, out: &mut [f32], reference: &mut dyn RenderReference) -> usize {
        self.render_inner(out, Some(reference))
    }

    fn render_inner(
        &self,
        out: &mut [f32],
        mut reference: Option<&mut dyn RenderReference>,
    ) -> usize {
        let mut inner = self.lock();
        let mut written = 0usize;
        while written < out.len() {
            let (take, block, finished) = {
                let Some(front) = inner.queued.front_mut() else {
                    break;
                };
                let remaining = front.remaining();
                let take = remaining.min(out.len() - written);
                if take == 0 {
                    // Defensive: an aborted chunk leaves a zero-length head.
                    (0usize, None, true)
                } else {
                    let start = front.cursor;
                    out[written..written + take].copy_from_slice(&front.pcm[start..start + take]);
                    front.cursor += take;
                    (take, Some(front.sample_rate_hz), front.remaining() == 0)
                }
            };
            if finished {
                inner.queued.pop_front();
                if take == 0 {
                    continue;
                }
            }
            let rate = block.expect("every consumed block carries its rate");
            if let Some(reference) = reference.as_mut() {
                reference.push_reference(&out[written..written + take], rate);
            }
            inner.buffered_ms = inner.buffered_ms.saturating_sub(duration_ms(take, rate));
            written += take;
        }
        inner.played += written as u64;
        written
    }

    /// A new generation for a new session: clear and move the epoch (a session
    /// restart must never mix the previous session's audio in).
    pub fn begin_session(&self) -> u64 {
        self.new_generation()
    }

    /// The session ended (`停止`): clear and move the epoch, for the same
    /// reason as [`Self::begin_session`].
    pub fn end_session(&self) -> u64 {
        self.new_generation()
    }

    fn new_generation(&self) -> u64 {
        let epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let mut inner = self.lock();
        inner.queued.clear();
        inner.buffered_ms = 0;
        epoch
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A poisoned lock must not silence the user's voice: the queue's data
        // is plain audio, so recovering the guard is strictly better than
        // panicking on the audio path.
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

impl Default for PlayoutQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// The cascade hands chunks here and never learns how they are played.
impl PlayoutSink for PlayoutQueue {
    fn set_marks(&mut self, marks: MarkHandle) {
        self.lock().marks = marks;
    }

    fn play(&mut self, epoch: u64, segment_id: u64, chunk: &AudioChunk) {
        // A refusal is already counted in `stale_chunks`; a playout sink has no
        // way to report it upward and must not stall the pipeline over it.
        let _ = self.push(epoch, segment_id, chunk);
    }
}

fn duration_ms(samples: usize, sample_rate_hz: u32) -> u64 {
    if sample_rate_hz == 0 {
        return 0;
    }
    samples as u64 * 1_000 / sample_rate_hz as u64
}

fn samples_for_ms(ms: u64, sample_rate_hz: u32) -> usize {
    (ms * sample_rate_hz as u64 / 1_000) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(ms: u64, rate: u32) -> AudioChunk {
        AudioChunk::new(vec![0.5; samples_for_ms(ms, rate)], rate)
    }

    #[test]
    fn capacity_keeps_the_newest_audio() {
        let mut queue = PlayoutQueue::with_config(PlayoutConfig {
            capacity_ms: 100,
            ..PlayoutConfig::default()
        });
        let epoch = queue.epoch();
        for _ in 0..4 {
            queue.push(epoch, 1, &chunk(50, 48_000)).expect("admitted");
        }
        assert_eq!(queue.buffered_ms(), 100);
        assert_eq!(queue.buffered_chunks(), 2);
        assert_eq!(queue.stats().dropped_chunks, 2);
    }

    #[test]
    fn interrupt_keeps_only_the_fade() {
        let mut queue = PlayoutQueue::new();
        queue.push(queue.epoch(), 1, &chunk(200, 48_000)).unwrap();
        let outcome = queue.interrupt();
        assert_eq!(outcome.faded_samples, samples_for_ms(FADE_OUT_MS, 48_000));
        assert_eq!(outcome.dropped_ms, 195);
        assert_eq!(queue.buffered_ms(), FADE_OUT_MS);
    }

    #[test]
    fn gate_is_configurable() {
        let config = PlayoutConfig::default();
        assert!(!should_interrupt(MIN_INTERRUPT_SPEECH_MS - 1, &config));
        assert!(should_interrupt(MIN_INTERRUPT_SPEECH_MS, &config));
    }

    #[test]
    fn rate_change_is_reflected_in_the_fade() {
        // 24 kHz: 5 ms is 120 samples, half of the 48 kHz case.
        let mut queue = PlayoutQueue::new();
        queue.push(queue.epoch(), 1, &chunk(100, 24_000)).unwrap();
        let outcome = queue.interrupt();
        assert_eq!(outcome.faded_samples, 120);
    }
}
