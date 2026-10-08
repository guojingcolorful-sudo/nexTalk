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
//! sentence starts after the old one's final sample" (no overlap) is a fact
//! about one timeline rather than two clocks. The 02-01 `PlaybackFirstSample`
//! mark is the *other* end of that timeline: it fires when the device consumes
//! the sentence's first sample, because that is when the user starts hearing it
//! (WR-02 — see `mark_first_played`).
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
/// The only block length the echo canceller accepts (10 ms at 48 kHz): the
/// mirror frames to this, and a take at any other rate is refused, not sent
/// (CR-01).
const AEC_FRAME_SAMPLES: usize = crate::audio::aec::FRAME_SAMPLES;
/// The rate the far-end reference must be at — the AEC's own processing rate.
const MIRROR_RATE_HZ: u32 = crate::audio::capture::GRAPH_RATE_HZ;
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
    /// Mirror emissions the echo canceller did not receive (CR-01): a frame it
    /// refused, or a played take that was not at the graph rate. The canceller
    /// accepts only whole 480-sample graph-rate frames, so anything counted here
    /// is audio the far-end reference is missing — the diagnostics read it.
    pub mirror_failures: u64,
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
    /// One entry per sentence of the current generation, fixed at enqueue.
    first_positions: Vec<SegmentPosition>,
    /// The AEC mirror's frame assembler (CR-01): what the device consumed but
    /// what is not yet a whole 10 ms frame.
    mirror: MirrorFramer,
    stats: PlayoutStats,
    last_interrupt_at_ms: Option<u64>,
    marks: MarkHandle,
}

/// One sentence's place in the output timeline (WR-01/WR-02).
struct SegmentPosition {
    segment_id: u64,
    /// The write cursor at enqueue — the no-overlap comparison baseline, true
    /// before a single sample has been played.
    position: u64,
    /// Did the rig record `PlaybackFirstSample` for this sentence yet? Flipped
    /// by the first render that writes one of its samples: the mark is the
    /// stopwatch's end (budget.rs), so it must follow the device, not the
    /// enqueue.
    marked: bool,
}

/// The AEC render mirror's frame assembler (CR-01).
///
/// The canceller processes exactly [`AEC_FRAME_SAMPLES`] (480) samples at the
/// graph rate and refuses every other length, while CoreAudio hands out blocks
/// of whatever size it chose (512 frames is the usual one) and a sentence can
/// end anywhere. So the mirror carries its remainder across ticks exactly like
/// [`crate::audio::resample::StreamingResampler`] carries its own — whole frames
/// only, never a zero-padded one (the canceller would then adapt against audio
/// the user never heard).
struct MirrorFramer {
    /// Samples of a frame the device has played but that is not yet complete.
    pending: [f32; AEC_FRAME_SAMPLES],
    pending_len: usize,
}

impl MirrorFramer {
    const fn new() -> Self {
        Self {
            pending: [0.0; AEC_FRAME_SAMPLES],
            pending_len: 0,
        }
    }

    /// Drop the partial frame. A generation boundary must not splice the
    /// previous session's tail onto the next session's first frame.
    fn clear(&mut self) {
        self.pending_len = 0;
    }

    /// Feed the samples the device just consumed and hand the canceller every
    /// frame that completes. Returns how many it refused.
    ///
    /// Copies only a frame that spans two ticks; when the played block starts on
    /// a frame boundary its frames go through as borrowed slices — this runs on
    /// the audio path, so no allocation (see [`crate::audio::bounded`]).
    fn push_played(
        &mut self,
        played: &[f32],
        rate: u32,
        reference: &mut dyn RenderReference,
    ) -> u64 {
        let mut refused = 0;
        let mut rest = played;

        if self.pending_len > 0 {
            let need = AEC_FRAME_SAMPLES - self.pending_len;
            let take = need.min(rest.len());
            self.pending[self.pending_len..self.pending_len + take]
                .copy_from_slice(&rest[..take]);
            self.pending_len += take;
            rest = &rest[take..];
            if self.pending_len == AEC_FRAME_SAMPLES {
                if !reference.push_reference(&self.pending, rate) {
                    refused += 1;
                }
                self.pending_len = 0;
            }
        }

        while rest.len() >= AEC_FRAME_SAMPLES {
            let (frame, tail) = rest.split_at(AEC_FRAME_SAMPLES);
            if !reference.push_reference(frame, rate) {
                refused += 1;
            }
            rest = tail;
        }

        if !rest.is_empty() {
            self.pending[..rest.len()].copy_from_slice(rest);
            self.pending_len = rest.len();
        }
        refused
    }
}

/// Fire [`Stage::PlaybackFirstSample`] the first time one of a sentence's
/// samples is written to the device (WR-02).
///
/// Marking at enqueue charged the end-to-end number for the sentence production
/// and skipped the jitter buffer's pre-roll — up to `target_ms` of real waiting
/// the user hears. A sentence that is dropped, interrupted or superseded before
/// it plays is heard by nobody and marked by nobody.
fn mark_first_played(inner: &mut Inner, segment_id: u64) {
    let Some(entry) = inner
        .first_positions
        .iter_mut()
        .find(|entry| entry.segment_id == segment_id)
    else {
        return;
    };
    if entry.marked {
        return;
    }
    entry.marked = true;
    // `mark` is a no-op on a disabled handle (unit tests, wiring without a rig).
    inner.marks.mark(Stage::PlaybackFirstSample);
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
                mirror: MirrorFramer::new(),
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
            .find(|entry| entry.segment_id == segment_id)
            .map(|entry| entry.position)
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
        // playback. The latency mark is deliberately *not* fired here: it is
        // the rig's stopwatch end, so it belongs to the first sample the device
        // consumes (WR-02, see `mark_first_played`).
        let position = inner.cursor;
        if !inner
            .first_positions
            .iter()
            .any(|entry| entry.segment_id == segment_id)
        {
            inner.first_positions.push(SegmentPosition {
                segment_id,
                position,
                marked: false,
            });
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

    /// [`Self::render`] while mirroring the rendered samples into the echo
    /// canceller's far-end reference (02-05 T5.1 consumes this) — in whole AEC
    /// frames, the remainder carried into the next call (CR-01).
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
                    (
                        take,
                        Some((front.sample_rate_hz, front.segment_id)),
                        front.remaining() == 0,
                    )
                }
            };
            if finished {
                inner.queued.pop_front();
                if take == 0 {
                    continue;
                }
            }
            let (rate, segment_id) = block.expect("every consumed block carries its rate");
            // The last-stage boundary is consumption (WR-02): this is the first
            // sample of that sentence the device has actually taken.
            mark_first_played(&mut inner, segment_id);
            if let Some(reference) = reference.as_mut() {
                // Reborrow, so the next iteration keeps its own handle.
                let handle: &mut dyn RenderReference = &mut **reference;
                let refused = if rate == MIRROR_RATE_HZ {
                    // Whole AEC frames only, carried across ticks (CR-01).
                    inner
                        .mirror
                        .push_played(&out[written..written + take], rate, handle)
                } else {
                    // The canceller accepts graph-rate frames only; handing it a
                    // 24 kHz take would mis-date the reference, so it is refused
                    // here — and counted, never silently.
                    1
                };
                inner.stats.mirror_failures += refused;
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
        // A partial frame is audio from the generation that just ended: splicing
        // it onto the new session's first frame would hand the canceller a
        // reference the device never produced.
        inner.mirror.clear();
        // Positions belong to the generation (WR-01). Segment ids restart at 1
        // with a fresh segmenter, so a surviving table would suppress the
        // restarted session's first latency mark (the enqueue guard reads it)
        // and hand the no-overlap check a position from the old timeline.
        // The cursor itself stays monotonic: it indexes this queue's output
        // timeline, which is a fact about the queue, not about a session.
        inner.first_positions.clear();
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

// ---------------------------------------------------------------------------
// the jitter layer (02-05 T5.3)
// ---------------------------------------------------------------------------

/// Buffer depth a new segment pre-rolls to before it starts playing.
///
/// Vendor TTS arrives in bursts, not at the device's pace; playing the first
/// 40 ms the moment it lands is what turns every network hiccup into a stutter.
/// 120 ms is deep enough to ride out a burst gap and shallow enough that the
/// first syllable is not late — it comes out of the same ≤2 s budget the
/// barge-in gate spends from.
///
/// This is charging the budget honestly: the rig's `PlaybackFirstSample` mark
/// fires at device consumption (WR-02), so the pre-roll is part of the
/// end-to-end number rather than hidden in front of it.
pub const DEFAULT_TARGET_MS: u64 = 120;

/// Never buffer deeper than this (the plan's hard cap).
///
/// **A deep buffer is not safety, it is latency.** Every millisecond held here
/// is a millisecond the user waits before hearing their own sentence finish,
/// and — worse for this product — a millisecond of silence before 抢话 can
/// start. The 02-01 budget spends 2 s in total; 200 ms is the most this stage
/// may take without being the reason the budget breaks.
pub const HARD_CAP_MS: u64 = 200;

/// Below this depth the chain is one scheduling hiccup from an underrun, and
/// says so (`JitterStats::low_water_events`) instead of waiting to be told by
/// an audible gap. This is the tuning signal the failure-case library reads.
pub const LOW_WATER_MS: u64 = 60;

/// The jitter buffer's tunables, named so the failure-case library can cite
/// them (and so the ordering constraint below is checkable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JitterPolicy {
    /// Depth a new segment plays from (see [`DEFAULT_TARGET_MS`]).
    pub target_ms: u64,
    /// Backpressure ceiling (see [`HARD_CAP_MS`]).
    pub hard_cap_ms: u64,
    /// Low-water warning line (see [`LOW_WATER_MS`]).
    pub low_water_ms: u64,
    /// Whether a new segment pre-rolls. Off makes the chain a pass-through —
    /// useful when the device itself is already buffering, and the switch the
    /// failure-case library flips to reproduce a stutter.
    pub pre_roll: bool,
}

impl Default for JitterPolicy {
    fn default() -> Self {
        Self {
            target_ms: DEFAULT_TARGET_MS,
            hard_cap_ms: HARD_CAP_MS,
            low_water_ms: LOW_WATER_MS,
            pre_roll: true,
        }
    }
}

impl JitterPolicy {
    /// A policy that never pre-rolls and never caps — the degenerate case the
    /// tests use to isolate the queue behaviour underneath.
    pub const fn immediate() -> Self {
        Self {
            target_ms: 0,
            hard_cap_ms: DEFAULT_CAPACITY_MS,
            low_water_ms: 0,
            pre_roll: false,
        }
    }

    /// The plan's ordering constraint, as a value: a target deeper than the cap
    /// could never be reached, and the chain would play nothing at all.
    pub fn is_consistent(&self) -> bool {
        if !self.pre_roll {
            return true;
        }
        self.target_ms <= self.hard_cap_ms && self.low_water_ms <= self.target_ms
    }
}

/// Jitter-buffer counters (T5.3). All of them are diagnostics — none of them
/// changes what is played.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JitterStats {
    /// New segments that pre-rolled to the target depth.
    pub pre_rolls: u64,
    /// Segments that started below the target because the producer had stopped
    /// filling (a one-word sentence must not wait forever for depth).
    pub short_starts: u64,
    /// Ticks that wanted more samples than the buffer had.
    pub underruns: u64,
    /// Ticks that rendered with less than `low_water_ms` left.
    pub low_water_events: u64,
    /// Chunks refused at this chain's front door because their generation was
    /// over. Counted here rather than in the queue because the queue never saw
    /// them — they were turned away before the resampler ran.
    pub stale_chunks: u64,
    /// Chunks retired by the hard cap (forwarded from the queue).
    pub dropped_chunks: u64,
    /// Milliseconds retired by the hard cap.
    pub dropped_ms: u64,
    /// Ticks that rendered at least one sample.
    pub renders: u64,
}

/// The playout chain: the 02-03 [`PlayoutQueue`] (epoch guard, fade, bounded
/// backpressure) with the jitter layer the real device needs on top, plus the
/// AEC render mirror.
///
/// Two contracts this type exists to hold:
///
/// - **Everything reaching the device reaches the echo canceller**, from the
///   same played buffer: the mirror is fed the written prefix, re-cut into whole
///   10 ms AEC frames (the only length the processor accepts — CR-01). TTS
///   arrives at 24 kHz and the graph runs at 48 kHz, so a chunk is resampled
///   *once* on the way in; after that the samples the device consumes and the
///   samples the canceller is given are the same samples, not a second
///   conversion that could drift (T-02-25).
/// - **The mirror is what is being played, never what was received.** Mirroring
///   at `push` would hand AEC3 a reference from the future, and its delay
///   estimate would converge on a lie.
#[derive(Clone)]
pub struct PlayoutChain {
    queue: PlayoutQueue,
    policy: JitterPolicy,
    /// The 24 kHz → 48 kHz converter, built on first use and reused (a new one
    /// per chunk would drop the carry-over and click at every seam).
    resampler: Arc<Mutex<Option<(u32, crate::audio::resample::StreamingResampler)>>>,
    state: Arc<Mutex<ChainState>>,
}

struct ChainState {
    /// May the chain consume? Cleared by an interrupt, a dry buffer and a new
    /// session; set by a pre-roll.
    playing: bool,
    /// Is the device being rebuilt (T5.4)? A suspended chain emits silence and
    /// consumes nothing — the buffer holds the sentence for the device that is
    /// coming back, and it is not charged for the silence in between.
    suspended: bool,
    /// Did audio arrive since the previous tick?
    pushed_since_tick: bool,
    /// The buffer holds nothing but the interrupt fade: play it out and stop,
    /// and do **not** count the resulting gap as an underrun — the sentence
    /// ended because the user cut it, not because the producer starved.
    draining_cut: bool,
    stats: JitterStats,
    /// The queue's lifetime drop counters as they stood when this session
    /// began. The queue's numbers are deliberately lifetime-wide (02-03's
    /// diagnostics); the chain reports the session's, by difference.
    dropped_baseline_chunks: u64,
    dropped_baseline_ms: u64,
}

impl PlayoutChain {
    pub fn new() -> Self {
        Self::with_policy(JitterPolicy::default())
    }

    pub fn with_policy(policy: JitterPolicy) -> Self {
        Self::with_policy_and_clock(policy, Arc::new(RealClock::new()))
    }

    pub fn with_policy_and_clock(
        policy: JitterPolicy,
        clock: Arc<dyn TimeSource + Send + Sync>,
    ) -> Self {
        debug_assert!(
            policy.is_consistent(),
            "a pre-roll target deeper than the hard cap can never be reached"
        );
        let queue = PlayoutQueue::with_clock(
            PlayoutConfig {
                // The hard cap is enforced by the queue's own backpressure —
                // one implementation of "drop the oldest, count it", not two.
                capacity_ms: policy.hard_cap_ms,
                ..PlayoutConfig::default()
            },
            clock,
        );
        Self {
            queue,
            policy,
            resampler: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(ChainState {
                playing: false,
                suspended: false,
                pushed_since_tick: false,
                draining_cut: false,
                stats: JitterStats::default(),
                dropped_baseline_chunks: 0,
                dropped_baseline_ms: 0,
            })),
        }
    }

    pub fn policy(&self) -> JitterPolicy {
        self.policy
    }

    /// The underlying epoch-guarded queue (its stats, timeline and marks are
    /// still the authority for everything 02-03 defined).
    pub fn queue(&self) -> &PlayoutQueue {
        &self.queue
    }

    /// This session's counters. The buffer's own drop tallies are lifetime
    /// figures (02-03's diagnostics read them that way), so what is reported
    /// here is the difference since the session began — 停止 must leave the
    /// next session with nothing to inherit.
    pub fn stats(&self) -> JitterStats {
        let state = self.state();
        let queue = self.queue.stats();
        let mut stats = state.stats;
        stats.dropped_chunks = queue
            .dropped_chunks
            .saturating_sub(state.dropped_baseline_chunks);
        stats.dropped_ms = queue.dropped_ms.saturating_sub(state.dropped_baseline_ms);
        stats
    }

    pub fn epoch(&self) -> u64 {
        self.queue.epoch()
    }

    /// Unplayed milliseconds, as the device sees them.
    pub fn buffered_ms(&self) -> u64 {
        self.queue.buffered_ms()
    }

    pub fn is_playing(&self) -> bool {
        self.state().playing
    }

    /// Is the device being rebuilt (T5.4)?
    pub fn is_suspended(&self) -> bool {
        self.state().suspended
    }

    /// Hold everything and emit silence while the device is gone.
    ///
    /// A device that is not there must not be fed — the samples would be spent
    /// on it and lost — but the sentence must not be spent either. So the
    /// buffer, the epoch and the water marks all stand still, and the counters
    /// stay quiet: an outage is not an underrun the producer caused, and
    /// counting it as one would blame the wrong layer.
    ///
    /// What is deliberately *not* done here: clearing the buffer (that is
    /// [`Self::end_session`]) and dropping the resampler's carry-over (a new
    /// converter would restart the filter, and the seam would click exactly
    /// where the recovery is supposed to be seamless).
    pub fn suspend(&mut self) {
        self.state().suspended = true;
    }

    /// Release the hold: playback continues from the very next sample it
    /// would have played. Nothing was consumed and nothing was reset, so the
    /// far side of the outage continues the same waveform — a step
    /// discontinuity is what a speaker renders as a click.
    pub fn resume(&mut self) {
        self.state().suspended = false;
    }

    /// Enqueue one synthesised chunk, converted to the graph rate.
    ///
    /// The conversion happens here rather than at render time so that what the
    /// device plays and what the AEC is mirrored are the same buffer. A chunk
    /// whose generation is already over is refused (02-03's contract).
    pub fn push(
        &mut self,
        epoch: u64,
        segment_id: u64,
        pcm: &[f32],
        sample_rate_hz: u32,
    ) -> Result<(), StaleChunk> {
        if epoch != self.queue.epoch() {
            // The generation check runs before the resampler: converting audio
            // for a sentence nobody will hear spends the CPU budget the ≤2 s
            // path needs, and a stale chunk is refused *immediately* — that is
            // the whole point of 02-03's epoch guard (02-05 T5.3 Test 6).
            let current = self.queue.epoch();
            self.state().stats.stale_chunks += 1;
            return Err(StaleChunk {
                chunk_epoch: epoch,
                current_epoch: current,
            });
        }
        if pcm.is_empty() {
            return Ok(());
        }

        let converted = self.to_graph_rate(pcm, sample_rate_hz);
        let chunk = AudioChunk::new(converted, crate::audio::capture::GRAPH_RATE_HZ);
        self.queue.push(epoch, segment_id, &chunk)?;
        self.state().pushed_since_tick = true;
        Ok(())
    }

    /// Convert one chunk to the graph rate. 48 kHz passes through untouched —
    /// an unnecessary resample is an unnecessary filter.
    fn to_graph_rate(&self, pcm: &[f32], sample_rate_hz: u32) -> Vec<f32> {
        let graph = crate::audio::capture::GRAPH_RATE_HZ;
        if sample_rate_hz == graph || sample_rate_hz == 0 {
            return pcm.to_vec();
        }
        let mut guard = self
            .resampler
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let stale = !matches!(guard.as_ref(), Some((rate, _)) if *rate == sample_rate_hz);
        if stale {
            match crate::audio::resample::StreamingResampler::new(sample_rate_hz, graph, 240) {
                Ok(built) => *guard = Some((sample_rate_hz, built)),
                // A rate pair rubato refuses is a programming error, but a
                // sentence that is heard slightly wrong beats a sentence that
                // is not heard at all — and `resample_mono` is the same
                // conversion without the streaming state.
                Err(_) => return crate::audio::resample::resample_mono(pcm, sample_rate_hz, graph),
            }
        }
        // No `flush` here: a chunk boundary is not a stream boundary. Flushing
        // would convert the carry-over with zeros behind it on every chunk, and
        // the FFT window would smear that padding into the first real samples
        // of the next one. Held samples cost a few milliseconds of delay; they
        // do not cost correctness, and they reappear on the next push.
        guard
            .as_mut()
            .expect("just built or already present")
            .1
            .process(pcm)
    }

    /// Fill `out` with the next block of audio and mirror it into the AEC.
    ///
    /// Returns how many samples were written (0 while pre-rolling or idle).
    /// Everything written is graph-rate, and the mirror receives the written
    /// prefix **in whole 10 ms AEC frames** — the queue carries a partial frame
    /// across ticks ([`MirrorFramer`]) because the canceller refuses every other
    /// length (CR-01). What it knows about is therefore always audio that was
    /// really played: no padding, and never a slice from the future.
    pub fn tick_mirrored(&mut self, out: &mut [f32], reference: &mut dyn RenderReference) -> usize {
        self.tick_inner(out, Some(reference))
    }

    /// [`Self::tick_mirrored`] without the mirror (tests, and any path that
    /// deliberately runs without AEC).
    pub fn tick(&mut self, out: &mut [f32]) -> usize {
        self.tick_inner(out, None)
    }

    fn tick_inner(
        &mut self,
        out: &mut [f32],
        mut reference: Option<&mut dyn RenderReference>,
    ) -> usize {
        if out.is_empty() {
            return 0;
        }
        out.fill(0.0);

        let mut state = self.state();
        if state.suspended {
            // Silence, and nothing consumed. The block is already zeroed, so
            // what the device receives is silence rather than the last block
            // repeated — and the buffer, the marks and the counters are exactly
            // as they were.
            return 0;
        }
        if !state.playing {
            // Nothing buffered is not a gap — it is the silence between
            // sentences. Reading the water marks here would turn every pause
            // into an underrun alarm.
            let buffered = self.queue.buffered_ms();
            if buffered == 0 {
                state.pushed_since_tick = false;
                return 0;
            }
            let reached_target = buffered >= self.policy.target_ms;
            // A sentence shorter than the target must still be heard: once the
            // producer stops filling, waiting longer only delays it.
            let producer_paused = !state.pushed_since_tick;
            if !self.policy.pre_roll || reached_target || producer_paused {
                state.playing = true;
                if self.policy.pre_roll {
                    if reached_target {
                        state.stats.pre_rolls += 1;
                    } else {
                        state.stats.short_starts += 1;
                    }
                }
            } else {
                state.pushed_since_tick = false;
                return 0;
            }
        }
        state.pushed_since_tick = false;
        drop(state);

        let written = match reference.as_mut() {
            Some(reference) => self.queue.render_mirrored(out, *reference),
            None => self.queue.render(out),
        };

        let mut state = self.state();
        if written > 0 {
            state.stats.renders += 1;
        }

        if state.playing && written < out.len() {
            if state.draining_cut {
                // 抢话 drained: the fade was the last audio and the sentence
                // ended because the user ended it. Silence here is the plan
                // working, not a gap, so it is not counted as one.
                state.draining_cut = false;
                state.playing = false;
            } else {
                // Underrun: the device asked for more than the buffer had. The
                // samples that did come out are cut short, so ramp their tail
                // to exact silence — the same reasoning as the interrupt fade,
                // and the reason a stutter is heard as a stop, not a click.
                state.stats.underruns += 1;
                let fade = samples_for_ms(FADE_OUT_MS, crate::audio::capture::GRAPH_RATE_HZ);
                let start = written.saturating_sub(fade.min(written));
                apply_fade_to_silence(&mut out[start..written]);
                state.playing = false;
            }
        }

        if self.queue.buffered_ms() < self.policy.low_water_ms {
            state.stats.low_water_events += 1;
        }
        written
    }

    /// Barge-in (02-03's contract, unchanged): advance the generation, clear,
    /// keep the fade tail. The jitter layer stops consuming with it, so the AEC
    /// mirror stops at the same instant the audio does.
    pub fn interrupt(&mut self) -> InterruptOutcome {
        let outcome = self.queue.interrupt();
        let mut state = self.state();
        state.pushed_since_tick = false;
        // The queue kept a fade ramp; it still has to be heard, or the cut is a
        // click after all. So the chain stays playing just long enough to
        // drain it — and `draining_cut` says the silence that follows is the
        // interrupt's doing, not a starving producer's.
        state.draining_cut = outcome.faded_samples > 0;
        state.playing = outcome.faded_samples > 0;
        outcome
    }

    /// A session start: a new generation, an empty buffer, and counters that
    /// belong to nobody.
    pub fn begin_session(&mut self) -> u64 {
        let epoch = self.queue.begin_session();
        self.reset_state();
        epoch
    }

    /// A session stop (「停止」): clear the buffer, reset the water marks, keep
    /// nothing for the next session to inherit.
    pub fn end_session(&mut self) -> u64 {
        let epoch = self.queue.end_session();
        self.reset_state();
        epoch
    }

    fn reset_state(&self) {
        let queue = self.queue.stats();
        let mut state = self.state();
        state.playing = false;
        // A session boundary releases the rebuild hold. A chain that started a
        // session still suspended would be silent for good; the manager resumes
        // it on the next successful rebuild anyway, so clearing here can only
        // make the worse case (permanent silence) unreachable.
        state.suspended = false;
        state.pushed_since_tick = false;
        state.draining_cut = false;
        state.stats = JitterStats::default();
        // Re-base the difference counters on the queue's lifetime tallies as
        // they stand now: 停止 leaves nothing for the next session to inherit.
        state.dropped_baseline_chunks = queue.dropped_chunks;
        state.dropped_baseline_ms = queue.dropped_ms;
    }

    fn state(&self) -> MutexGuard<'_, ChainState> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

impl Default for PlayoutChain {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PlayoutChain {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PlayoutChain")
            .field("policy", &self.policy)
            .field("playing", &self.is_playing())
            .field("buffered_ms", &self.buffered_ms())
            .field("stats", &self.stats())
            .finish()
    }
}

/// The rebuild gate the device layer holds (T5.4). The chain is the thing that
/// must go quiet when the device leaves, and the thing that must come back
/// without a seam — so it is the gate, and the manager never needs to know it
/// is a playout chain at all.
impl crate::audio::device::RebuildGate for PlayoutChain {
    fn suspend(&mut self) {
        PlayoutChain::suspend(self);
    }

    fn resume(&mut self) {
        PlayoutChain::resume(self);
    }
}

/// The cascade pushes a chunk and never learns how it is played.
impl PlayoutSink for PlayoutChain {
    fn set_marks(&mut self, marks: MarkHandle) {
        self.queue.set_marks(marks);
    }

    fn play(&mut self, epoch: u64, segment_id: u64, chunk: &AudioChunk) {
        // A refusal is already counted as a stale chunk; a playout sink has no
        // way to report upward and must not stall the pipeline over it.
        let _ = self.push(epoch, segment_id, &chunk.pcm, chunk.sample_rate_hz);
    }
}

/// Ramp a block to exact zero over its own length (the tail of an underrun, or
/// whatever survives an interrupt).
fn apply_fade_to_silence(block: &mut [f32]) {
    let len = block.len();
    if len == 0 {
        return;
    }
    for (index, sample) in block.iter_mut().enumerate() {
        let gain = 1.0 - (index as f32 + 1.0) / len as f32;
        *sample *= gain;
    }
    if let Some(last) = block.last_mut() {
        *last = 0.0;
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
