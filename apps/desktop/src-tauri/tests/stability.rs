//! Stability gate (02-03 T3.3–T3.6): barge-in playout, fragment retry, the
//! per-vendor circuit breaker, silent abstention and the D-04 numeric check.
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
//! The second half (T3.4/T3.5) covers the failure path: a fragment retries on
//! D-09's 100/200 ms schedule inside its 500 ms budget, a provider with two
//! consecutive bad fragments is cut off by its own breaker (D-10, two-minute
//! window), and an exhausted fragment degrades to the original Chinese — no
//! silence, no fabricated English, no vendor message in the trace.
//!
//! Zero network, zero keys, no `sleep`: every timing assertion reads an
//! injected clock or a counter.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nextalk_desktop_lib::audio::playout::{
    should_interrupt, PlayoutConfig, PlayoutQueue, FADE_OUT_MS, MIN_INTERRUPT_SPEECH_MS,
};
use nextalk_desktop_lib::audio::{PlayoutSink, RenderReference};
use nextalk_desktop_lib::lan::server::{
    ConfidenceSource as WireConfidenceSource, ServerEvent, Speaker, SubtitleTrace,
};
use nextalk_desktop_lib::pipeline::breaker::{
    BreakerState, RetryConfig, RetryPolicy, WaitMode, BREAKER_OPEN_MS,
    MAX_RETRY_FRAGMENT_CONCURRENCY,
};
use nextalk_desktop_lib::pipeline::budget::Stage;
use nextalk_desktop_lib::pipeline::cascade::{Cascade, CascadeConfig, SegmentScript, SharedClock};
use nextalk_desktop_lib::pipeline::confidence::ConfidenceSource;
use nextalk_desktop_lib::pipeline::stages::{
    AbstainReason, AudioChunk, MarkHandle, ScriptedStt, ScriptedTranslator, ScriptedTts,
    StageError, SttPartial, TokenUsage, TranslatorEvent,
};
use nextalk_desktop_lib::pipeline::validate::{
    MismatchReason, ValidationResult, NUMERIC_MISMATCH_CODE,
};
use nextalk_desktop_lib::sim::source::TimeSource;
use nextalk_desktop_lib::state::SessionState;
use nextalk_desktop_lib::trace::jsonl::{SegmentStatus, TraceRecord};

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
    assert!(
        written > 0,
        "an interrupt keeps a fade, it does not hard-cut"
    );
    let fade = &tail[..written];
    for pair in fade.windows(2) {
        assert!(
            pair[1].abs() <= pair[0].abs() + f32::EPSILON,
            "the fade must converge monotonically: {pair:?}"
        );
    }
    assert_eq!(
        fade.last().copied().unwrap_or(1.0),
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
    let mut queue =
        PlayoutQueue::with_clock(PlayoutConfig::default(), Arc::new(StepClock::new(240)));
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

/// WR-01: a generation reset clears the position table with the buffer. Segment
/// ids restart at 1 with a fresh segmenter, so a surviving table would suppress
/// the restarted session's first latency mark and hand its no-overlap check a
/// position from the previous timeline.
#[test]
fn playout_a_new_generation_inherits_no_positions_and_no_suppressed_mark() {
    let mut queue = PlayoutQueue::new();
    let marks: Arc<Mutex<Vec<Stage>>> = Arc::new(Mutex::new(Vec::new()));
    queue.set_marks(MarkHandle::new({
        let marks = Arc::clone(&marks);
        move |stage| marks.lock().unwrap().push(stage)
    }));

    // Generation 1: segment 1 plays to its end.
    let epoch = queue.epoch();
    queue.push(epoch, 1, &loud_chunk(20)).expect("admitted");
    let mut out = vec![0.0f32; samples_for(20)];
    assert_eq!(queue.render(&mut out), samples_for(20));
    assert_eq!(queue.first_sample_position(1), Some(0), "recorded at enqueue");
    let played = queue.rendered_samples();
    assert_eq!(played, samples_for(20) as u64);

    // 停止, then a fresh session whose segmenter starts at 1 again.
    queue.end_session();
    let epoch = queue.epoch();
    queue
        .push(epoch, 1, &loud_chunk(20))
        .expect("the new generation admits its own segment 1");
    assert_eq!(
        queue.first_sample_position(1),
        Some(played),
        "the restarted session's segment 1 sits in this generation's timeline"
    );

    // The old entry must not suppress the new session's first-sample mark.
    let mut out = vec![0.0f32; samples_for(5)];
    assert_eq!(queue.render(&mut out), samples_for(5));
    assert_eq!(
        marks.lock().unwrap().as_slice(),
        [Stage::PlaybackFirstSample, Stage::PlaybackFirstSample],
        "one first-sample mark per generation"
    );
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
        fn push_reference(&mut self, samples: &[f32], sample_rate_hz: u32) -> bool {
            self.blocks.push((samples.len(), sample_rate_hz));
            true
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

// ------------------------------- fragment retry + circuit breaker (T3.4) ---

const FRAME_MS: u64 = 10;
const FRAME_SAMPLES: usize = 480; // 10 ms @ 48 kHz

/// A 10 ms frame of a steady 440 Hz sine at `amplitude` — the segmenter's VAD
/// needs real energy, not silence.
fn sine_frame(amplitude: f32) -> Vec<f32> {
    (0..FRAME_SAMPLES)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            (2.0 * std::f32::consts::PI * 440.0 * t).sin() * amplitude
        })
        .collect()
}

/// `ms` of speech as a `SegmentScript` for `epoch` — one fragment's audio.
fn speech_script(epoch: u64, ms: u64) -> SegmentScript {
    let frames = (0..ms / FRAME_MS)
        .map(|i| (i * FRAME_MS, sine_frame(0.3)))
        .collect();
    SegmentScript { epoch, frames }
}

/// One committed final — the gate's unit of admission.
fn committed_final(text: &str) -> SttPartial {
    let mut partial = SttPartial::without_confidence("scripted", "scripted-1", text);
    partial.is_final = true;
    partial.committed = true;
    partial
}

/// A scripted translation request that fails with a retryable transport error.
fn failing_script() -> Vec<TranslatorEvent> {
    vec![TranslatorEvent::Failed(StageError::transport(
        "scripted",
        "scripted transport failure",
    ))]
}

/// The always-failing translator: every call replays the same failure.
fn always_failing_translator() -> ScriptedTranslator {
    ScriptedTranslator::new(failing_script())
}

/// The credential-failure translator: a 401 is a client error (D-06).
fn failing_auth_translator() -> ScriptedTranslator {
    ScriptedTranslator::new(vec![TranslatorEvent::Failed(StageError::http(
        "scripted", 401,
    ))])
}

/// A successful translation request — one final fragment plus its usage.
fn succeed_script(text: &str) -> Vec<TranslatorEvent> {
    vec![
        TranslatorEvent::Fragment {
            text: text.to_string(),
            final_flag: true,
            provider: "scripted".to_string(),
            model_version: "scripted-1".to_string(),
        },
        TranslatorEvent::Usage(TokenUsage {
            prompt_tokens: 2,
            completion_tokens: 3,
        }),
    ]
}

/// A clock the test moves by hand — the breaker's two-minute window must not
/// cost two minutes of real time (no `sleep`, ever).
#[derive(Clone, Default)]
struct ManualClock(Arc<AtomicU64>);

impl ManualClock {
    fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl TimeSource for ManualClock {
    fn elapsed_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// The retry-cascade builder: injected clock, immediate waits (the schedule is
/// asserted from the ledger, never slept through) and a playout queue whose
/// session generation is already open — so a degraded fragment that (wrongly)
/// played would land in the queue and fail the no-audio assertion.
fn retry_cascade(
    stt: ScriptedStt,
    translator: ScriptedTranslator,
    policy: RetryPolicy,
    clock: SharedClock,
) -> (Cascade<ScriptedStt>, PlayoutQueue) {
    let playout = PlayoutQueue::new();
    let cascade = Cascade::from_source(
        stt,
        translator.into(),
        ScriptedTts::default().into(),
        playout.clone(),
        clock,
    )
    .with_config(CascadeConfig {
        retry: RetryConfig {
            policy,
            wait: WaitMode::Immediate,
        },
        ..CascadeConfig::default()
    });
    (cascade, playout)
}

/// Test 8 (D-09, signature): a fragment that keeps failing is retried on the
/// 100/200 ms schedule, gives up inside the 500 ms budget, and never reaches
/// TTS — the ledger proves both the schedule and the silence.
#[tokio::test]
async fn retry_retries_twice_with_the_injected_backoff_schedule() {
    let stt = ScriptedStt::new(vec![committed_final("这句话重试了两次")]);
    let (mut cascade, playout) = retry_cascade(
        stt,
        always_failing_translator(),
        RetryPolicy::default(),
        SharedClock::new(StepClock::new(240)),
    );
    let epoch = playout.begin_session();
    let outcomes = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();

    assert_eq!(outcomes.len(), 1);
    let degraded = outcomes[0]
        .degraded
        .as_ref()
        .expect("the fragment degraded");
    assert_eq!(degraded.error_code, "retry_exhausted");
    assert_eq!(degraded.attempts, 3, "the first call plus two retries");
    assert_eq!(degraded.waited_ms, 300, "100 + 200 ms of planned backoff");

    let ledger = cascade.ledger();
    assert_eq!(ledger.translator_calls, 3);
    let waits: Vec<(u32, u64)> = ledger
        .retry_waits
        .iter()
        .map(|wait| (wait.attempt, wait.delay_ms))
        .collect();
    assert_eq!(
        waits,
        vec![(1, 100), (2, 200)],
        "D-09's schedule, one wait per retry, in order"
    );
    for wait in &ledger.retry_waits {
        assert_eq!(wait.segment_id, outcomes[0].segment.id);
        assert_eq!(
            wait.at_ms % 240,
            0,
            "the wait is stamped with the injected clock, not wall time"
        );
    }
    assert!(
        ledger.retry_waits[1].at_ms > ledger.retry_waits[0].at_ms,
        "the second wait is stamped later than the first"
    );

    // Nothing was fabricated and nothing was spoken.
    assert!(!outcomes[0].translated);
    assert_eq!(outcomes[0].tts_calls, 0);
    assert_eq!(
        ledger.translated.len(),
        1,
        "the fragment reached the vendor three times; nothing usable came back"
    );
    assert!(
        ledger.tts_inputs.is_empty(),
        "the invariant witness is clean"
    );
    assert_eq!(
        playout.buffered_chunks(),
        0,
        "a degraded fragment makes no sound"
    );
    assert_eq!(
        cascade.breaker_state("scripted"),
        BreakerState::Closed,
        "one bad fragment cannot trip the breaker (threshold 2)"
    );
}

/// Test 9 (D-09 budget): the budget is the other way to give up — with 250 ms
/// of headroom the 200 ms wait no longer fits, so the fragment stops after two
/// calls, and the next fragment's budget starts fresh.
#[tokio::test]
async fn retry_gives_up_when_the_budget_is_gone_and_keeps_going() {
    let stt = ScriptedStt::new(vec![committed_final("第一句"), committed_final("第二句")]);
    let (mut cascade, playout) = retry_cascade(
        stt,
        always_failing_translator(),
        RetryPolicy {
            max_attempts: 2,
            backoff_ms: [100, 200],
            budget_ms: 250,
        },
        SharedClock::new(StepClock::new(240)),
    );
    let epoch = playout.begin_session();
    let outcomes = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();

    assert_eq!(outcomes.len(), 2, "both fragments closed");
    for outcome in &outcomes {
        let degraded = outcome.degraded.as_ref().expect("both fragments degraded");
        assert_eq!(degraded.error_code, "retry_budget_exhausted");
        assert_eq!(degraded.attempts, 2, "the 200 ms wait never fit the budget");
        assert_eq!(degraded.waited_ms, 100, "only the 100 ms wait was spent");
    }
    assert_eq!(
        cascade.ledger().translator_calls,
        4,
        "each fragment spent its own budget — no cross-fragment bleed"
    );
}

/// Test 10 (GOV-12): the retry scope is exactly one fragment. Two failing
/// fragments each get the full schedule — no shared counter, no bleed — and
/// only one fragment is ever in flight.
#[tokio::test]
async fn retry_scope_stays_one_fragment() {
    let stt = ScriptedStt::new(vec![committed_final("第一句"), committed_final("第二句")]);
    let (mut cascade, playout) = retry_cascade(
        stt,
        always_failing_translator(),
        RetryPolicy::default(),
        SharedClock::new(StepClock::new(240)),
    );
    let epoch = playout.begin_session();
    let outcomes = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();

    assert_eq!(outcomes.len(), 2);
    let first = outcomes[0].segment.id;
    let second = outcomes[1].segment.id;
    let waits: Vec<(u64, u32, u64)> = cascade
        .ledger()
        .retry_waits
        .iter()
        .map(|wait| (wait.segment_id, wait.attempt, wait.delay_ms))
        .collect();
    assert_eq!(
        waits,
        vec![
            (first, 1, 100),
            (first, 2, 200),
            (second, 1, 100),
            (second, 2, 200),
        ],
        "each fragment owns its schedule, grouped and in order"
    );
    assert_eq!(cascade.ledger().translator_calls, 6, "3 calls per fragment");
    assert_eq!(
        MAX_RETRY_FRAGMENT_CONCURRENCY, 1,
        "GOV-12: retries are serialised — one fragment in flight"
    );
}

/// Test 11 (D-06): a 401 is a client error — one call, no waits, no breaker
/// strike. Retrying a credential failure only burns latency, and enough of
/// them must still never open the circuit.
#[tokio::test]
async fn retry_client_errors_skip_the_budget() {
    let stt = ScriptedStt::new(vec![
        committed_final("第一句"),
        committed_final("第二句"),
        committed_final("第三句"),
    ]);
    let (mut cascade, playout) = retry_cascade(
        stt,
        failing_auth_translator(),
        RetryPolicy::default(),
        SharedClock::new(StepClock::new(240)),
    );
    let epoch = playout.begin_session();
    let outcomes = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();

    assert_eq!(outcomes.len(), 3);
    for outcome in &outcomes {
        let degraded = outcome.degraded.as_ref().expect("degraded");
        assert_eq!(degraded.error_code, "client_error");
        assert_eq!(degraded.attempts, 1, "a 401 is not retried");
    }
    assert_eq!(
        cascade.ledger().translator_calls,
        3,
        "one call per fragment — no retry, no budget spend"
    );
    assert!(cascade.ledger().retry_waits.is_empty());
    assert_eq!(
        cascade.breaker_state("scripted"),
        BreakerState::Closed,
        "client errors are the user's problem, not the vendor's: never a strike"
    );
}

/// Test 12 (D-10): two bad fragments open the vendor's circuit — the third
/// fragment is refused without a single translator call (`circuit_open`), and
/// nothing is spoken. The other stages keep running: only the vendor is cut.
#[tokio::test]
async fn retry_stops_calling_when_the_breaker_opens() {
    let stt = ScriptedStt::new(vec![
        committed_final("第一句"),
        committed_final("第二句"),
        committed_final("第三句"),
    ]);
    let (mut cascade, playout) = retry_cascade(
        stt,
        always_failing_translator(),
        RetryPolicy::default(),
        SharedClock::new(StepClock::new(240)),
    );
    let epoch = playout.begin_session();
    let outcomes = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();

    assert_eq!(outcomes.len(), 3);
    assert_eq!(
        outcomes[0].degraded.as_ref().unwrap().error_code,
        "retry_exhausted"
    );
    assert_eq!(
        outcomes[1].degraded.as_ref().unwrap().error_code,
        "retry_exhausted"
    );
    let refused = outcomes[2]
        .degraded
        .as_ref()
        .expect("the third fragment degraded");
    assert_eq!(refused.error_code, "circuit_open");
    assert_eq!(
        refused.attempts, 0,
        "the open breaker refuses the call outright"
    );
    assert_eq!(
        cascade.ledger().translator_calls,
        6,
        "two fragments × three calls; the third never called"
    );
    assert!(matches!(
        cascade.breaker_state("scripted"),
        BreakerState::Open { .. }
    ));
    assert_eq!(playout.buffered_chunks(), 0);
}

/// Test 13 (D-10 half-open): after the open window elapses the next fragment
/// is the single probe. It succeeds, the breaker closes, and the following
/// fragment translates normally — the degraded era leaves no residue.
#[tokio::test]
async fn retry_recovers_through_the_half_open_probe() {
    let stt = ScriptedStt::new(vec![committed_final("第一句"), committed_final("第二句")]);
    let translator = ScriptedTranslator::per_call(vec![
        failing_script(),
        failing_script(),
        failing_script(), // fragment 1: exhausted
        failing_script(),
        failing_script(),
        failing_script(), // fragment 2: exhausted — opens the circuit
        succeed_script("recovered one"),
        succeed_script("recovered two"),
    ]);
    let clock = ManualClock::default();
    let (mut cascade, playout) = retry_cascade(
        stt,
        translator,
        RetryPolicy::default(),
        SharedClock::new(clock.clone()),
    );
    let epoch = playout.begin_session();
    let first = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();
    assert_eq!(first.len(), 2);
    assert!(first.iter().all(|outcome| outcome.degraded.is_some()));
    assert!(
        matches!(cascade.breaker_state("scripted"), BreakerState::Open { .. }),
        "two strikes opened the circuit"
    );

    // The open window passes (no sleeping: the test moves the clock).
    clock.advance(BREAKER_OPEN_MS + 1);
    let epoch = playout.begin_session();
    let second = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();

    assert_eq!(second.len(), 2);
    assert!(
        second.iter().all(|outcome| outcome.degraded.is_none()),
        "the degraded form is gone: {second:?}"
    );
    assert!(
        second.iter().all(|outcome| outcome.translated),
        "the probe's success restored translation"
    );
    assert_eq!(cascade.breaker_state("scripted"), BreakerState::Closed);
    assert_eq!(
        cascade.ledger().translator_calls,
        8,
        "six failures, then one probe and one normal call"
    );
    assert!(
        playout.buffered_chunks() > 0,
        "recovered fragments speak again"
    );
}

/// A closed user sentence carrying the per-segment provenance the cascade
/// attaches — the shape the state bridge forwards to the trace writer (T3.7).
fn traced_sentence(error_code: Option<String>) -> ServerEvent {
    ServerEvent::Subtitle {
        id: "u1".into(),
        speaker: Speaker::User,
        seq: 1,
        zh: Some("这句话进了轨迹".into()),
        en: Some("This sentence reached the trace.".into()),
        final_flag: true,
        confidence: None,
        trace: Some(SubtitleTrace {
            segment_start_ms: 300,
            term_hits: vec![],
            provider: "volc".into(),
            model_version: "icl-2.0".into(),
            confidence_source: WireConfidenceSource::Proxy,
            error_code,
        }),
    }
}

/// Test 14 (GOV-10/D-07): the degradation reaches the trace as data — the
/// aggregatable error code in `errorCode`, the status in `status` — never as a
/// vendor message blob, and never as a confidence mark on the subtitle
/// (GOV-01/02, 2026-09-30 revision: no subtitle confidence badges in Phase 2).
#[tokio::test]
async fn retry_exhaustion_records_the_error_code_for_the_trace() {
    let stt = ScriptedStt::new(vec![committed_final("这句话进了轨迹")]);
    let (mut cascade, playout) = retry_cascade(
        stt,
        always_failing_translator(),
        RetryPolicy::default(),
        SharedClock::new(StepClock::new(240)),
    );
    let epoch = playout.begin_session();
    let outcomes = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();

    let degraded = outcomes[0].degraded.as_ref().expect("degraded");
    let record = TraceRecord::from_event(
        "session-trace",
        1_791_158_400_000,
        &traced_sentence(Some(degraded.error_code.clone())),
    )
    .expect("a closed sentence produces a record");
    assert_eq!(record.status, SegmentStatus::Degraded);
    assert_eq!(record.provider, "volc");
    let line = record
        .to_json_line()
        .expect("the degraded record serializes");
    assert!(line.contains("\"errorCode\":\"retry_exhausted\""), "{line}");
    assert!(line.contains("\"status\":\"degraded\""), "{line}");
    assert!(
        !line.contains("scripted transport failure"),
        "the trace carries the code, not the message blob: {line}"
    );

    // The healthy shape stays representable too — one record per sentence,
    // with the provenance the segment actually carried.
    let ok = TraceRecord::from_event("session-trace", 1_791_158_400_001, &traced_sentence(None))
        .expect("a clean sentence produces a record");
    assert_eq!(ok.status, SegmentStatus::Ok);
    assert_eq!(ok.error_code, None);
    let ok_line = ok.to_json_line().expect("ok serializes");
    assert!(ok_line.contains("\"status\":\"ok\""), "{ok_line}");
    assert!(ok_line.contains("\"errorCode\":null"), "{ok_line}");
}

// ---------------------- silent abstention + D-04 numeric check (T3.6) ---

/// Test 15 (D-03): the only normal-path abstention is a closed segment with no
/// valid text — nothing is translated, nothing is spoken, and the verdict
/// rides on the outcome so T3.6 can emit the `abstained` event (both surfaces
/// show 「待翻译」). No confidence reading exists that could cause this.
#[tokio::test]
async fn abstention_fires_only_when_there_is_no_valid_text() {
    let stt = ScriptedStt::new(vec![committed_final("   ")]);
    let (mut cascade, playout) = retry_cascade(
        stt,
        ScriptedTranslator::one_fragment("never used"),
        RetryPolicy::default(),
        SharedClock::new(StepClock::new(240)),
    );
    let epoch = playout.begin_session();
    let outcomes = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();

    assert_eq!(outcomes.len(), 1, "the blank segment still closed");
    assert_eq!(
        outcomes[0].abstained,
        Some(AbstainReason::Unrecognized),
        "speech was present; nothing recognisable came out of it"
    );
    assert!(!outcomes[0].translated, "no vendor call, no translation");
    assert_eq!(outcomes[0].tts_calls, 0, "an abstention makes no sound");
    assert_eq!(
        outcomes[0].validation, None,
        "no candidate ever reached the check"
    );
    assert!(outcomes[0].degraded.is_none(), "abstention is not an error");

    let ledger = cascade.ledger();
    assert_eq!(ledger.empty_segments, vec![outcomes[0].segment.id]);
    assert_eq!(ledger.translator_calls, 0, "nothing was sent to the vendor");
    assert!(ledger.tts_inputs.is_empty());
    assert_eq!(playout.buffered_chunks(), 0);
}

/// Test 16 (D-03): a translator-declared abstention is a decided outcome, not
/// a failure — carried on the outcome and the ledger, never spoken, and never
/// rendered in the degraded form (the vendor answered, with a refusal).
#[tokio::test]
async fn translator_declared_abstention_is_carried_on_the_outcome() {
    let stt = ScriptedStt::new(vec![committed_final("嗯……")]);
    let translator = ScriptedTranslator::new(vec![TranslatorEvent::Abstained {
        reason: AbstainReason::SilentAudio,
    }]);
    let (mut cascade, playout) = retry_cascade(
        stt,
        translator,
        RetryPolicy::default(),
        SharedClock::new(StepClock::new(240)),
    );
    let epoch = playout.begin_session();
    let outcomes = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();

    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].abstained, Some(AbstainReason::SilentAudio));
    assert!(!outcomes[0].translated, "a refusal is not a translation");
    assert_eq!(outcomes[0].tts_calls, 0);
    assert!(outcomes[0].degraded.is_none(), "the vendor did not fail");
    assert_eq!(cascade.ledger().abstained, vec![AbstainReason::SilentAudio]);
    assert!(cascade.ledger().tts_inputs.is_empty());
    assert_eq!(playout.buffered_chunks(), 0);
}

/// Test 17 (D-03, 2026-09-30 revision): confidence is provenance, never control
/// flow — a 0.05 vendor score does not abstain, does not degrade, and does not
/// stop the answer from speaking. (No subtitle badge: the number only enters
/// the JSONL trace.)
#[tokio::test]
async fn low_confidence_never_abstains() {
    let mut partial = committed_final("查询用了 800 毫秒");
    partial.confidence = Some(0.05);
    partial.confidence_source = ConfidenceSource::Vendor;
    let stt = ScriptedStt::new(vec![partial]);
    let translator = ScriptedTranslator::one_fragment("The query took 800 ms.");
    let (mut cascade, playout) = retry_cascade(
        stt,
        translator,
        RetryPolicy::default(),
        SharedClock::new(StepClock::new(240)),
    );
    let epoch = playout.begin_session();
    let outcomes = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();

    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].abstained, None);
    assert!(outcomes[0].translated);
    assert_eq!(outcomes[0].tts_calls, 1, "low confidence still speaks");
    assert_eq!(
        outcomes[0].validation,
        Some(ValidationResult::Match),
        "800 毫秒 == 800 ms: the deterministic check passes"
    );
    assert!(outcomes[0].degraded.is_none());
    assert_eq!(
        cascade.ledger().tts_inputs,
        vec!["The query took 800 ms.".to_string()]
    );
    assert!(playout.buffered_chunks() > 0, "the answer is audible");
}

/// Test 18 (GOV-04/D-04): a number that drifts is the one silent lie the
/// cascade could still tell — a candidate whose numbers do not survive
/// translation is withheld at the single TTS exit and shown as the original
/// (the locked degraded form, code `numeric_mismatch`).
#[tokio::test]
async fn numeric_mismatch_falls_back_to_the_original() {
    let stt = ScriptedStt::new(vec![committed_final("查询用了 800 毫秒")]);
    let translator = ScriptedTranslator::one_fragment("The query took 900 ms.");
    let (mut cascade, playout) = retry_cascade(
        stt,
        translator,
        RetryPolicy::default(),
        SharedClock::new(StepClock::new(240)),
    );
    let epoch = playout.begin_session();
    let outcomes = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();

    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].translated, "the call itself succeeded");
    assert_eq!(
        outcomes[0].tts_calls, 0,
        "the mismatched English is withheld"
    );
    assert_eq!(
        outcomes[0].validation,
        Some(ValidationResult::Mismatch {
            reason: MismatchReason::NumberDrift,
            expected: "800 毫秒".to_string(),
            found: "900 ms".to_string(),
        })
    );
    let degraded = outcomes[0]
        .degraded
        .as_ref()
        .expect("the fallback renders the original in the degraded form");
    assert_eq!(degraded.error_code, NUMERIC_MISMATCH_CODE);
    assert_eq!(NUMERIC_MISMATCH_CODE, "numeric_mismatch");

    let ledger = cascade.ledger();
    assert_eq!(
        ledger.validation_checks, 1,
        "the candidate was checked once"
    );
    assert_eq!(ledger.validation_rejections, vec![outcomes[0].segment.id]);
    assert!(ledger.tts_inputs.is_empty(), "nothing reached TTS");
    assert_eq!(playout.buffered_chunks(), 0);
}

/// Test 19 (GOV-04 coverage): every candidate that reaches the single TTS exit
/// passes the check exactly once — the ledger equation has no remainder, so no
/// translation can bypass the layer.
#[tokio::test]
async fn numeric_validation_checks_every_spoken_candidate() {
    let stt = ScriptedStt::new(vec![
        committed_final("第一句：这个查询用了 800 毫秒"),
        committed_final("第二句：没有数字"),
    ]);
    let translator = ScriptedTranslator::per_call(vec![
        succeed_script("The query took 900 ms."),
        succeed_script("No digits here at all."),
    ]);
    let (mut cascade, playout) = retry_cascade(
        stt,
        translator,
        RetryPolicy::default(),
        SharedClock::new(StepClock::new(240)),
    );
    let epoch = playout.begin_session();
    let outcomes = cascade
        .drive_user_track(&speech_script(epoch, 300))
        .await
        .unwrap();

    assert_eq!(outcomes.len(), 2);
    assert!(matches!(
        outcomes[0].validation,
        Some(ValidationResult::Mismatch {
            reason: MismatchReason::NumberDrift,
            ..
        })
    ));
    assert_eq!(outcomes[1].validation, Some(ValidationResult::Match));

    let ledger = cascade.ledger();
    assert_eq!(ledger.translator_calls, 2);
    assert_eq!(ledger.validation_rejections, vec![outcomes[0].segment.id]);
    assert_eq!(
        ledger.tts_inputs,
        vec!["No digits here at all.".to_string()],
        "only the validated candidate was spoken"
    );
    assert_eq!(
        ledger.validation_checks as usize,
        ledger.tts_inputs.len() + ledger.validation_rejections.len(),
        "GOV-04 witness: checks = spoken + withheld — no candidate bypasses the layer"
    );
    assert!(playout.buffered_chunks() > 0);
}
