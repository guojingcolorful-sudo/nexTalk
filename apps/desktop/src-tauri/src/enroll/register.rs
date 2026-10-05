//! voice_clone training: the one outbound call of the enrollment flow (02-04 T4.2).
//!
//! The protocol is the field-for-field shape verified by
//! `tools/vendor-experiments/volc-voice-clone.mjs` (and the training fallback
//! in `cross-lingual-clone-probe.mjs`):
//!
//! ```text
//! POST https://openspeech.bytedance.com/api/v3/tts/voice_clone
//! X-Api-Key: <VOLC_CLONE_ACCESS_TOKEN, else VOLC_TTS_ACCESS_TOKEN>
//! X-Api-Request-Id: <fresh UUID v4 per request>
//! {"speaker_id": "S_…", "audio": {"data": "<base64 wav>", "format": "wav"},
//!  "text": "<transcript>", "language": 0}
//! ```
//!
//! Privacy and safety rules that hold here (T-02-16/T-02-17):
//!
//! - the sample is read from `<app data>/enroll/` only — the upload path is
//!   exactly one directory, and anything else is refused *locally*;
//! - the 10 MB vendor cap is enforced before the request so a doomed upload
//!   never burns a training quota;
//! - credentials come from the environment and appear in no error string;
//!   errors carry the endpoint as host + path (never a scheme or a full URL).
//!
//! Failure classification follows the shared HTTP classifier
//! ([`classify_http_status`]) with one vendor-specific carve-out: code
//! `45001109` is the WER gate ("text and audio do not match") and gets its own
//! variant so the UI can say 请重录 instead of 请稍后重试. A single call never
//! auto-retries; retry policy belongs to the caller.

use std::fmt;
use std::path::Path;
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};

use crate::pipeline::stages::{classify_http_status, RetryClass};

use super::capture::{enrollment_dir, MAX_SAMPLE_BYTES};
use super::voice_store::{
    rfc3339_utc, VoiceProfile, VoiceStore, VoiceStoreError, CLONE_RESOURCE_ID, PROFILE_STATUS_READY,
};

/// The verified training endpoint (see the module docs).
pub const VOICE_CLONE_URL: &str = "https://openspeech.bytedance.com/api/v3/tts/voice_clone";
/// The endpoint's path on its own — what the mock serves and what errors name.
pub const VOICE_CLONE_PATH: &str = "/api/v3/tts/voice_clone";
/// Preferred credential variable (newer, narrower scope).
pub const CLONE_ACCESS_TOKEN_VAR: &str = "VOLC_CLONE_ACCESS_TOKEN";
/// Fallback credential variable (the one the TTS stream already reads).
pub const FALLBACK_ACCESS_TOKEN_VAR: &str = "VOLC_TTS_ACCESS_TOKEN";
/// Optional pre-chosen speaker id (mirrors the probe: the id is caller-supplied).
pub const NEW_SPEAKER_ID_VAR: &str = "VOLC_CLONE_NEW_SPEAKER_ID";
/// voice_clone takes the take as WAV.
pub const CLONE_AUDIO_FORMAT: &str = "wav";
/// The vendor's WER gate: transcript and audio do not match.
pub const WER_GATE_CODE: u64 = 45_001_109;
/// The vendor's hard upload limit; same number the capture guard uses.
pub const MAX_UPLOAD_BYTES: usize = MAX_SAMPLE_BYTES;
/// Whole-request budget (the body is up to 10 MB over a home uplink).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// Connect budget — a dead network must not hold the UI for two minutes.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Everything the training call can fail with, at the resolution the UI and
/// the retrain flow need.
#[derive(Debug, Clone, PartialEq)]
pub enum CloneTrainError {
    /// No file at the sample path.
    SampleMissing { detail: String },
    /// A file that is not an enrollment take (outside `enroll/`, unreadable
    /// as WAV).
    SampleRejected { detail: String },
    /// Over the vendor's 10 MB cap — refused before any request.
    AudioTooLarge { bytes: usize, max_bytes: usize },
    /// No credential in the environment.
    MissingCredentials,
    /// The vendor's WER gate (45001109): the recording does not read as the
    /// transcript.
    TranscriptMismatch { code: u64 },
    /// Transient (429/5xx/408/425): the caller may retry.
    Retryable { status: Option<u16>, detail: String },
    /// 401/403: our credential is wrong — never retry, fix the config.
    Credentials { status: u16 },
    /// The vendor refused for good; the original code is preserved.
    Terminal {
        status: Option<u16>,
        code: Option<u64>,
        detail: String,
    },
    /// The socket or the request never made it.
    Transport { endpoint: String, detail: String },
    /// Training succeeded but the profile could not be written.
    Store { detail: String },
}

impl CloneTrainError {
    /// Stable identifier for the frontend and tests.
    pub fn code(&self) -> &'static str {
        match self {
            Self::SampleMissing { .. } => "sample_missing",
            Self::SampleRejected { .. } => "sample_rejected",
            Self::AudioTooLarge { .. } => "audio_too_large",
            Self::MissingCredentials => "missing_credentials",
            Self::TranscriptMismatch { .. } => "transcript_mismatch",
            Self::Retryable { .. } => "retryable",
            Self::Credentials { .. } => "credentials",
            Self::Terminal { .. } => "terminal",
            Self::Transport { .. } => "transport",
            Self::Store { .. } => "store",
        }
    }

    /// UI-ready Chinese copy. Never carries a credential or a full URL.
    pub fn message(&self) -> String {
        match self {
            Self::SampleMissing { .. } => "找不到录音文件，请重新录制".to_string(),
            Self::SampleRejected { .. } => "录音文件无效，请重新录制".to_string(),
            Self::AudioTooLarge {
                bytes, max_bytes, ..
            } => format!("录音文件过大（共 {bytes} 字节，上限 {max_bytes} 字节），请缩短录音"),
            Self::MissingCredentials => format!(
                "缺少供应商凭据，请设置 {CLONE_ACCESS_TOKEN_VAR} 或 {FALLBACK_ACCESS_TOKEN_VAR}"
            ),
            Self::TranscriptMismatch { .. } => "录音与文本不匹配，请重录".to_string(),
            Self::Retryable { .. } => "训练服务暂时不可用，请稍后重试".to_string(),
            Self::Credentials { .. } => "访问凭据无效，请检查供应商配置".to_string(),
            Self::Terminal { code, status, .. } => match (code, status) {
                (Some(code), _) => format!("训练被拒绝（错误码 {code}）"),
                (None, Some(status)) => format!("训练被拒绝（状态码 {status}）"),
                (None, None) => "训练被拒绝".to_string(),
            },
            Self::Transport { endpoint, .. } => format!("无法连接训练服务（{endpoint}）"),
            Self::Store { .. } => "音色档案保存失败".to_string(),
        }
    }
}

impl fmt::Display for CloneTrainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for CloneTrainError {}

/// A successful training response.
#[derive(Debug, Clone, PartialEq)]
pub struct TrainOutcome {
    pub http_status: u16,
    /// The vendor's `code`, when the body carried one (0 = success).
    pub code: Option<u64>,
}

/// The HTTP client for the training endpoint, with the credential and the
/// request shape it needs. Tests build one against a loopback mock;
/// production builds one with [`CloneTrainer::from_lookup`].
#[derive(Debug, Clone)]
pub struct CloneTrainer {
    http: reqwest::Client,
    endpoint: String,
    /// Host (+port) and path only — what every error message may name (T-02-17).
    label: String,
    api_key: Option<String>,
}

impl CloneTrainer {
    pub fn new(endpoint: impl Into<String>, api_key: Option<String>) -> Self {
        let endpoint = endpoint.into();
        let label = host_and_path(&endpoint);
        Self {
            http: client_with_timeout(REQUEST_TIMEOUT),
            endpoint,
            label,
            api_key,
        }
    }

    /// Production construction: the fixed verified endpoint plus the
    /// environment credential (`VOLC_CLONE_ACCESS_TOKEN` first, falling back
    /// to `VOLC_TTS_ACCESS_TOKEN`).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        Self::new(VOICE_CLONE_URL, api_key_from_lookup(&lookup))
    }

    /// Override the request budget (tests keep it short).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.http = client_with_timeout(timeout);
        self
    }

    /// Endpoint as host + path — the only form safe to log.
    pub fn endpoint_label(&self) -> &str {
        &self.label
    }

    /// Upload one take and register `speaker_id` with the vendor.
    ///
    /// A single call never auto-retries (test 5 pins this): transient
    /// failures come back as [`CloneTrainError::Retryable`] for the caller to
    /// decide.
    pub async fn train(
        &self,
        audio: &[u8],
        transcript: &str,
        speaker_id: &str,
    ) -> Result<TrainOutcome, CloneTrainError> {
        let api_key = self
            .api_key
            .as_ref()
            .map(|key| key.trim())
            .filter(|key| !key.is_empty())
            .ok_or(CloneTrainError::MissingCredentials)?;

        let body = request_body(
            speaker_id,
            &base64::engine::general_purpose::STANDARD.encode(audio),
            CLONE_AUDIO_FORMAT,
            transcript,
        );

        let response = self
            .http
            .post(&self.endpoint)
            // A fresh id per request — 火山 rejects replays (the mjs draws a
            // new crypto.randomUUID() for exactly this reason).
            .header("X-Api-Key", api_key)
            .header("X-Api-Request-Id", uuid::Uuid::new_v4().to_string())
            .json(&body)
            .send()
            .await
            .map_err(|error| CloneTrainError::Transport {
                endpoint: self.label.clone(),
                detail: transport_reason(&error),
            })?;

        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| CloneTrainError::Transport {
                endpoint: self.label.clone(),
                detail: transport_reason(&error),
            })?;

        // A vendor error may be JSON, HTML or empty; anything that does not
        // parse behaves like a body with no code.
        let parsed: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        classify_response(status, &parsed)
    }
}

/// The request body, exactly as the verified script sends it.
pub fn request_body(speaker_id: &str, audio_base64: &str, format: &str, text: &str) -> Value {
    json!({
        "speaker_id": speaker_id,
        "audio": { "data": audio_base64, "format": format },
        "text": text,
        "language": 0,
    })
}

/// Map one vendor response onto success or the right failure variant.
///
/// Order matters: the credential statuses win over everything, the shared
/// classifier decides transient, then the WER gate is recognised by *code*
/// (it commonly arrives on a 200), then any other non-2xx or nonzero code is
/// terminal with its original code preserved.
pub fn classify_response(status: u16, body: &Value) -> Result<TrainOutcome, CloneTrainError> {
    let code = body.get("code").and_then(Value::as_u64);
    let detail = body
        .get("message")
        .or_else(|| body.get("msg"))
        .and_then(Value::as_str)
        .map(|message| message.chars().take(200).collect())
        .unwrap_or_default();

    if status == 401 || status == 403 {
        return Err(CloneTrainError::Credentials { status });
    }
    if classify_http_status(status) == RetryClass::Retryable {
        return Err(CloneTrainError::Retryable {
            status: Some(status),
            detail,
        });
    }
    if code == Some(WER_GATE_CODE) {
        return Err(CloneTrainError::TranscriptMismatch {
            code: WER_GATE_CODE,
        });
    }
    if (200..300).contains(&status) && code.is_none_or(|code| code == 0) {
        return Ok(TrainOutcome {
            http_status: status,
            code,
        });
    }
    Err(CloneTrainError::Terminal {
        status: Some(status),
        code,
        detail,
    })
}

/// The credential, `VOLC_CLONE_ACCESS_TOKEN` first, `VOLC_TTS_ACCESS_TOKEN`
/// second. Blank values count as missing.
pub fn api_key_from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
    let non_blank = |name: &str| {
        lookup(name)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    non_blank(CLONE_ACCESS_TOKEN_VAR).or_else(|| non_blank(FALLBACK_ACCESS_TOKEN_VAR))
}

/// The speaker id for a new training run: the env override when present,
/// otherwise a generated `S_<32 hex>` (the id is caller-supplied — the probe
/// never invents one, and neither does the app: it *does* choose the id, but
/// only ever this shape).
pub fn next_speaker_id(lookup: impl Fn(&str) -> Option<String>) -> String {
    if let Some(provided) = lookup(NEW_SPEAKER_ID_VAR)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return provided;
    }
    format!("S_{}", uuid::Uuid::new_v4().simple())
}

/// Host (+port) and path from a URL — how an endpoint may appear in errors and
/// logs (T-02-17).
pub fn host_and_path(url: &str) -> String {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    without_scheme
        .split(['?', '#'])
        .next()
        .unwrap_or(without_scheme)
        .trim_end_matches('/')
        .to_string()
}

/// The enrollment take the API will receive, once it passed every local gate.
struct LoadedSample {
    bytes: Vec<u8>,
    duration_s: f64,
}

/// The full training flow: local gates → the one upload → the local profile.
pub async fn train_voice_clone(
    trainer: &CloneTrainer,
    store: &VoiceStore,
    sample_path: &Path,
    transcript: &str,
    speaker_id: &str,
) -> Result<VoiceProfile, CloneTrainError> {
    let sample = load_sample(store, sample_path)?;
    trainer.train(&sample.bytes, transcript, speaker_id).await?;

    // Chain the new generation onto the history. An unreadable old profile is
    // replaced rather than blocking the retrain (the user is retraining
    // *because* something is wrong).
    let previous = match store.load() {
        Ok(Some(old)) => {
            let mut history = Vec::with_capacity(old.previous.len() + 1);
            history.push(old.take());
            history.extend(old.previous);
            history
        }
        Ok(None) => Vec::new(),
        Err(error) => {
            eprintln!("[enroll] replacing an unreadable voice profile: {error}");
            Vec::new()
        }
    };

    let profile = VoiceProfile {
        speaker_id: speaker_id.to_string(),
        resource_id: CLONE_RESOURCE_ID.to_string(),
        created_at: rfc3339_utc(std::time::SystemTime::now()),
        sample_path: sample_path.display().to_string(),
        duration_s: sample.duration_s,
        status: PROFILE_STATUS_READY.to_string(),
        previous,
    };
    store
        .save(&profile)
        .map_err(|error: VoiceStoreError| CloneTrainError::Store {
            detail: error.to_string(),
        })?;
    Ok(profile)
}

/// Every local gate between the recorded take and the upload: the file lives
/// under `enroll/` (T-02-16), exists, fits the vendor's 10 MB cap, and reads
/// as a valid WAV whose duration the profile can record.
fn load_sample(store: &VoiceStore, sample_path: &Path) -> Result<LoadedSample, CloneTrainError> {
    let enroll = enrollment_dir(store.root());
    if !sample_path.starts_with(&enroll) {
        return Err(CloneTrainError::SampleRejected {
            detail: format!("sample is outside {}", enroll.display()),
        });
    }
    let metadata =
        std::fs::metadata(sample_path).map_err(|error| CloneTrainError::SampleMissing {
            detail: error.to_string(),
        })?;
    if !metadata.is_file() {
        return Err(CloneTrainError::SampleMissing {
            detail: "not a file".to_string(),
        });
    }
    // The cap fires before the read: a 10 MB+ take never touches the wire.
    if metadata.len() > MAX_UPLOAD_BYTES as u64 {
        return Err(CloneTrainError::AudioTooLarge {
            bytes: metadata.len() as usize,
            max_bytes: MAX_UPLOAD_BYTES,
        });
    }
    let bytes = std::fs::read(sample_path).map_err(|error| CloneTrainError::SampleMissing {
        detail: error.to_string(),
    })?;

    let reader =
        hound::WavReader::open(sample_path).map_err(|error| CloneTrainError::SampleRejected {
            detail: format!("not a readable WAV: {error}"),
        })?;
    let spec = reader.spec();
    if spec.sample_rate == 0 {
        return Err(CloneTrainError::SampleRejected {
            detail: "WAV reports a zero sample rate".to_string(),
        });
    }
    Ok(LoadedSample {
        bytes,
        duration_s: reader.len() as f64 / spec.sample_rate as f64,
    })
}

fn client_with_timeout(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(timeout)
        .build()
        // A client construction failure is a TLS-backend problem; the default
        // client still works and the request error surfaces normally.
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// A short, URL-free reason for a transport failure (T-02-17: messages name
/// host + path only, and reqwest's Display embeds the full URL).
fn transport_reason(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "timeout".to_string()
    } else if error.is_connect() {
        "connection failed".to_string()
    } else {
        "request failed".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn the_credential_prefers_the_clone_variable() {
        let both = api_key_from_lookup(map_lookup(&[
            (CLONE_ACCESS_TOKEN_VAR, "clone-token"),
            (FALLBACK_ACCESS_TOKEN_VAR, "tts-token"),
        ]));
        assert_eq!(both.as_deref(), Some("clone-token"));

        let fallback = api_key_from_lookup(map_lookup(&[(FALLBACK_ACCESS_TOKEN_VAR, "tts-token")]));
        assert_eq!(fallback.as_deref(), Some("tts-token"));

        let blank = api_key_from_lookup(map_lookup(&[(CLONE_ACCESS_TOKEN_VAR, "   ")]));
        assert_eq!(blank, None, "a blank variable is not a credential");

        assert_eq!(api_key_from_lookup(map_lookup(&[])), None);
    }

    #[test]
    fn speaker_ids_are_generated_or_taken_from_the_environment() {
        let generated = next_speaker_id(map_lookup(&[]));
        assert!(generated.starts_with("S_"), "{generated}");
        assert_eq!(generated.len(), 34, "S_ + 32 hex characters");
        assert_ne!(
            generated,
            next_speaker_id(map_lookup(&[])),
            "each run gets a fresh id"
        );

        let provided = next_speaker_id(map_lookup(&[(NEW_SPEAKER_ID_VAR, "S_custom")]));
        assert_eq!(provided, "S_custom");
    }

    #[test]
    fn classification_follows_status_then_code() {
        // Success shapes: {"code": 0} and a body with no code at all.
        assert_eq!(
            classify_response(200, &json!({ "code": 0 })),
            Ok(TrainOutcome {
                http_status: 200,
                code: Some(0)
            })
        );
        assert_eq!(
            classify_response(200, &json!({})),
            Ok(TrainOutcome {
                http_status: 200,
                code: None
            })
        );

        // The WER gate is recognised even on a 200.
        assert_eq!(
            classify_response(200, &json!({ "code": WER_GATE_CODE })),
            Err(CloneTrainError::TranscriptMismatch {
                code: WER_GATE_CODE
            })
        );
        assert!(matches!(
            classify_response(400, &json!({ "code": WER_GATE_CODE })),
            Err(CloneTrainError::TranscriptMismatch { .. })
        ));

        // Credentials before everything else.
        assert_eq!(
            classify_response(401, &json!({})),
            Err(CloneTrainError::Credentials { status: 401 })
        );
        assert_eq!(
            classify_response(403, &json!({})),
            Err(CloneTrainError::Credentials { status: 403 })
        );

        // Transient statuses.
        assert!(matches!(
            classify_response(429, &json!({ "message": "slow down" })),
            Err(CloneTrainError::Retryable {
                status: Some(429),
                ..
            })
        ));
        assert!(matches!(
            classify_response(503, &json!({})),
            Err(CloneTrainError::Retryable {
                status: Some(503),
                ..
            })
        ));

        // Anything else is terminal, code preserved.
        assert_eq!(
            classify_response(400, &json!({ "code": 12345678, "message": "bad" })),
            Err(CloneTrainError::Terminal {
                status: Some(400),
                code: Some(12345678),
                detail: "bad".to_string(),
            })
        );
        assert!(matches!(
            classify_response(200, &json!({ "code": 40000001 })),
            Err(CloneTrainError::Terminal {
                code: Some(40000001),
                ..
            })
        ));
    }

    #[test]
    fn the_request_body_is_exactly_the_verified_shape() {
        let body = request_body("S_x", "QUJD", "wav", "文本");
        assert_eq!(body["speaker_id"], json!("S_x"));
        assert_eq!(body["audio"]["data"], json!("QUJD"));
        assert_eq!(body["audio"]["format"], json!("wav"));
        assert_eq!(body["text"], json!("文本"));
        assert_eq!(body["language"], json!(0));
        assert_eq!(body.as_object().expect("object").len(), 4);
    }

    #[test]
    fn endpoint_labels_never_carry_a_scheme_or_query() {
        assert_eq!(
            host_and_path("https://openspeech.bytedance.com/api/v3/tts/voice_clone"),
            "openspeech.bytedance.com/api/v3/tts/voice_clone"
        );
        assert_eq!(
            host_and_path("http://127.0.0.1:8787/api/v3/tts/voice_clone/"),
            "127.0.0.1:8787/api/v3/tts/voice_clone"
        );
        assert_eq!(host_and_path("ws://h/x?token=secret"), "h/x");
    }

    #[test]
    fn messages_stay_credential_free() {
        assert!(!CloneTrainError::MissingCredentials
            .message()
            .contains("secret"));
        let large = CloneTrainError::AudioTooLarge {
            bytes: 11_000_000,
            max_bytes: MAX_UPLOAD_BYTES,
        };
        assert!(large.message().contains("11000000"));
        assert!(!large.message().contains("http"));
    }
}
