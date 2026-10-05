//! Stability gate (02-03 T3.3): the barge-in playout queue.
//!
//! 抢话 is the one behaviour that must never glitch: when the user starts
//! speaking over the cloned voice, the old audio has to be gone within a
//! blink, faded instead of cut (a hard cut is an audible click), and the new
//! sentence must never overlap the old one. Three separate mechanisms do that
//! and each has its own test here:
//!
//! 1. the **epoch guard** — [`PlayoutQueue::interrupt`] advances the generation
//!    *before* it clears, so audio still in flight from the old generation is
//!    refused on arrival instead of being mistaken for the new one;
//! 2. the **fade** — the retained tail ramps to zero, so nothing pops;
//! 3. the **minimum-speech gate** — without AEC (02-05) the microphone hears
//!    the speakers, and a gate-less interrupt loop would cut the user's own
//!    answer apart.
//!
//! Zero network, zero keys, no `sleep`: every timing assertion reads an
//! injected clock or a counter.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nextalk_desktop_lib::audio::playout::{
    should_interrupt, PlayoutConfig, PlayoutQueue, FADE_OUT_MS, MIN_INTERRUPT_SPEECH_MS,
};
use nextalk_desktop_lib::audio::{PlayoutSink, RenderReference};
use nextalk_desktop_lib::pipeline::budget::Stage;
use nextalk_desktop_lib::pipeline::stages::{AudioChunk, MarkHandle};
use nextalk_desktop_lib::sim::source::TimeSource;
use nextalk_desktop_lib::state::SessionState;

const RATE: u32 = 48_000;

/// A constant-amplitude chunk — a constant signal makes the fade's monotone
/// ramp directly assertable.
fn loud_chunk(ms: u64) -> AudioChunk {
    let samples = (RATE as u64 * ms / 1_000) as usize;
    AudioChunk::new(vec![0.8; samples], RATE)
}

fn samples_for(ms: u64) -> usize {
    (RATE as u64 * ms / 1_000) as usize
}

fn rendered_ms(samples: usize) -> u64 {
    samples as u64 * 1_000 / RATE as u64
}

/// The scripted clock the queue stamps interrupts with (never wall time).
struct StepClock {
    next_ms: AtomicU64,
    step_ms: u64,
}

impl StepClock {
    fn new(step_ms: u64) -> Self {
        Self {
            next_ms: AtomicU64::new(0),
            step_ms,
        }
    }
}

impl TimeSource for StepClock {
    fn elapsed_ms(&self) -> u64 {
        self.next_ms.fetch_add(self.step_ms, Ordering::SeqCst)
    }
}

// ---------------------------------------------------------------- interrupt ---

/// Test 1: an interrupt while the queue is mid-sentence drops everything not
/// yet played except a short fade, and that fade converges to zero — no click.
#[test]
fn interrupt_drops_unplayed_playout_audio_and_fades_the_tail() {
    let mut queue = PlayoutQueue::new();
    let epoch = queue.epoch();
    queue
        .push(epoch, 1, &loud_chunk(200))
        .expect("a fresh chunk is admitted");

    // The device consumed 10 ms of the sentence when the user cut in.
    let mut out = vec![0.0f32; 480];
    assert_eq!(queue.render(&mut out), 480);
    assert_eq!(queue.buffered_ms(), 190);

    let outcome = queue.interrupt();
    assert!(outcome.dropped_chunks >= 1, "{outcome:?}");
    assert_eq!(outcome.faded_samples, samples_for(FADE_OUT_MS));
    assert_eq!(
        outcome.dropped_ms,
        190 - FADE_OUT_MS,
        "everything but the fade was dropped"
    );
    assert_eq!(queue.buffered_ms(), FADE_OUT_MS);
    assert!(
        queue.buffered_ms() < 10,
        "only the fade tail may survive an interrupt: {}ms",
        queue.buffered_ms()
    );

    // The survivor is the fade: monotone to zero, last sample exactly silent.
    let mut tail = vec![0.0f32; 4_096];
    let written = queue.render(&mut tail);
    assert_eq!(written, outcome.faded_samples, "the fade is what is left");
    assert!(written > 0, "an interrupt keeps a fade, it does not hard-cut");
    let tail = &tail[..written];
    for pair in tail.windows(2) {
        assert!(
            pair[1].abs() <= pair[0].abs() + f32::EPSILON,
            "the fade must converge monotonically: {pair:?}"
        );
    }
    assert_eq!(
        tail.last().copied().unwrap_or(1.0),
        0.0,
        "the fade lands on silence — the click-free cut"
    );
    assert_eq!(queue.buffered_ms(), 0);

    // A second render finds nothing: the interrupt really cleared the queue.
    assert_eq!(queue.render(&mut tail), 0);
}

/// Test 2: `interrupt()` advances the epoch **before** it clears the buffer.
/// Reversed, an enqueue racing the interrupt keeps the old epoch and survives
/// into the new generation — the "old audio continues as the new sentence" bug.
#[test]
fn interrupt_advances_epoch_before_clearing_playout() {
    let mut queue = PlayoutQueue::new();
    let before = queue.epoch();
    queue
        .push(before, 1, &loud_chunk(200))
        .expect("a fresh chunk is admitted");
    let buffered_before = queue.buffered_ms();

    let mut epoch_seen_during_clear = 0;
    let mut buffered_seen_during_clear = 0;
    let outcome = queue.interrupt_observed(|| {
        // Runs after the epoch advanced, before the buffer is cleared.
        epoch_seen_during_clear = queue.epoch();
        buffered_seen_during_clear = queue.buffered_ms();
    });

    assert_eq!(
        epoch_seen_during_clear,
        before + 1,
        "the epoch must already have moved when the clear begins"
    );
    assert_eq!(
        buffered_seen_during_clear, buffered_before,
        "…and the buffer must still be intact at that instant"
    );
    assert_eq!(outcome.epoch, before + 1);
    assert!(queue.buffered_ms() < buffered_before, "the clear happened");

    // Audio stamped with the old epoch is refused on arrival; the new one lands.
    let stale = queue
        .push(before, 2, &loud_chunk(50))
        .expect_err("old-epoch audio must be refused");
    assert_eq!(stale.chunk_epoch, before);
    assert_eq!(stale.current_epoch, before + 1);
    assert_eq!(queue.stats().stale_chunks, 1);
    queue
        .push(before + 1, 3, &loud_chunk(50))
        .expect("new-epoch audio is admitted");
    assert!(queue.buffered_ms() >= 50);
}

/// Test 3: after an interrupt the next sentence's first rendered sample comes
/// after the last sample of the one that was cut — playback never overlaps.
#[test]
fn playout_after_interrupt_never_overlaps() {
    let mut queue = PlayoutQueue::new();
    let marks: Arc<Mutex<Vec<Stage>>> = Arc::new(Mutex::new(Vec::new()));
    queue.set_marks(MarkHandle::new({
        let marks = Arc::clone(&marks);
        move |stage| marks.lock().unwrap().push(stage)
    }));

    let epoch = queue.epoch();
    queue.push(epoch, 1, &loud_chunk(100)).expect("admitted");
    let mut out = vec![0.0f32; 240]; // 5 ms consumed
    assert_eq!(queue.render(&mut out), 240);

    queue.interrupt();
    let mut fade = vec![0.0f32; 4_096];
    let fade_samples = queue.render(&mut fade) as u64;
    let old_last = queue.rendered_samples();
    assert_eq!(old_last, 240 + fade_samples);
    assert_eq!(
        queue.first_sample_position(1),
        Some(0),
        "the cut sentence started at position 0"
    );

    queue
        .push(queue.epoch(), 2, &loud_chunk(50))
        .expect("the new sentence is admitted");
    let new_first = queue
        .first_sample_position(2)
        .expect("the new sentence rendered");
    assert_eq!(
        new_first, old_last,
        "the new sentence starts after the old one's final sample (no overlap)"
    );
    assert!(
        queue.rendered_samples() > new_first,
        "the new sentence is actually playing"
    );
    assert_eq!(
        marks.lock().unwrap().as_slice(),
        [Stage::PlaybackFirstSample, Stage::PlaybackFirstSample],
        "one first-sample mark per sentence, in order"
    );
}

/// Test 4: the minimum-speech gate. Below it, a cough or the speakers' own
/// echo must not cut the sentence (there is no AEC before 02-05); at/above it,
/// the interrupt fires.
#[test]
fn interrupt_requires_the_minimum_speech_gate() {
    let config = PlayoutConfig::default();
    assert!(
        (300..=350).contains(&MIN_INTERRUPT_SPEECH_MS),
        "the gate lives in the 300–350ms window (got {MIN_INTERRUPT_SPEECH_MS})"
    );
    assert!(!should_interrupt(299, &config), "a cough is not a barge-in");
    assert!(
        !should_interrupt(300, &config),
        "300ms is still under the gate"
    );
    assert!(
        should_interrupt(350, &config),
        "350ms of speech is a real interruption"
    );

    // The gate is configurable: a tuning experiment can move it without a
    // second code path.
    let tuned = PlayoutConfig {
        min_interrupt_speech_ms: 300,
        ..PlayoutConfig::default()
    };
    assert!(should_interrupt(300, &tuned));

    // The queue stamps interrupts with the injected clock (no wall time).
    let mut queue = PlayoutQueue::with_clock(PlayoutConfig::default(), Arc::new(StepClock::new(240)));
    queue
        .push(queue.epoch(), 1, &loud_chunk(50))
        .expect("admitted");
    let outcome = queue.interrupt();
    assert_eq!(queue.last_interrupt_at_ms(), Some(outcome.at_ms));
    assert_eq!(outcome.at_ms, 0, "the first clock read is the stamp");
    let second = queue.interrupt();
    assert_eq!(second.at_ms, 240, "the clock advances per read");
}

// ----------------------------------------------------------- session bounds ---

/// Test 5: the session lifecycle owns the queue — 停止 clears it and moves the
/// epoch, so a restart can never mix the previous session's audio in.
#[test]
fn playout_queue_clears_on_session_stop_and_isolates_sessions() {
    let state = SessionState::new(8787);
    let mut playout = state.playout().clone();
    let first_epoch = playout.epoch();
    playout
        .push(first_epoch, 1, &loud_chunk(100))
        .expect("admitted");
    assert!(playout.buffered_ms() > 0);

    state.stop_session();
    assert_eq!(playout.buffered_ms(), 0, "停止 clears the playout queue");
    assert!(
        playout.epoch() > first_epoch,
        "停止 advances the playout epoch ({} -> {})",
        first_epoch,
        playout.epoch()
    );

    // An old-session chunk arriving after the stop is refused outright…
    playout
        .push(first_epoch, 2, &loud_chunk(100))
        .expect_err("previous-session audio must not enter the new session");
    assert_eq!(playout.buffered_ms(), 0);
    // …while the current generation plays normally.
    playout
        .push(playout.epoch(), 3, &loud_chunk(100))
        .expect("current-session audio is admitted");
    assert!(playout.buffered_ms() > 0);

    // Restarting a session clears again (the old sentence dies with the stop).
    state.start_session().expect("restart");
    assert_eq!(playout.buffered_ms(), 0);
}

/// Test 6: the queue is bounded. A producer that outruns the device loses the
/// *oldest* unplayed audio and never blocks, and the loss is counted (a silent
/// drop would look like a vendor failure downstream).
#[test]
fn playout_queue_is_bounded_and_drops_the_oldest() {
    let mut queue = PlayoutQueue::with_config(PlayoutConfig {
        capacity_ms: 100,
        ..PlayoutConfig::default()
    });
    let epoch = queue.epoch();
    for _ in 0..10 {
        queue
            .push(epoch, 1, &loud_chunk(50))
            .expect("the producer never blocks — over-capacity is a drop, not a stall");
    }

    assert!(
        queue.buffered_ms() <= 100,
        "the queue must stay bounded: {}ms",
        queue.buffered_ms()
    );
    assert_eq!(queue.buffered_chunks(), 2, "100ms of 50ms chunks");
    assert_eq!(
        queue.stats().dropped_chunks,
        8,
        "every chunk past the capacity was dropped, oldest first"
    );
    assert!(queue.stats().dropped_ms >= 400);

    // What is left is the NEWEST audio: rendering yields the last two chunks.
    let mut out = vec![0.0f32; 8_192];
    let written = queue.render(&mut out);
    assert_eq!(rendered_ms(written), 100);
}

/// The AEC reference mirror (02-05 T5.1) is driven by every render — the
/// interface exists now so the audio graph does not have to change shape when
/// echo cancellation lands.
#[test]
fn playout_render_mirrors_into_the_reference_path() {
    #[derive(Default)]
    struct Recorder {
        blocks: Vec<(usize, u32)>,
    }
    impl RenderReference for Recorder {
        fn push_reference(&mut self, samples: &[f32], sample_rate_hz: u32) {
            self.blocks.push((samples.len(), sample_rate_hz));
        }
    }

    let mut queue = PlayoutQueue::new();
    queue
        .push(queue.epoch(), 1, &loud_chunk(10))
        .expect("admitted");
    let mut recorder = Recorder::default();
    let mut out = vec![0.0f32; 480];
    assert_eq!(queue.render_mirrored(&mut out, &mut recorder), 480);
    assert_eq!(
        recorder.blocks,
        vec![(480, RATE)],
        "every rendered block is mirrored to the AEC reference"
    );
}
