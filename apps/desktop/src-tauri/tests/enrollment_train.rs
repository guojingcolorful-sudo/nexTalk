//! T4.2 音色训练（voice_clone）与本地音色档案 — integration suite (02-04).
//!
//! The training call is the one place in the enrollment flow that leaves the
//! machine: the captured WAV goes to 火山's `voice_clone` endpoint and comes
//! back as a speaker id the TTS stage can synthesise with. Everything else —
//! the 10 MB pre-check, the error classification, the profile on disk, the
//! preset fallback — is local.
//!
//! The protocol here is the field-for-field shape verified by
//! `tools/vendor-experiments/volc-voice-clone.mjs` (and the training fallback
//! in `cross-lingual-clone-probe.mjs`): POST
//! `https://openspeech.bytedance.com/api/v3/tts/voice_clone` with `X-Api-Key`
//! and a *fresh* `X-Api-Request-Id` (UUID) per request, body
//! `{speaker_id, audio: {data (base64), format: "wav"}, text, language: 0}`.
//!
//! Every test runs against an axum mock bound to `127.0.0.1:0`. No test calls
//! [`CloneTrainer::from_lookup`], so nothing here can reach the real vendor —
//! the live training run happens once by hand with the keys in
//! `tools/vendor-experiments/.env`.
//!
//! Privacy assertions (T-02-16/T-02-17) ride along: the profile never contains
//! a credential, error strings carry host + path but never the key, and the
//! profile file is owner-only.

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::{json, Value};
use tokio::net::TcpListener;

use nextalk_desktop_lib::enroll::register::{
    train_voice_clone, CloneTrainError, CloneTrainer, MAX_UPLOAD_BYTES, VOICE_CLONE_PATH,
    VOICE_CLONE_URL,
};
use nextalk_desktop_lib::enroll::voice_store::{
    VoiceProfile, VoiceStore, CLONE_RESOURCE_ID, DEFAULT_PRESET_VOICE, PROFILE_STATUS_READY,
};
use nextalk_desktop_lib::pipeline::stages::{SpeakerId, VoiceRef};

/// The path the mock serves. Kept as a literal so the mock and the client
/// constant are asserted against each other rather than sharing one value.
const MOCK_PATH: &str = "/api/v3/tts/voice_clone";

const API_KEY: &str = "clone-key-0001";

// ---------------------------------------------------------------------------
// mock vendor (the mock_vendors.rs harness pattern, reduced to one endpoint)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Recorded {
    method: String,
    path: String,
    api_key: Option<String>,
    request_id: Option<String>,
    content_type: Option<String>,
    body: Value,
}

#[derive(Clone)]
struct ScriptedResponse {
    status: u16,
    body: Value,
}

#[derive(Clone)]
struct MockState {
    requests: Arc<Mutex<Vec<Recorded>>>,
    response: Arc<Mutex<ScriptedResponse>>,
}

impl MockState {
    fn new(status: u16, body: Value) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            response: Arc::new(Mutex::new(ScriptedResponse { status, body })),
        }
    }

    fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().expect("mock requests").clone()
    }

    fn request_count(&self) -> usize {
        self.requests.lock().expect("mock requests").len()
    }
}

struct Server {
    addr: SocketAddr,
    state: MockState,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl Server {
    fn url(&self) -> String {
        format!("http://{}{MOCK_PATH}", self.addr)
    }

    fn requests(&self) -> Vec<Recorded> {
        self.state.requests()
    }

    fn request_count(&self) -> usize {
        self.state.request_count()
    }
}

/// One scripted endpoint: records every request, always answers with the
/// currently scripted status + JSON body.
async fn spawn_mock(state: MockState) -> Server {
    let router = Router::new()
        .route(MOCK_PATH, any(capture))
        .with_state(state.clone());
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("mock binds an ephemeral loopback port");
    let addr = listener.local_addr().expect("mock has an address");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    Server {
        addr,
        state,
        handle,
    }
}

async fn capture(
    State(state): State<MockState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };
    let recorded = Recorded {
        method: method.to_string(),
        path: uri.path().to_string(),
        api_key: header("x-api-key"),
        request_id: header("x-api-request-id"),
        content_type: header("content-type"),
        body: serde_json::from_slice(&body).unwrap_or(Value::Null),
    };
    state.requests.lock().expect("mock requests").push(recorded);

    let scripted = state.response.lock().expect("mock response").clone();
    let status = StatusCode::from_u16(scripted.status).expect("scripted status");
    (status, Json(scripted.body)).into_response()
}

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

fn temp_root(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("nextalk-02-04-train-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    root
}

/// A real 16 kHz mono PCM16 WAV — what the capture step writes.
fn write_wav(dir: &Path, name: &str, seconds: f32) -> PathBuf {
    std::fs::create_dir_all(dir).expect("wav dir");
    let path = dir.join(format!("{name}.wav"));
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).expect("wav writer");
    let len = (seconds * 16_000.0) as usize;
    for n in 0..len {
        let t = n as f32 / 16_000.0;
        let sample =
            ((2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.3 * i16::MAX as f32) as i16;
        writer.write_sample(sample).expect("write sample");
    }
    writer.finalize().expect("finalize");
    path
}

/// A sample where the capture step puts them: `<root>/enroll/<name>.wav`.
fn write_sample(root: &Path, name: &str, seconds: f32) -> PathBuf {
    write_wav(&root.join("enroll"), name, seconds)
}

fn trainer(mock: &Server) -> CloneTrainer {
    CloneTrainer::new(mock.url(), Some(API_KEY.to_string()))
}

// ---------------------------------------------------------------------------
// Test 1 — request construction, field-for-field with the experiment script
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_training_request_matches_the_experiment_script_field_for_field() {
    let root = temp_root("request-shape");
    let mock = spawn_mock(MockState::new(200, json!({ "code": 0 }))).await;
    let sample = write_sample(&root, "take-1", 1.5);
    let store = VoiceStore::new(&root);
    let transcript = "大家好，我是做后端开发的，平时主要负责服务的性能优化和稳定性建设。";

    let profile = train_voice_clone(&trainer(&mock), &store, &sample, transcript, "S_02-04-test")
        .await
        .expect("the mock accepted the training");
    assert_eq!(profile.speaker_id, "S_02-04-test");

    assert_eq!(
        mock.request_count(),
        1,
        "exactly one request per training call"
    );
    let request = &mock.requests()[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, VOICE_CLONE_PATH);
    assert_eq!(request.api_key.as_deref(), Some(API_KEY));
    assert_eq!(request.content_type.as_deref(), Some("application/json"));
    let request_id = request.request_id.clone().expect("X-Api-Request-Id");
    uuid::Uuid::parse_str(&request_id)
        .expect("the request id is a UUID — the mjs uses crypto.randomUUID()");

    let body = &request.body;
    assert_eq!(body["speaker_id"], json!("S_02-04-test"));
    assert_eq!(body["language"], json!(0));
    assert_eq!(body["text"], json!(transcript));
    assert_eq!(body["audio"]["format"], json!("wav"));

    // The uploaded audio is the sample file itself, base64-encoded.
    let decoded = STANDARD
        .decode(body["audio"]["data"].as_str().expect("audio.data"))
        .expect("audio.data is base64");
    assert_eq!(
        decoded,
        std::fs::read(&sample).expect("sample bytes"),
        "the WAV bytes travel unmodified"
    );

    // Exactly the fields the verified script sends — nothing extra, nothing missing.
    let fields: BTreeSet<&str> = body
        .as_object()
        .expect("body object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        fields,
        BTreeSet::from(["speaker_id", "audio", "text", "language"])
    );
    let audio_fields: Vec<&str> = body["audio"]
        .as_object()
        .expect("audio object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(audio_fields, vec!["data", "format"]);

    // A second request draws a fresh id (每请求一个新 UUID).
    train_voice_clone(
        &trainer(&mock),
        &store,
        &sample,
        transcript,
        "S_02-04-test-2",
    )
    .await
    .expect("retraining succeeds");
    let ids: Vec<String> = mock
        .requests()
        .iter()
        .map(|request| request.request_id.clone().expect("request id"))
        .collect();
    assert_ne!(ids[0], ids[1], "reusing a request id is a protocol error");
}

// ---------------------------------------------------------------------------
// Test 2 — the 10 MB cap fires before anything is uploaded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_sample_over_ten_megabytes_is_refused_before_any_request() {
    let root = temp_root("too-large");
    let mock = spawn_mock(MockState::new(200, json!({ "code": 0 }))).await;
    let dir = root.join("enroll");
    std::fs::create_dir_all(&dir).expect("enroll dir");
    let big = dir.join("big.wav");
    std::fs::write(&big, vec![0u8; MAX_UPLOAD_BYTES + 1]).expect("big file");
    let store = VoiceStore::new(&root);

    let error = train_voice_clone(&trainer(&mock), &store, &big, "文本", "S_big")
        .await
        .expect_err("11 MB is over the vendor's 10 MB limit");
    match error {
        CloneTrainError::AudioTooLarge { bytes, max_bytes } => {
            assert_eq!(bytes, MAX_UPLOAD_BYTES + 1);
            assert_eq!(max_bytes, MAX_UPLOAD_BYTES);
        }
        other => panic!("expected AudioTooLarge, got {other:?}"),
    }
    assert_eq!(
        mock.request_count(),
        0,
        "the refusal must not waste a training quota"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — only samples from the enroll directory are uploaded
// ---------------------------------------------------------------------------

#[tokio::test]
async fn only_samples_inside_the_enroll_directory_are_uploaded() {
    let root = temp_root("containment");
    let outside = temp_root("containment-outside");
    let mock = spawn_mock(MockState::new(200, json!({ "code": 0 }))).await;
    let store = VoiceStore::new(&root);

    // A file that exists but is not an enrollment take (T-02-16: the upload
    // path is exactly one directory).
    let foreign = write_wav(&outside, "somewhere-else", 1.0);
    let error = train_voice_clone(&trainer(&mock), &store, &foreign, "文本", "S_foreign")
        .await
        .expect_err("a path outside enroll/ is refused");
    assert!(
        matches!(error, CloneTrainError::SampleRejected { .. }),
        "{error:?}"
    );
    assert!(error.message().contains("重新录制"), "{}", error.message());

    // A missing file is its own error, also locally.
    let missing = root.join("enroll").join("never-recorded.wav");
    let error = train_voice_clone(&trainer(&mock), &store, &missing, "文本", "S_missing")
        .await
        .expect_err("nothing to upload");
    assert!(
        matches!(error, CloneTrainError::SampleMissing { .. }),
        "{error:?}"
    );
    assert!(
        error.message().contains("找不到录音文件"),
        "{}",
        error.message()
    );

    assert_eq!(mock.request_count(), 0, "neither case reaches the wire");
}

// ---------------------------------------------------------------------------
// Test 4 — the WER gate (45001109) has its own error and its own copy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_wer_gate_is_its_own_error_with_its_own_copy() {
    // The gate often arrives on the success code path — a 200 with a nonzero
    // vendor code — and must be classified by the code, not the status.
    let root = temp_root("wer-gate");
    let mock = spawn_mock(MockState::new(
        200,
        json!({ "code": 45001109, "message": "the text does not match the audio" }),
    ))
    .await;
    let sample = write_sample(&root, "take-1", 1.0);
    let store = VoiceStore::new(&root);

    let error = train_voice_clone(&trainer(&mock), &store, &sample, "错误文本", "S_wer")
        .await
        .expect_err("the transcript does not match the recording");
    match &error {
        CloneTrainError::TranscriptMismatch { code } => assert_eq!(*code, 45001109),
        other => panic!("expected TranscriptMismatch, got {other:?}"),
    }
    assert_eq!(error.code(), "transcript_mismatch");
    let message = error.message();
    assert!(message.contains("录音与文本不匹配"), "{message}");
    assert!(message.contains("请重录"), "{message}");
    assert_ne!(
        message,
        CloneTrainError::Retryable {
            status: Some(429),
            detail: String::new(),
        }
        .message(),
        "the WER gate is not a transient blip"
    );
    assert_eq!(mock.request_count(), 1, "no automatic retry");
}

// ---------------------------------------------------------------------------
// Test 5 — retryable vs credential failures
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transient_and_credential_failures_are_classified_apart() {
    let cases: [(u16, &str, &str); 4] = [
        (429, "retryable", "too many requests"),
        (503, "retryable", "service unavailable"),
        (401, "credentials", "invalid token"),
        (403, "credentials", "forbidden"),
    ];

    for (status, expected_code, detail) in cases {
        let root = temp_root(&format!("classify-{status}"));
        let mock = spawn_mock(MockState::new(status, json!({ "message": detail }))).await;
        let sample = write_sample(&root, "take-1", 1.0);
        let store = VoiceStore::new(&root);

        let error = train_voice_clone(&trainer(&mock), &store, &sample, "文本", "S_x")
            .await
            .expect_err("the mock refused");
        assert_eq!(error.code(), expected_code, "HTTP {status}: {error:?}");
        match (&error, status) {
            (
                CloneTrainError::Retryable {
                    status: Some(seen), ..
                },
                429 | 503,
            ) => {
                assert_eq!(*seen, status)
            }
            (CloneTrainError::Credentials { status: seen }, 401 | 403) => {
                assert_eq!(*seen, status)
            }
            (other, _) => panic!("HTTP {status} classified as {other:?}"),
        }
        assert_eq!(
            mock.request_count(),
            1,
            "a single call must not auto-retry — retry policy belongs to the caller"
        );
        // T-02-17: no error string ever carries a credential.
        assert!(
            !format!("{error:?}").contains(API_KEY) && !error.message().contains(API_KEY),
            "the key leaked into {error:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 6 — every other vendor rejection is terminal and keeps its code
// ---------------------------------------------------------------------------

#[tokio::test]
async fn other_vendor_rejections_keep_their_original_code() {
    // Both shapes exist in the wild: an HTTP 4xx wrapping a vendor code, and a
    // 200 carrying a nonzero code.
    let cases: [(u16, u64); 2] = [(400, 12345678), (200, 40000001)];

    for (status, code) in cases {
        let root = temp_root(&format!("terminal-{code}"));
        let mock = spawn_mock(MockState::new(
            status,
            json!({ "code": code, "message": "rejected" }),
        ))
        .await;
        let sample = write_sample(&root, "take-1", 1.0);
        let store = VoiceStore::new(&root);

        let error = train_voice_clone(&trainer(&mock), &store, &sample, "文本", "S_x")
            .await
            .expect_err("the mock refused");
        match &error {
            CloneTrainError::Terminal {
                status: seen,
                code: seen_code,
                ..
            } => {
                assert_eq!(*seen, Some(status));
                assert_eq!(*seen_code, Some(code));
            }
            other => panic!("expected Terminal, got {other:?}"),
        }
        assert!(
            error.message().contains(&code.to_string()),
            "the original code stays visible: {}",
            error.message()
        );
        assert!(
            !error.message().contains("http"),
            "no scheme or full URL in copy: {}",
            error.message()
        );
    }
}

// ---------------------------------------------------------------------------
// Test 7 — a clean profile lands on disk (no credentials inside)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn success_writes_a_clean_profile_without_credentials() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_root("profile");
    let mock = spawn_mock(MockState::new(200, json!({ "code": 0 }))).await;
    let sample = write_sample(&root, "take-1", 1.5);
    let store = VoiceStore::new(&root);

    let profile = train_voice_clone(&trainer(&mock), &store, &sample, "文本", "S_profile")
        .await
        .expect("training succeeds");
    assert_eq!(profile.resource_id, CLONE_RESOURCE_ID);
    assert_eq!(profile.status, PROFILE_STATUS_READY);
    assert!(profile.previous.is_empty(), "the first take has no anchor");

    let path = store.profile_path();
    assert_eq!(path, root.join("voice").join("profile.json"));
    let saved = std::fs::read_to_string(&path).expect("profile.json exists");
    for needle in [API_KEY, "X-Api-Key", "api_key", "access_token"] {
        assert!(!saved.contains(needle), "{needle} leaked into the profile");
    }

    let raw: Value = serde_json::from_str(&saved).expect("valid JSON");
    assert_eq!(raw["speakerId"], json!("S_profile"));
    assert_eq!(raw["resourceId"], json!(CLONE_RESOURCE_ID));
    assert_eq!(raw["status"], json!("ready"));
    assert_eq!(raw["samplePath"], json!(sample.display().to_string()));
    assert_eq!(raw["previous"], json!([]));
    let created = raw["createdAt"].as_str().expect("createdAt");
    assert_eq!(created.len(), 20, "RFC3339 UTC: {created}");
    assert!(created.ends_with('Z'), "{created}");
    assert!(
        (raw["durationS"].as_f64().expect("durationS") - 1.5).abs() < 0.01,
        "{}",
        raw["durationS"]
    );

    let mode = std::fs::metadata(&path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "the voice profile is private data");

    // The same document round-trips through the store.
    assert_eq!(store.load().expect("load"), Some(profile));
}

// ---------------------------------------------------------------------------
// Test 8 — retraining replaces the profile and keeps the rollback anchor
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retraining_replaces_the_profile_and_keeps_the_rollback_anchor() {
    let root = temp_root("retrain");
    let mock = spawn_mock(MockState::new(200, json!({ "code": 0 }))).await;
    let store = VoiceStore::new(&root);
    let first = write_sample(&root, "take-1", 1.5);
    let second = write_sample(&root, "take-2", 2.0);

    train_voice_clone(&trainer(&mock), &store, &first, "第一版", "S_first")
        .await
        .expect("first training");
    let profile = train_voice_clone(&trainer(&mock), &store, &second, "第二版", "S_second")
        .await
        .expect("retraining");

    assert_eq!(profile.speaker_id, "S_second");
    assert_eq!(profile.sample_path, second.display().to_string());
    assert_eq!(profile.previous.len(), 1);
    assert_eq!(profile.previous[0].speaker_id, "S_first");
    assert_eq!(profile.previous[0].sample_path, first.display().to_string());
    assert_eq!(profile.previous[0].status, PROFILE_STATUS_READY);

    // A third take pushes the anchor further: newest-first, so `previous[0]`
    // is always the voice a failed retrain rolls back to.
    let third = write_sample(&root, "take-3", 1.0);
    let profile = train_voice_clone(&trainer(&mock), &store, &third, "第三版", "S_third")
        .await
        .expect("third training");
    assert_eq!(profile.previous.len(), 2);
    assert_eq!(profile.previous[0].speaker_id, "S_second");
    assert_eq!(profile.previous[1].speaker_id, "S_first");
    assert_eq!(profile.speaker_id, "S_third");
}

// ---------------------------------------------------------------------------
// Test 9 — deletion removes the record and the samples
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleting_the_profile_removes_the_record_and_the_samples() {
    let root = temp_root("delete");
    let mock = spawn_mock(MockState::new(200, json!({ "code": 0 }))).await;
    let store = VoiceStore::new(&root);
    let first = write_sample(&root, "take-1", 1.5);
    let second = write_sample(&root, "take-2", 2.0);
    train_voice_clone(&trainer(&mock), &store, &first, "第一版", "S_first")
        .await
        .expect("first training");
    train_voice_clone(&trainer(&mock), &store, &second, "第二版", "S_second")
        .await
        .expect("retraining");

    let outcome = store.delete().expect("delete succeeds");
    assert!(outcome.profile_removed);
    assert!(!store.profile_path().exists(), "profile.json is gone");
    assert_eq!(outcome.samples.len(), 2, "both takes are erased");
    for path in &outcome.samples {
        assert!(!path.exists(), "{} still exists", path.display());
    }
    assert!(!first.exists() && !second.exists());
    assert_eq!(store.load().expect("load after delete"), None);

    // Idempotent: a second delete is a clean no-op.
    let again = store.delete().expect("second delete");
    assert!(!again.profile_removed);
    assert!(again.samples.is_empty());
}

#[tokio::test]
async fn a_corrupt_profile_still_deletes_cleanly() {
    let root = temp_root("delete-corrupt");
    let store = VoiceStore::new(&root);
    std::fs::create_dir_all(root.join("voice")).expect("voice dir");
    std::fs::write(store.profile_path(), "{ this is not json").expect("corrupt profile");
    let orphan = write_sample(&root, "take-orphan", 1.0);

    let outcome = store
        .delete()
        .expect("deletion must not depend on a readable profile");
    assert!(outcome.profile_removed, "the corrupt record is removed");
    assert!(!store.profile_path().exists());
    assert_eq!(outcome.samples, vec![orphan.clone()]);
    assert!(!orphan.exists(), "orphaned samples do not survive a wipe");
}

// ---------------------------------------------------------------------------
// Test 10 — a dead endpoint fails as transport, with no URL in the copy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_closed_endpoint_fails_without_a_url_in_the_message() {
    let root = temp_root("closed-port");
    let store = VoiceStore::new(&root);
    let sample = write_sample(&root, "take-1", 1.0);
    let trainer = CloneTrainer::new(
        "http://127.0.0.1:1/api/v3/tts/voice_clone",
        Some(API_KEY.to_string()),
    )
    .with_timeout(Duration::from_secs(5));

    let error = train_voice_clone(&trainer, &store, &sample, "文本", "S_dead")
        .await
        .expect_err("nothing listens on port 1");
    match &error {
        CloneTrainError::Transport { endpoint, .. } => {
            assert_eq!(endpoint, "127.0.0.1:1/api/v3/tts/voice_clone")
        }
        other => panic!("expected Transport, got {other:?}"),
    }
    assert!(
        !error.message().contains("http"),
        "scheme or full URL leaked: {}",
        error.message()
    );
}

#[test]
fn the_default_endpoint_is_the_verified_vendor_path() {
    assert_eq!(
        VOICE_CLONE_URL,
        "https://openspeech.bytedance.com/api/v3/tts/voice_clone"
    );
    assert_eq!(VOICE_CLONE_PATH, "/api/v3/tts/voice_clone");
}

// ---------------------------------------------------------------------------
// Test 11 — credentials are named as missing before any request
// ---------------------------------------------------------------------------

#[tokio::test]
async fn missing_credentials_are_named_before_any_request() {
    let root = temp_root("no-key");
    let mock = spawn_mock(MockState::new(200, json!({ "code": 0 }))).await;
    let store = VoiceStore::new(&root);
    let sample = write_sample(&root, "take-1", 1.0);
    let trainer = CloneTrainer::new(mock.url(), None);

    let error = train_voice_clone(&trainer, &store, &sample, "文本", "S_nokey")
        .await
        .expect_err("no key, no training");
    assert!(
        matches!(error, CloneTrainError::MissingCredentials),
        "{error:?}"
    );
    assert_eq!(error.code(), "missing_credentials");
    assert!(error.message().contains("凭据"), "{}", error.message());
    assert!(
        error.message().contains("VOLC_CLONE_ACCESS_TOKEN"),
        "the copy names the variable to set: {}",
        error.message()
    );
    assert_eq!(mock.request_count(), 0);
}

// ---------------------------------------------------------------------------
// Test 12 — resolution: clone when ready, preset otherwise, never a panic
// ---------------------------------------------------------------------------

#[test]
fn resolution_falls_back_to_the_preset_voice_when_there_is_no_usable_profile() {
    let root = temp_root("resolve");
    let store = VoiceStore::new(&root);

    // First run: preset, and that is not a warning.
    let resolution = store.resolve(DEFAULT_PRESET_VOICE);
    assert_eq!(
        resolution.voice,
        VoiceRef::Preset(DEFAULT_PRESET_VOICE.to_string())
    );
    assert!(resolution.warning.is_none(), "first run is normal");

    // A ready profile wins.
    let profile = VoiceProfile {
        speaker_id: "S_ready".to_string(),
        resource_id: CLONE_RESOURCE_ID.to_string(),
        created_at: "2026-10-05T00:00:00Z".to_string(),
        sample_path: root.join("enroll").join("take-1.wav").display().to_string(),
        duration_s: 90.0,
        status: PROFILE_STATUS_READY.to_string(),
        previous: Vec::new(),
    };
    store.save(&profile).expect("save profile");
    let resolution = store.resolve(DEFAULT_PRESET_VOICE);
    assert_eq!(resolution.voice, VoiceRef::Clone(SpeakerId::new("S_ready")));
    assert!(resolution.warning.is_none());

    // A corrupt profile falls back with a readable Chinese warning — the app
    // must open with the preset voice instead of refusing to start.
    std::fs::write(store.profile_path(), "{ not json").expect("corrupt profile");
    let resolution = store.resolve(DEFAULT_PRESET_VOICE);
    assert_eq!(
        resolution.voice,
        VoiceRef::Preset(DEFAULT_PRESET_VOICE.to_string())
    );
    let warning = resolution.warning.expect("a corrupt profile is reported");
    assert!(warning.contains("预置"), "{warning}");

    // The preset itself is overridable (VOLC_TTS_VOICE is read by the caller).
    let resolution = store.resolve("my_custom_voice");
    assert_eq!(
        resolution.voice,
        VoiceRef::Preset("my_custom_voice".to_string())
    );
}
