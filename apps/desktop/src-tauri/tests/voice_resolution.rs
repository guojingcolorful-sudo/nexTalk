//! T4.3 预置音色回退与逐片段音色选择 — integration suite (02-04).
//!
//! The cascade asks a voice source once per fragment which voice the TTS
//! stage should speak with. This suite pins the four states a user can be in:
//!
//! | state | expected voice | why it matters |
//! |-------|----------------|----------------|
//! | no profile | preset | 未注册也能用：a first run must converse |
//! | corrupt profile | preset + readable warning | a broken file must never block the app |
//! | ready profile | clone | the product's promise |
//! | trained mid-session | clone from the *next* fragment | no restart (T4.3 Test 4) |
//!
//! The source is `voice_resolver(root)`, which reads the profile on every
//! call — the tests write the profile *after* the cascade was assembled to
//! prove nothing is cached at process start (Test 4).
//!
//! Zero network, zero keys: every stage is a scripted double (the cascade-level
//! equivalent of the vendor mocks in `tests/mock_vendors.rs`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nextalk_desktop_lib::enroll::voice_store::{
    voice_resolver, VoiceProfile, VoiceStore, DEFAULT_PRESET_VOICE, PROFILE_STATUS_READY,
};
use nextalk_desktop_lib::pipeline::cascade::{Cascade, PlayoutSink, SegmentScript, SharedClock};
use nextalk_desktop_lib::pipeline::stages::{
    AudioChunk, MarkHandle, ScriptedStt, ScriptedTranslator, ScriptedTts, SttPartial, VoiceRef,
};
use nextalk_desktop_lib::sim::source::TimeSource;

const FRAME_MS: u64 = 10;
const FRAME_SAMPLES: usize = 480; // 10 ms @ 48 kHz

// ------------------------------------------------------------------ doubles ---

/// Deterministic step clock (sleep is banned in tests).
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

/// Records what was played; the resolution suite never asserts marks.
#[derive(Clone, Default)]
struct ScriptedPlayout {
    played: Arc<Mutex<Vec<(u64, u64, usize)>>>,
}

impl ScriptedPlayout {
    fn played(&self) -> Vec<(u64, u64, usize)> {
        self.played.lock().expect("playout log").clone()
    }
}

impl PlayoutSink for ScriptedPlayout {
    fn set_marks(&mut self, _marks: MarkHandle) {}

    fn play(&mut self, epoch: u64, segment_id: u64, chunk: &AudioChunk) {
        self.played
            .lock()
            .expect("playout log")
            .push((epoch, segment_id, chunk.samples()));
    }
}

// ----------------------------------------------------------------- audio ---

fn sine_frame(amplitude: f32) -> Vec<f32> {
    (0..FRAME_SAMPLES)
        .map(|i| {
            let t = i as f32 / 48_000.0;
            (2.0 * std::f32::consts::PI * 440.0 * t).sin() * amplitude
        })
        .collect()
}

fn speech_frames(start_ms: u64, ms: u64) -> Vec<(u64, Vec<f32>)> {
    (0..ms / FRAME_MS)
        .map(|i| (start_ms + i * FRAME_MS, sine_frame(0.3)))
        .collect()
}

fn committed_final(text: &str) -> SttPartial {
    let mut partial = SttPartial::without_confidence("scripted", "scripted-1", text);
    partial.is_final = true;
    partial.committed = true;
    partial
}

// ----------------------------------------------------------------- fixtures ---

fn temp_root(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("nextalk-02-04-voice-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    root
}

fn ready_profile(root: &Path, speaker_id: &str) -> VoiceProfile {
    VoiceProfile {
        speaker_id: speaker_id.to_string(),
        resource_id: "seed-icl-2.0".to_string(),
        created_at: "2026-10-05T00:00:00Z".to_string(),
        sample_path: root.join("enroll").join("take-1.wav").display().to_string(),
        duration_s: 90.0,
        status: PROFILE_STATUS_READY.to_string(),
        previous: Vec::new(),
    }
}

/// A cascade over scripted stages whose voice source is the real store root.
fn voice_cascade(
    root: &Path,
    stt: ScriptedStt,
    translator: ScriptedTranslator,
) -> (Cascade, ScriptedPlayout, ScriptedTts) {
    let tts = ScriptedTts::default();
    let playout = ScriptedPlayout::default();
    let clock = SharedClock::new(StepClock::new(240));
    let cascade = Cascade::new(
        stt.into(),
        translator.into(),
        tts.clone().into(),
        playout.clone(),
        clock,
    )
    .with_voice_source(voice_resolver(root));
    (cascade, playout, tts)
}

fn one_fragment_story() -> (ScriptedStt, ScriptedTranslator) {
    (
        ScriptedStt::partial_then_final("半句话", "完整句子"),
        ScriptedTranslator::one_fragment("a complete sentence"),
    )
}

// ------------------------------------------------------------------ tests ---

/// Test 1: no profile at all — the preset voice speaks and the fragment runs
/// end to end (the cascade-level half of ROADMAP criterion 3).
#[tokio::test]
async fn resolve_voice_without_a_profile_returns_the_preset_and_the_fragment_still_runs() {
    let root = temp_root("no-profile");
    let (stt, translator) = one_fragment_story();
    let (mut cascade, playout, tts) = voice_cascade(&root, stt, translator);

    let outcomes = cascade
        .drive_user_track(&SegmentScript {
            epoch: 1,
            frames: speech_frames(0, 400),
        })
        .await
        .expect("an unregistered user's fragment must complete");

    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].translated, "STT → translate succeeded");
    assert_eq!(outcomes[0].tts_calls, 1);
    assert_eq!(playout.played().len(), 1, "the audio reached the playout");

    let calls = tts.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].0, "a complete sentence",
        "the translation was spoken"
    );
    assert_eq!(
        calls[0].1,
        VoiceRef::Preset(DEFAULT_PRESET_VOICE.to_string()),
        "no profile means the preset voice, silently"
    );
}

/// Test 2: a corrupt profile is a fallback, never a blocker.
#[tokio::test]
async fn resolve_voice_with_a_corrupt_profile_falls_back_without_blocking() {
    let root = temp_root("corrupt-profile");
    std::fs::create_dir_all(root.join("voice")).expect("voice dir");
    std::fs::write(root.join("voice").join("profile.json"), "{ not json").expect("corrupt profile");

    let (stt, translator) = one_fragment_story();
    let (mut cascade, _playout, tts) = voice_cascade(&root, stt, translator);
    let outcomes = cascade
        .drive_user_track(&SegmentScript {
            epoch: 1,
            frames: speech_frames(0, 400),
        })
        .await
        .expect("a corrupt profile must not stop the conversation");

    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        tts.calls()[0].1,
        VoiceRef::Preset(DEFAULT_PRESET_VOICE.to_string())
    );
}

/// Test 3: a ready profile is the clone.
#[tokio::test]
async fn resolve_voice_with_a_ready_profile_uses_the_clone() {
    let root = temp_root("ready-profile");
    let store = VoiceStore::new(&root);
    store
        .save(&ready_profile(&root, "S_ready"))
        .expect("save profile");

    let (stt, translator) = one_fragment_story();
    let (mut cascade, _playout, tts) = voice_cascade(&root, stt, translator);
    cascade
        .drive_user_track(&SegmentScript {
            epoch: 1,
            frames: speech_frames(0, 400),
        })
        .await
        .expect("a registered user's fragment completes");

    assert_eq!(
        tts.calls()[0].1,
        VoiceRef::Clone(nextalk_desktop_lib::pipeline::stages::SpeakerId::new(
            "S_ready"
        ))
    );
}

/// Test 4: training that finishes mid-session applies from the *next*
/// fragment — nothing is cached at assembly.
#[tokio::test]
async fn resolve_voice_is_read_per_fragment_so_training_applies_mid_session() {
    let root = temp_root("mid-session");
    let (stt, translator) = one_fragment_story();
    let (mut cascade, _playout, tts) = voice_cascade(&root, stt, translator);

    cascade
        .drive_user_track(&SegmentScript {
            epoch: 1,
            frames: speech_frames(0, 400),
        })
        .await
        .expect("first fragment (unregistered)");
    assert_eq!(
        tts.calls()[0].1,
        VoiceRef::Preset(DEFAULT_PRESET_VOICE.to_string()),
        "the first fragment ran before any training"
    );

    // Training completes while the session is live.
    VoiceStore::new(&root)
        .save(&ready_profile(&root, "S_fresh"))
        .expect("training lands the profile");

    cascade
        .drive_user_track(&SegmentScript {
            epoch: 2,
            frames: speech_frames(700, 400),
        })
        .await
        .expect("second fragment (now registered)");

    let calls = tts.calls();
    assert_eq!(calls.len(), 2, "both fragments spoke");
    assert_eq!(
        calls[1].1,
        VoiceRef::Clone(nextalk_desktop_lib::pipeline::stages::SpeakerId::new(
            "S_fresh"
        )),
        "the next fragment must pick up the new voice without a restart"
    );
}

/// Test 5: deleting the profile takes the clone away on the next fragment.
#[tokio::test]
async fn resolve_voice_never_returns_a_deleted_clone() {
    let root = temp_root("delete-then-resolve");
    let store = VoiceStore::new(&root);
    store
        .save(&ready_profile(&root, "S_gone"))
        .expect("save profile");

    let (stt, translator) = one_fragment_story();
    let (mut cascade, _playout, tts) = voice_cascade(&root, stt, translator);

    cascade
        .drive_user_track(&SegmentScript {
            epoch: 1,
            frames: speech_frames(0, 400),
        })
        .await
        .expect("first fragment uses the clone");
    assert!(matches!(tts.calls()[0].1, VoiceRef::Clone(_)));

    store.delete().expect("the user deletes their voice");

    cascade
        .drive_user_track(&SegmentScript {
            epoch: 2,
            frames: speech_frames(700, 400),
        })
        .await
        .expect("the conversation continues on the preset");
    assert_eq!(
        tts.calls()[1].1,
        VoiceRef::Preset(DEFAULT_PRESET_VOICE.to_string())
    );
}

/// Test 6: an unregistered user completes a whole conversation — both
/// fragments of the story close, both translate and speak, and every
/// synthesis uses the preset voice (mock stages).
///
/// The double replays its script on every `start`, so one drive carries the
/// whole two-fragment conversation; nothing is cached from the previous run.
#[tokio::test]
async fn resolve_voice_runs_a_whole_unregistered_conversation_on_mock_stages() {
    let root = temp_root("full-conversation");
    let stt = ScriptedStt::new(vec![
        committed_final("第一句话"),
        committed_final("第二句话"),
    ]);
    let (mut cascade, playout, tts) = voice_cascade(
        &root,
        stt,
        ScriptedTranslator::one_fragment("a complete sentence"),
    );

    let outcomes = cascade
        .drive_user_track(&SegmentScript {
            epoch: 1,
            frames: speech_frames(0, 400),
        })
        .await
        .expect("an unregistered conversation must run end to end");

    assert_eq!(outcomes.len(), 2, "both exchanges closed");
    assert!(
        outcomes.iter().all(|outcome| outcome.translated),
        "both exchanges were translated: {outcomes:?}"
    );
    assert_eq!(playout.played().len(), 2, "both answers were spoken");
    let calls = tts.calls();
    assert_eq!(calls.len(), 2);
    assert!(
        calls
            .iter()
            .all(|(_, voice)| *voice == VoiceRef::Preset(DEFAULT_PRESET_VOICE.to_string())),
        "an unregistered user hears the preset voice, not a failure: {calls:?}"
    );
}
