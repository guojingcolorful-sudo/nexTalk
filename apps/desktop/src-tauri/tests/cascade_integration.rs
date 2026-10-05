//! Cascade integration (02-03 T3.1+T3.2): the commit gate, sentence
//! aggregation, the local energy VAD, and the per-segment latency marks.
//!
//! The signature test is [`poisoned_partials_never_reach_tts`] — GOV-15's
//! "spoken English ⊆ committed finals" invariant. Everything else here covers
//! the four segment-close paths, the energy VAD's classification, the
//! interviewer line's structural exclusion from TTS, and the waterfall mark
//! completeness the 02-01 rig demands.
//!
//! Zero network, zero keys: every stage is a scripted double from 02-02.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nextalk_desktop_lib::pipeline::budget::{assert_within_budget, Stage};
use nextalk_desktop_lib::pipeline::cascade::{
    Cascade, InterviewerCascade, PlayoutSink, SegmentScript, SharedClock,
};
use nextalk_desktop_lib::pipeline::segment::{CloseReason, SegmentConfig};
use nextalk_desktop_lib::pipeline::stages::{
    AudioChunk, MarkHandle, ScriptedStt, ScriptedTranslator, ScriptedTts, StageError, SttEvent,
    SttPartial, SttSource, SttStream,
};
use nextalk_desktop_lib::sim::source::TimeSource;

// ------------------------------------------------------------------- helpers ---

const FRAME_MS: u64 = 10;
const FRAME_SAMPLES: usize = 480; // 10 ms @ 48 kHz

/// Deterministic step clock: every read advances by a fixed step. Injected so
/// the latency assertions never touch wall time (`sleep` is banned in tests).
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

/// The scripted playout double: records what was played and marks the first
/// sample of each segment's playback run (the 02-01 `PlaybackFirstSample`
/// boundary the real ring produces in 02-05).
#[derive(Clone, Default)]
struct ScriptedPlayout {
    played: Arc<Mutex<Vec<(u64, u64, usize)>>>,
    marks: Option<MarkHandle>,
    /// Which segment's playback run is in flight — shared across clones so the
    /// "first sample of this sentence" notion belongs to the stream, not to a
    /// clone of the double.
    playing_segment: Arc<Mutex<Option<u64>>>,
}

impl ScriptedPlayout {
    fn played(&self) -> Vec<(u64, u64, usize)> {
        self.played.lock().unwrap().clone()
    }
}

impl PlayoutSink for ScriptedPlayout {
    fn set_marks(&mut self, marks: MarkHandle) {
        self.marks = Some(marks);
    }

    fn play(&mut self, epoch: u64, segment_id: u64, chunk: &AudioChunk) {
        let mark_first = {
            let mut playing = self.playing_segment.lock().unwrap();
            if *playing == Some(segment_id) {
                false
            } else {
                *playing = Some(segment_id);
                true
            }
        };
        if mark_first {
            if let Some(marks) = &self.marks {
                marks.mark(Stage::PlaybackFirstSample);
            }
        }
        self.played
            .lock()
            .unwrap()
            .push((epoch, segment_id, chunk.samples()));
    }
}

/// A 10 ms frame of a steady 440 Hz sine at `amplitude`.
fn sine_frame(amplitude: f32) -> Vec<f32> {
    (0..FRAME_SAMPLES)
        .map(|i| {
            let t = i as f32 / 48_000.0;
            (2.0 * std::f32::consts::PI * 440.0 * t).sin() * amplitude
        })
        .collect()
}

fn silence_frame() -> Vec<f32> {
    vec![0.0; FRAME_SAMPLES]
}

/// Deterministic broadband-ish noise: alternating ±amplitude (AC energy only).
fn noise_frame(amplitude: f32) -> Vec<f32> {
    (0..FRAME_SAMPLES)
        .map(|i| if i % 2 == 0 { amplitude } else { -amplitude })
        .collect()
}

/// Builds a `(at_ms, frame)` audio timeline from a frame generator.
fn timeline(
    start_ms: u64,
    frames: usize,
    make: impl Fn(usize) -> Vec<f32>,
) -> Vec<(u64, Vec<f32>)> {
    (0..frames)
        .map(|i| (start_ms + i as u64 * FRAME_MS, make(i)))
        .collect()
}

fn speech_frames(start_ms: u64, ms: u64) -> Vec<(u64, Vec<f32>)> {
    timeline(start_ms, (ms / FRAME_MS) as usize, |_| sine_frame(0.3))
}

fn silence_frames(start_ms: u64, ms: u64) -> Vec<(u64, Vec<f32>)> {
    timeline(start_ms, (ms / FRAME_MS) as usize, |_| silence_frame())
}

fn concat(mut a: Vec<(u64, Vec<f32>)>, b: Vec<(u64, Vec<f32>)>) -> Vec<(u64, Vec<f32>)> {
    a.extend(b);
    a
}

fn committed_final(text: &str) -> SttPartial {
    let mut partial = SttPartial::without_confidence("scripted", "scripted-1", text);
    partial.is_final = true;
    partial.committed = true;
    partial
}

fn uncommitted_partial(text: &str) -> SttPartial {
    SttPartial::without_confidence("scripted", "scripted-1", text)
}

fn scripted_cascade(
    stt: ScriptedStt,
    translator: ScriptedTranslator,
) -> (Cascade, ScriptedPlayout) {
    let playout = ScriptedPlayout::default();
    let clock = SharedClock::new(StepClock::new(240));
    let cascade = Cascade::new(
        stt.into(),
        translator.into(),
        ScriptedTts::default().into(),
        playout.clone(),
        clock,
    );
    (cascade, playout)
}

// ------------------------------------------------------------------ the gate ---

/// Test 1 (signature): the poisoned partial never reaches TTS. If this fails,
/// a user says half a sentence to an interviewer — the worst failure of the
/// phase.
#[tokio::test]
async fn poisoned_partials_never_reach_tts() {
    let stt = ScriptedStt::partial_then_final("半句话 POISON", "完整句子");
    let (mut cascade, playout) =
        scripted_cascade(stt, ScriptedTranslator::one_fragment("a complete sentence"));

    let script = SegmentScript {
        epoch: 7,
        frames: speech_frames(0, 400),
    };
    let outcomes = cascade.drive_user_track(&script).await.unwrap();

    let ledger = cascade.ledger();
    // Sanity: the scripted story played out (one segment, one translation).
    assert_eq!(outcomes.len(), 1);
    // The gate admitted the committed final and nothing else.
    assert_eq!(ledger.admitted, vec!["完整句子".to_string()]);
    assert_eq!(ledger.rejected_partials, vec!["半句话 POISON".to_string()]);
    // Witness: everything ever handed to TTS.
    assert_eq!(ledger.tts_inputs, vec!["a complete sentence".to_string()]);
    assert!(
        ledger
            .tts_inputs
            .iter()
            .all(|text| !text.contains("POISON")),
        "GOV-15 不变量违反：spoken English ⊆ committed finals —— 毒化 partial 泄漏到了 TTS：{:?}",
        ledger.tts_inputs
    );
    // TTS inputs are translations of admitted finals only (∅-checked above,
    // subset by construction: translated ⊆ admitted).
    assert!(ledger
        .translated
        .iter()
        .all(|fragment| ledger.admitted.contains(&fragment.text)));
    assert_eq!(playout.played().len(), 1);
}

/// Test 2: a vendor resend (partial after final) is dropped — no retroactive
/// translation, no second segment.
#[tokio::test]
async fn late_partials_are_dropped_without_retranslation() {
    let stt = ScriptedStt::new(vec![
        committed_final("完整句子"),
        uncommitted_partial("半句话 POISON"),
    ]);
    let (mut cascade, _playout) =
        scripted_cascade(stt, ScriptedTranslator::one_fragment("a complete sentence"));

    let script = SegmentScript {
        epoch: 1,
        frames: speech_frames(0, 300),
    };
    let outcomes = cascade.drive_user_track(&script).await.unwrap();

    assert_eq!(outcomes.len(), 1, "a resend must not open a second segment");
    assert_eq!(
        cascade.ledger().translated.len(),
        1,
        "no retroactive translation"
    );
    assert_eq!(
        cascade.ledger().rejected_partials,
        vec!["半句话 POISON".to_string()]
    );
}

/// Test 3: the vendor's eos (committed final) closes the segment — no silence
/// long enough to trigger the VAD path.
#[tokio::test]
async fn vendor_eos_closes_the_segment() {
    let stt = ScriptedStt::partial_then_final("半句话", "完整句子");
    let (mut cascade, _playout) =
        scripted_cascade(stt, ScriptedTranslator::one_fragment("a complete sentence"));

    let script = SegmentScript {
        epoch: 1,
        frames: concat(speech_frames(0, 400), silence_frames(400, 300)),
    };
    let outcomes = cascade.drive_user_track(&script).await.unwrap();

    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].segment.close_reason, Some(CloseReason::Eos));
    assert_eq!(outcomes[0].segment.zh_text, "完整句子");
}

/// Test 4: silence ≥ 700 ms closes the segment; a sub-300 ms transient (cough,
/// keyboard) never produces one.
#[tokio::test]
async fn silence_closes_and_transients_are_discarded() {
    // (a) real speech + long silence → close, attributed to the silence.
    let stt = ScriptedStt::partial_then_final("半句话", "完整句子");
    let (mut cascade, _playout) =
        scripted_cascade(stt, ScriptedTranslator::one_fragment("a complete sentence"));
    let script = SegmentScript {
        epoch: 1,
        frames: concat(speech_frames(0, 400), silence_frames(400, 900)),
    };
    let outcomes = cascade.drive_user_track(&script).await.unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].segment.close_reason, Some(CloseReason::Silence));

    // (b) a 200 ms burst of noise, nothing else → no segment at all.
    let stt = ScriptedStt::new(vec![]);
    let (mut cascade, _playout) =
        scripted_cascade(stt, ScriptedTranslator::one_fragment("a complete sentence"));
    let script = SegmentScript {
        epoch: 1,
        frames: concat(
            timeline(0, 20, |_| noise_frame(0.05)),
            silence_frames(200, 900),
        ),
    };
    let outcomes = cascade.drive_user_track(&script).await.unwrap();
    assert!(
        outcomes.is_empty(),
        "a 200 ms transient must not produce a segment: {outcomes:?}"
    );
    assert!(
        !cascade.ledger().discarded_segments.is_empty(),
        "the transient is recorded as discarded, not silently dropped"
    );
}

/// Test 5: a segment at the 15 s ceiling is force-closed (reason
/// `MaxDuration`) instead of growing without bound.
#[tokio::test]
async fn max_duration_force_closes() {
    assert_eq!(SegmentConfig::default().max_segment_ms, 15_000);
    let stt = ScriptedStt::partial_then_final("半句话", "完整句子");
    let (mut cascade, _playout) =
        scripted_cascade(stt, ScriptedTranslator::one_fragment("a complete sentence"));

    // 15.2 s of continuous speech: past the ceiling, no silence anywhere.
    let script = SegmentScript {
        epoch: 1,
        frames: speech_frames(0, 15_200),
    };
    let outcomes = cascade.drive_user_track(&script).await.unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0].segment.close_reason,
        Some(CloseReason::MaxDuration)
    );
}

// --------------------------------------------------------- interviewer line ---

/// Test 6: Deepgram's `SpeechStarted` opens the line's segment, the committed
/// final closes it — subtitles only. The type has no translator and no TTS, so
/// "never re-voiced" is a compile-time property, not a convention.
#[tokio::test]
async fn interviewer_line_is_subtitle_only() {
    let stt = EventScript::new(vec![
        SttEvent::SpeechStarted { at_ms: 0 },
        SttEvent::Partial(uncommitted_partial("How would you")),
        SttEvent::Partial(committed_final("How would you implement a rate limiter")),
    ]);
    let mut line = InterviewerCascade::from_source(stt);
    let outcome = line.drive_line(11).await.unwrap();

    assert_eq!(outcome.segments.len(), 1);
    assert_eq!(outcome.segments[0].close_reason, Some(CloseReason::Eos));
    assert_eq!(
        outcome.segments[0].zh_text,
        "How would you implement a rate limiter"
    );
    let last = outcome.subtitles.last().unwrap();
    assert!(last.is_final);
    assert_eq!(last.text, "How would you implement a rate limiter");
    // Structural: `InterviewerOutcome` carries no audio and the cascade owns
    // no TtsSink; there is no code path from this line into synthesis.
}

/// Test 6 (cont.): a `SpeechStarted` on the user line is a barge-in signal,
/// not a segment event — it must not open or close anything by itself.
#[tokio::test]
async fn speech_started_alone_does_not_close_user_segments() {
    let stt = EventScript::new(vec![
        SttEvent::SpeechStarted { at_ms: 0 },
        SttEvent::Partial(committed_final("完整句子")),
    ]);
    let playout = ScriptedPlayout::default();
    let clock = SharedClock::new(StepClock::new(240));
    let mut cascade = Cascade::from_source(
        stt,
        ScriptedTranslator::one_fragment("a complete sentence").into(),
        ScriptedTts::default().into(),
        playout,
        clock,
    );
    let script = SegmentScript {
        epoch: 1,
        frames: speech_frames(0, 300),
    };
    let outcomes = cascade.drive_user_track(&script).await.unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].segment.close_reason, Some(CloseReason::Eos));
}

// --------------------------------------------------------------- the marks ---

/// Test 8: every segment yields exactly one complete waterfall — all five
/// 02-01 boundaries, in order, inside the 2000 ms budget.
#[tokio::test]
async fn every_segment_records_a_complete_waterfall() {
    let stt = ScriptedStt::partial_then_final("半句话", "完整句子");
    let (mut cascade, _playout) =
        scripted_cascade(stt, ScriptedTranslator::one_fragment("a complete sentence"));

    let script = SegmentScript {
        epoch: 3,
        frames: speech_frames(0, 300),
    };
    let first = cascade.drive_user_track(&script).await.unwrap();
    let second = cascade.drive_user_track(&script).await.unwrap();
    assert_eq!(first.len() + second.len(), 2);

    let waterfalls = cascade.take_waterfalls();
    assert_eq!(waterfalls.len(), 2, "one waterfall per closed segment");
    assert_eq!(waterfalls[0].segment_id, 1);
    assert_eq!(waterfalls[1].segment_id, 2);
    assert!(waterfalls[0].cold, "the session's first segment is cold");
    assert!(!waterfalls[1].cold);
    for waterfall in &waterfalls {
        // Completeness is what `from_marks` enforces (missing/duplicate/
        // out-of-order all fail); the budget gate is the 02-01 contract.
        assert_eq!(waterfall.stage_ms.len(), Stage::ALL.len());
        assert_within_budget(waterfall).unwrap_or_else(|breach| panic!("{breach}"));
    }
}

// ------------------------------------------------------------------ fixtures ---

/// A scripted STT that replays raw [`SttEvent`]s — the `SpeechStarted` /
/// utterance-end shapes [`ScriptedStt`] cannot express.
struct EventScript {
    events: Vec<SttEvent>,
}

impl EventScript {
    fn new(events: Vec<SttEvent>) -> Self {
        Self { events }
    }
}

impl SttSource for EventScript {
    fn provider(&self) -> &'static str {
        "scripted"
    }

    fn model_version(&self) -> String {
        "scripted-1".to_string()
    }

    fn set_marks(&mut self, _marks: MarkHandle) {}

    fn start(&mut self, _epoch: u64) -> Result<SttStream, StageError> {
        let (upstream_tx, _upstream_rx) = tokio::sync::mpsc::channel(16);
        let (events_tx, events_rx) = tokio::sync::mpsc::channel(16);
        let events = self.events.clone();
        tokio::spawn(async move {
            for event in events {
                if events_tx.send(event).await.is_err() {
                    return;
                }
            }
        });
        Ok(SttStream::new("scripted", upstream_tx, events_rx))
    }
}
