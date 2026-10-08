//! 火山 Seed-ICL 2.0 streaming synthesis — English text in, the user's cloned
//! voice out (T2.5).
//!
//! Ported field-by-field from `tools/vendor-experiments/volc-tts-stream.mjs`,
//! the script that was verified against the service on 2026-09-29.
//!
//! # Wire format
//!
//! ```text
//! client → one binary frame, then read:
//!   [0x11, 0x10, 0x10, 0x00] + u32be(len) + JSON
//!   {"user":{"uid"},"req_params":{"text","speaker","audio_params":{...}}}
//!   handshake: X-Api-Key / X-Api-Resource-Id / X-Api-Request-Id
//!
//! server → [0x11, msgType<<4, 0x10, 0x00] + …
//!   msgType = (data[1] >> 4) & 0x0f   0b1111 error | 0b1011 audio | 0b1001 json
//!   error:  u32be(code) @4, u32be(size) @8, message
//!   other:  u32be(event) @4, u32be(sid_len) @8, sid, u32be(payload_len), payload
//!   event 352 = audio for this request, 152 = session finished
//! ```
//!
//! # The cross-lingual risk (research correction 5)
//!
//! `.env.example` says an ICL 2.0 clone "only supports synthesis in the same
//! language as the training audio", while the synthesis parameters document a
//! cross-lingual path. **The two keys below are that path**: with
//! `explicit_language: "en"` and `tone_fidelity: false` an English sentence
//! goes to a clone trained on Chinese. This client sends them explicitly — it
//! never relies on a server default — but it does **not** assume they work:
//! the probe in 02-04 (T4.0) is the arbiter, and if it fails, D-11's
//! single-vendor conclusion fails with it.
//!
//! `format` is `pcm`, not the experiment script's `mp3`: the cascade consumes
//! PCM directly (24 kHz mono, the rate 02-05's playout chain resamples from).

use std::sync::{Arc, Mutex, MutexGuard};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use crate::pipeline::budget::Stage;

use super::config::{Endpoint, Endpoints, VolcCredentials};
use super::error::{ErrorKind, RetryClass, StageError};
use super::traits::{
    AudioChunk, MarkHandle, TtsEvent, TtsSink, TtsStream, TtsUsage, VoiceRef, EVENT_QUEUE_ITEMS,
};

// ---------------------------------------------------------------------------
// protocol constants
// ---------------------------------------------------------------------------

/// Synthesis sample rate: 24 kHz mono PCM16LE.
pub const SAMPLE_RATE_HZ: u32 = 24_000;

pub const AUDIO_FORMAT: &str = "pcm";

/// The synthesis text's language. Cross-lingual path — see the module docs.
pub const EXPLICIT_LANGUAGE: &str = "en";

/// `tone_fidelity: false` (还原模式) is **mandatory** for cross-lingual text.
pub const TONE_FIDELITY: bool = false;

/// `X-Api-Resource-Id` for a cloned voice.
pub const RESOURCE_CLONE: &str = "seed-icl-2.0";

/// `X-Api-Resource-Id` for a preset voice.
pub const RESOURCE_PRESET: &str = "seed-tts-2.0";

/// Audio payloads arrive under this event.
pub const EVT_TTS_RESPONSE: u32 = 352;

/// The session's terminal frame.
pub const EVT_SESSION_FINISHED: u32 = 152;

/// The only `status_code` on the terminal frame that means success.
pub const STATUS_SUCCESS: u32 = 20_000_000;

pub const MSG_TYPE_AUDIO: u8 = 0b1011;
pub const MSG_TYPE_JSON: u8 = 0b1001;
pub const MSG_TYPE_ERROR: u8 = 0b1111;

/// The four bytes every frame starts with.
pub const FRAME_HEADER: [u8; 4] = [0x11, 0x10, 0x10, 0x00];

pub const HEADER_API_KEY: &str = "X-Api-Key";
pub const HEADER_RESOURCE_ID: &str = "X-Api-Resource-Id";
pub const HEADER_REQUEST_ID: &str = "X-Api-Request-Id";

const PROVIDER: &str = "volc";

// ---------------------------------------------------------------------------
// the request
// ---------------------------------------------------------------------------

/// Which resource serves this voice: a clone rides the ICL resource, a preset
/// the plain TTS one. The wrong id is a handshake rejection, not a quality
/// difference.
pub fn resource_id_for(voice: &VoiceRef) -> &'static str {
    match voice {
        VoiceRef::Clone(_) => RESOURCE_CLONE,
        VoiceRef::Preset(_) => RESOURCE_PRESET,
    }
}

/// The vendor-side speaker name for this voice (`S_xxx` for a clone).
pub fn speaker_of(voice: &VoiceRef) -> &str {
    match voice {
        VoiceRef::Clone(speaker) => speaker.as_str(),
        VoiceRef::Preset(name) => name,
    }
}

/// The one JSON body a synthesis request carries.
///
/// `uid` is the account id — 火山 uses it for attribution; no user identifier
/// leaves this process. `explicit_language` and `tone_fidelity` are always
/// sent: the cross-lingual path must never depend on a server default (see the
/// module docs for the risk that path carries).
pub fn request_body(uid: &str, text: &str, speaker: &str) -> Value {
    request_body_for(uid, text, speaker, Some(EXPLICIT_LANGUAGE))
}

/// The body with an explicit language choice: `Some(language)` is the
/// cross-lingual path (`explicit_language` + `tone_fidelity: false`);
/// `None` omits both keys and leaves the vendor's same-language defaults
/// alone — the params the T4.0 probe's `clone-zh` reference used.
pub fn request_body_for(uid: &str, text: &str, speaker: &str, language: Option<&str>) -> Value {
    let mut audio_params = json!({
        "format": AUDIO_FORMAT,
        "sample_rate": SAMPLE_RATE_HZ,
    });
    if let Some(language) = language {
        audio_params["explicit_language"] = json!(language);
        audio_params["tone_fidelity"] = json!(TONE_FIDELITY);
    }
    json!({
        "user": { "uid": uid },
        "req_params": {
            "text": text,
            "speaker": speaker,
            "audio_params": audio_params,
        }
    })
}

/// `[0x11, 0x10, 0x10, 0x00] + u32be(len) + JSON` — the request frame.
pub fn request_frame(uid: &str, text: &str, speaker: &str) -> Vec<u8> {
    request_frame_for(uid, text, speaker, Some(EXPLICIT_LANGUAGE))
}

/// The frame for one language choice (see [`request_body_for`]).
pub fn request_frame_for(uid: &str, text: &str, speaker: &str, language: Option<&str>) -> Vec<u8> {
    let payload = request_body_for(uid, text, speaker, language).to_string();
    let mut frame = Vec::with_capacity(FRAME_HEADER.len() + 4 + payload.len());
    frame.extend_from_slice(&FRAME_HEADER);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload.as_bytes());
    frame
}

/// The three handshake headers, with a **fresh request id per connection**:
/// 火山 correlates support traces by it, so a reused id would make two
/// connections indistinguishable in their logs.
pub fn handshake_headers(
    credentials: &VolcCredentials,
    voice: &VoiceRef,
) -> Vec<(&'static str, String)> {
    vec![
        (
            HEADER_API_KEY,
            credentials.access_token.expose().to_string(),
        ),
        (HEADER_RESOURCE_ID, resource_id_for(voice).to_string()),
        (HEADER_REQUEST_ID, uuid::Uuid::new_v4().to_string()),
    ]
}

// ---------------------------------------------------------------------------
// the response
// ---------------------------------------------------------------------------

/// One frame from the server. The type is the high nibble of byte 1:
/// `0b1111` error, `0b1011` audio, `0b1001` JSON.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerFrame {
    /// A session-level error frame carrying its own code.
    Error { code: u32, message: String },
    /// Raw little-endian PCM16 audio.
    Audio { event: u32, pcm16_le: Vec<u8> },
    /// Any JSON frame. `body` is `None` when the payload is empty or does not
    /// parse — the experiment script's `catch { json = null }`.
    Json { event: u32, body: Option<Value> },
}

/// Parse one binary frame.
///
/// Every length is bounds-checked: a truncated frame is a retryable protocol
/// failure, never a panic. (The experiment script would throw here; a stage
/// client may not — an uncaught throw inside the driver would kill the stage
/// silently.)
pub fn parse_frame(data: &[u8]) -> Result<ServerFrame, StageError> {
    let msg_type = data
        .get(1)
        .map(|byte| (byte >> 4) & 0x0f)
        .ok_or_else(|| truncated("frame header"))?;

    if msg_type == MSG_TYPE_ERROR {
        let code = read_u32(data, 4)?;
        let size = read_u32(data, 8)? as usize;
        let end = 12usize
            .checked_add(size)
            .ok_or_else(|| truncated("error message"))?;
        let message = data
            .get(12..end)
            .ok_or_else(|| truncated("error message"))?;
        return Ok(ServerFrame::Error {
            code,
            message: String::from_utf8_lossy(message).into_owned(),
        });
    }

    let event = read_u32(data, 4)?;
    let sid_len = read_u32(data, 8)? as usize;
    let payload_len_at = 12usize
        .checked_add(sid_len)
        .ok_or_else(|| truncated("session id"))?;
    let payload_len = read_u32(data, payload_len_at)? as usize;
    let payload_start = payload_len_at
        .checked_add(4)
        .ok_or_else(|| truncated("payload length"))?;
    let payload_end = payload_start
        .checked_add(payload_len)
        .ok_or_else(|| truncated("payload length"))?;
    let payload = data
        .get(payload_start..payload_end)
        .ok_or_else(|| truncated("payload"))?;

    match msg_type {
        MSG_TYPE_AUDIO => Ok(ServerFrame::Audio {
            event,
            pcm16_le: payload.to_vec(),
        }),
        _ => Ok(ServerFrame::Json {
            event,
            body: serde_json::from_slice(payload).ok(),
        }),
    }
}

/// The finished frame's `status_code`: `20000000` is the only success. Anything
/// else means the audio the caller already received is not the whole answer,
/// so the request failed as a whole.
pub fn session_status(status_code: u32) -> Result<(), StageError> {
    if status_code == STATUS_SUCCESS {
        return Ok(());
    }
    Err(StageError::vendor(
        PROVIDER,
        ErrorKind::Vendor,
        format!("the session ended with status_code {status_code}"),
    ))
}

/// The accounting frame (D-13), `None` when the vendor sent none. A missing
/// `text_words` counts as zero rather than dropping the whole frame — the
/// characters are the part the cost model needs.
pub fn usage_from(body: &Value) -> Option<TtsUsage> {
    let usage = body.get("usage")?;
    let number = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0) as u32;
    Some(TtsUsage {
        characters: number("characters"),
        text_words: number("text_words"),
    })
}

fn truncated(what: &str) -> StageError {
    StageError::protocol(
        PROVIDER,
        format!("truncated frame: the {what} is cut short"),
    )
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, StageError> {
    let end = offset.checked_add(4).ok_or_else(|| truncated("length"))?;
    let bytes = data.get(offset..end).ok_or_else(|| truncated("length"))?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

// ---------------------------------------------------------------------------
// the client
// ---------------------------------------------------------------------------

/// 火山 Seed-ICL 2.0 streaming synthesis.
#[derive(Debug, Clone)]
pub struct VolcTts {
    credentials: VolcCredentials,
    endpoints: Endpoints,
    /// The resource the most recent request used. A preset and a clone are
    /// served by different resources, so the trace (D-16) must report the one
    /// that actually produced the audio.
    last_resource: Arc<Mutex<String>>,
    /// The synthesis language choice. The cascade's text is always English
    /// (translation output) → the cross-lingual path is the default; the
    /// enrollment preview's Chinese sample opts out via [`Self::same_language`].
    language: Option<String>,
    marks: MarkHandle,
}

impl VolcTts {
    pub fn new(credentials: VolcCredentials, endpoints: Endpoints) -> Self {
        let last_resource = credentials.resource_id.clone();
        Self {
            credentials,
            endpoints,
            last_resource: Arc::new(Mutex::new(last_resource)),
            language: Some(EXPLICIT_LANGUAGE.to_string()),
            marks: MarkHandle::disabled(),
        }
    }

    /// Synthesize in the voice's own language: no `explicit_language`, no
    /// `tone_fidelity` — the vendor defaults, exactly what the T4.0 probe sent
    /// for its `clone-zh` reference take.
    pub fn same_language(mut self) -> Self {
        self.language = None;
        self
    }

    pub fn from_env() -> Result<Self, StageError> {
        Ok(Self::new(
            VolcCredentials::from_env()?,
            Endpoints::from_env(),
        ))
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, StageError> {
        let credentials = VolcCredentials::from_lookup(&lookup)?;
        Ok(Self::new(credentials, Endpoints::from_lookup(lookup)))
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoints.volc_ws
    }

    fn set_last_resource(&self, resource: &str) {
        *self.last_resource_guard() = resource.to_string();
    }

    /// Same reasoning as the playout queue and the AEC: the guarded data is one
    /// plain string (the resource id the last request used), so recovering a
    /// poisoned lock beats killing the sentence that is being spoken (WR-06).
    fn last_resource_guard(&self) -> MutexGuard<'_, String> {
        self.last_resource
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

impl TtsSink for VolcTts {
    fn provider(&self) -> &'static str {
        PROVIDER
    }

    fn model_version(&self) -> String {
        self.last_resource_guard().clone()
    }

    fn set_marks(&mut self, marks: MarkHandle) {
        self.marks = marks;
    }

    fn synthesize(
        &mut self,
        text: &str,
        voice: &VoiceRef,
        _epoch: u64,
    ) -> Result<TtsStream, StageError> {
        let resource = resource_id_for(voice);
        self.set_last_resource(resource);

        let (sender, events) = mpsc::channel(EVENT_QUEUE_ITEMS);
        tokio::spawn(run_session(RunContext {
            url: self.endpoint().url().to_string(),
            headers: handshake_headers(&self.credentials, voice),
            frame: request_frame_for(
                &self.credentials.app_id,
                text,
                speaker_of(voice),
                self.language.as_deref(),
            ),
            marks: self.marks.clone(),
            sender,
        }));
        Ok(TtsStream::new(events))
    }
}

/// Everything one synthesis session needs. The credential travels as a header
/// value, never in the URL, so a `StageError` cannot pick it up.
struct RunContext {
    url: String,
    headers: Vec<(&'static str, String)>,
    frame: Vec<u8>,
    marks: MarkHandle,
    sender: mpsc::Sender<TtsEvent>,
}

async fn run_session(context: RunContext) {
    if let Err(error) = stream_session(&context).await {
        // The classified failure is the only signal the caller's retry/breaker
        // classification keys on (WR-03): wait for a slot instead of dropping
        // it on a full queue. A send failure means the caller is gone; nothing
        // left to report to.
        let _ = context.sender.send(TtsEvent::Failed(error)).await;
    }
}

async fn stream_session(context: &RunContext) -> Result<(), StageError> {
    let mut request = context.url.clone().into_client_request().map_err(|error| {
        StageError::new(
            PROVIDER,
            ErrorKind::Config,
            RetryClass::Terminal,
            format!("the synthesis URL is not a valid request: {error}"),
        )
    })?;
    for (name, value) in &context.headers {
        let value = HeaderValue::from_str(value).map_err(|_| {
            StageError::new(
                PROVIDER,
                ErrorKind::Config,
                RetryClass::Client,
                "a handshake header value contains invalid characters",
            )
        })?;
        request.headers_mut().insert(*name, value);
    }

    let mut socket = match tokio_tungstenite::connect_async(request).await {
        Ok((socket, _response)) => socket,
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            return Err(
                StageError::http(PROVIDER, response.status().as_u16()).with_endpoint(&context.url)
            )
        }
        Err(error) => {
            return Err(
                StageError::transport(PROVIDER, format!("connect failed: {error}"))
                    .with_endpoint(&context.url),
            )
        }
    };

    socket
        .send(Message::Binary(context.frame.clone().into()))
        .await
        .map_err(|error| StageError::transport(PROVIDER, format!("send failed: {error}")))?;

    let mut marked = false;
    while let Some(message) = socket.next().await {
        let message = message
            .map_err(|error| StageError::transport(PROVIDER, format!("socket error: {error}")))?;
        let Message::Binary(bytes) = message else {
            continue;
        };
        match parse_frame(&bytes)? {
            ServerFrame::Error { code, message } => {
                return Err(StageError::vendor(
                    PROVIDER,
                    ErrorKind::Vendor,
                    format!("error frame {code}: {message}"),
                ))
            }
            ServerFrame::Audio { event, pcm16_le } if event == EVT_TTS_RESPONSE => {
                if !marked {
                    context.marks.mark(Stage::TtsFirstAudio);
                    marked = true;
                }
                let chunk = AudioChunk::from_pcm16_le(&pcm16_le, SAMPLE_RATE_HZ);
                send(&context.sender, TtsEvent::Audio(chunk)).await?;
            }
            // Other audio events are not this stage's business (the experiment
            // script logs and ignores them).
            ServerFrame::Audio { .. } => {}
            ServerFrame::Json {
                event: EVT_SESSION_FINISHED,
                body,
            } => {
                let Some(body) = body else {
                    return Err(StageError::protocol(
                        PROVIDER,
                        "the finished frame carried no body",
                    ));
                };
                let status = body
                    .get("status_code")
                    .and_then(Value::as_u64)
                    .unwrap_or_default() as u32;
                session_status(status)?;
                send(
                    &context.sender,
                    TtsEvent::Finished {
                        usage: usage_from(&body),
                    },
                )
                .await?;
                // The server closes after this frame; dropping the socket here
                // saves the close handshake from the 2 s budget.
                return Ok(());
            }
            // Session-level JSON events carry nothing this stage consumes.
            ServerFrame::Json { .. } => {}
        }
    }

    Err(StageError::transport(
        PROVIDER,
        "the socket closed before SESSION_FINISHED",
    ))
}

/// Hands one event to the caller, waiting for a slot (WR-03).
///
/// `try_send` conflated "the queue is momentarily full" with "the caller is
/// gone" and dropped the audio chunk or the terminal 完成 event in the first
/// case — a dropped chunk is an audible click, a dropped 完成 is a session that
/// never ends. The await is safe here: this is the async synthesis task, not an
/// audio callback with a deadline.
async fn send(sender: &mpsc::Sender<TtsEvent>, event: TtsEvent) -> Result<(), StageError> {
    sender
        .send(event)
        .await
        .map_err(|_| StageError::transport(PROVIDER, "the caller stopped reading the audio"))
}

#[cfg(test)]
mod tests {
    use super::super::config::{Endpoints, Secret};
    use super::super::traits::SpeakerId;
    use super::*;

    // -----------------------------------------------------------------------
    // frame parity with tools/vendor-experiments/volc-tts-stream.mjs
    // -----------------------------------------------------------------------

    /// The experiment script's `parse()` walks exactly this layout; the bytes
    /// below are built the same way its server frames were observed.
    fn server_frame(msg_type: u8, event: u32, payload: &[u8]) -> Vec<u8> {
        let sid = b"sid-1";
        let mut frame = vec![0x11, msg_type << 4, 0x10, 0x00];
        frame.extend_from_slice(&event.to_be_bytes());
        frame.extend_from_slice(&(sid.len() as u32).to_be_bytes());
        frame.extend_from_slice(sid);
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    fn error_frame(code: u32, message: &str) -> Vec<u8> {
        let mut frame = vec![0x11, MSG_TYPE_ERROR << 4, 0x10, 0x00];
        frame.extend_from_slice(&code.to_be_bytes());
        frame.extend_from_slice(&(message.len() as u32).to_be_bytes());
        frame.extend_from_slice(message.as_bytes());
        frame
    }

    #[test]
    fn an_audio_frame_parses_to_its_event_and_payload() {
        // Test 1: `msgType = (data[1] >> 4) & 0x0f` picks 0b1011 out of 0xB0.
        let pcm = [0x01u8, 0x02, 0x03, 0x04];
        let frame = server_frame(MSG_TYPE_AUDIO, EVT_TTS_RESPONSE, &pcm);
        assert_eq!(
            frame[1] >> 4 & 0x0f,
            MSG_TYPE_AUDIO,
            "the nibble is the type"
        );
        match parse_frame(&frame).expect("audio frame") {
            ServerFrame::Audio { event, pcm16_le } => {
                assert_eq!(event, EVT_TTS_RESPONSE);
                assert_eq!(pcm16_le, pcm);
            }
            other => panic!("expected audio, got {other:?}"),
        }
    }

    #[test]
    fn a_json_frame_parses_to_its_body() {
        let body = json!({ "status_code": STATUS_SUCCESS, "usage": { "characters": 42 } });
        let frame = server_frame(
            MSG_TYPE_JSON,
            EVT_SESSION_FINISHED,
            body.to_string().as_bytes(),
        );
        match parse_frame(&frame).expect("json frame") {
            ServerFrame::Json { event, body } => {
                assert_eq!(event, EVT_SESSION_FINISHED);
                let body = body.expect("a parsed body");
                assert_eq!(body["status_code"], json!(STATUS_SUCCESS));
                assert_eq!(body["usage"]["characters"], json!(42));
            }
            other => panic!("expected json, got {other:?}"),
        }
    }

    #[test]
    fn a_json_frame_with_an_unparseable_body_keeps_its_event() {
        // The script's `catch { json = null }`: the event still reaches the
        // caller, the body does not.
        let frame = server_frame(MSG_TYPE_JSON, 999, b"{not json");
        match parse_frame(&frame).expect("frame") {
            ServerFrame::Json { event, body } => {
                assert_eq!(event, 999);
                assert!(body.is_none(), "the body is dropped, the event is not");
            }
            other => panic!("expected json, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_json_body_is_not_an_error() {
        let frame = server_frame(MSG_TYPE_JSON, 450, b"");
        match parse_frame(&frame).expect("frame") {
            ServerFrame::Json { body, .. } => assert!(body.is_none()),
            other => panic!("expected json, got {other:?}"),
        }
    }

    #[test]
    fn an_error_frame_carries_its_code_and_message() {
        let frame = error_frame(45_000_001, "invalid speaker");
        match parse_frame(&frame).expect("error frame") {
            ServerFrame::Error { code, message } => {
                assert_eq!(code, 45_000_001);
                assert_eq!(message, "invalid speaker");
            }
            other => panic!("expected an error, got {other:?}"),
        }
    }

    #[test]
    fn a_truncated_frame_is_a_retryable_protocol_failure() {
        // The script's `readUInt32BE` would throw; here it must classify.
        let frame = server_frame(MSG_TYPE_AUDIO, EVT_TTS_RESPONSE, &[0u8; 8]);
        for cut in [1usize, 3, 5, frame.len() - 1] {
            let error = parse_frame(&frame[..cut]).expect_err("truncated");
            assert_eq!(error.provider, PROVIDER);
            assert_eq!(
                error.retry_class,
                super::super::error::RetryClass::Retryable
            );
        }
    }

    // -----------------------------------------------------------------------
    // the request
    // -----------------------------------------------------------------------

    #[test]
    fn the_request_body_carries_the_cross_lingual_parameters() {
        // Test 3.
        let body = request_body("app-1", "The query took 800 ms.", "S_9k337yqg2");
        assert_eq!(body["user"]["uid"], "app-1");
        let params = &body["req_params"];
        assert_eq!(params["text"], "The query took 800 ms.");
        assert_eq!(params["speaker"], "S_9k337yqg2");
        assert_eq!(params["audio_params"]["format"], "pcm");
        assert_eq!(params["audio_params"]["sample_rate"], json!(24_000));
        // The two keys the cross-lingual path depends on must be explicit.
        assert_eq!(params["audio_params"]["explicit_language"], "en");
        assert_eq!(params["audio_params"]["tone_fidelity"], json!(false));
    }

    #[test]
    fn the_same_language_body_leaves_the_cross_lingual_keys_out() {
        // The T4.0 probe's clone-zh reference take used no language params;
        // the enrollment preview's Chinese sample sends exactly the same.
        let body = request_body_for("app-1", "这是试听。", "S_9k337yqg2", None);
        let audio = &body["req_params"]["audio_params"];
        assert_eq!(audio["format"], "pcm");
        assert_eq!(audio["sample_rate"], json!(24_000));
        assert!(audio.get("explicit_language").is_none(), "{audio}");
        assert!(audio.get("tone_fidelity").is_none(), "{audio}");
    }

    #[test]
    fn the_request_frame_is_the_preamble_a_length_and_the_json() {
        // Test 2.
        let frame = request_frame("app-1", "hello", "S_test");
        assert_eq!(&frame[..4], &FRAME_HEADER);
        let len = u32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]) as usize;
        assert_eq!(len, frame.len() - 8, "the length covers the JSON only");
        let payload: Value = serde_json::from_slice(&frame[8..]).expect("the payload is JSON");
        assert_eq!(payload["req_params"]["text"], "hello");
        assert_eq!(payload["req_params"]["speaker"], "S_test");
    }

    #[test]
    fn the_resource_header_follows_the_voice_kind() {
        // Test 2: Clone → seed-icl-2.0, Preset → seed-tts-2.0.
        let clone = VoiceRef::Clone(SpeakerId::new("S_9k337yqg2"));
        let preset = VoiceRef::Preset("zh_female_vv_uranus_bigtts".to_string());
        assert_eq!(resource_id_for(&clone), RESOURCE_CLONE);
        assert_eq!(resource_id_for(&preset), RESOURCE_PRESET);
        assert_eq!(speaker_of(&clone), "S_9k337yqg2");
        assert_eq!(speaker_of(&preset), "zh_female_vv_uranus_bigtts");
    }

    #[test]
    fn every_handshake_carries_a_fresh_request_id() {
        // Test 2: one new UUID per connection.
        let credentials = VolcCredentials {
            app_id: "app-1".to_string(),
            access_token: Secret::new("test-volc-token"),
            resource_id: RESOURCE_CLONE.to_string(),
            preset_voice: None,
            clone_speaker: None,
        };
        let voice = VoiceRef::Clone(SpeakerId::new("S_test"));
        let first = handshake_headers(&credentials, &voice);
        let second = handshake_headers(&credentials, &voice);

        let header = |headers: &[(&'static str, String)], name: &str| {
            headers
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| panic!("{name} is missing"))
        };
        assert_eq!(header(&first, HEADER_API_KEY), "test-volc-token");
        assert_eq!(header(&first, HEADER_RESOURCE_ID), RESOURCE_CLONE);
        let one = header(&first, HEADER_REQUEST_ID);
        let two = header(&second, HEADER_REQUEST_ID);
        assert_ne!(one, two, "a new request id per connection");
        assert_eq!(one.len(), 36, "a UUID: {one}");
        assert_eq!(one.matches('-').count(), 4, "a UUID: {one}");
    }

    /// WR-06: a poisoned resource lock must not kill the sentence. Every other
    /// lock on the audio path (playout queue, AEC) recovers; this client
    /// aborted the TTS stage instead. Poison it through a genuinely panicking
    /// holder — the way any future holder can, `std` decides, not this crate —
    /// and assert both readers survive.
    #[test]
    fn a_poisoned_resource_lock_does_not_silence_the_sentence() {
        let tts = VolcTts::new(
            VolcCredentials {
                app_id: "app-1".to_string(),
                access_token: Secret::new("test-volc-token"),
                resource_id: RESOURCE_CLONE.to_string(),
                preset_voice: None,
                clone_speaker: None,
            },
            Endpoints::defaults(),
        );
        assert_eq!(tts.model_version(), RESOURCE_CLONE);

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = tts.last_resource.lock().expect("a fresh lock");
            panic!("a holder panicked with the resource lock held");
        }));
        assert!(panicked.is_err());
        assert!(tts.last_resource.is_poisoned(), "the lock is poisoned now");

        // The whole point: the stage keeps its voice instead of aborting.
        tts.set_last_resource("seed-icl-2.0-alt");
        assert_eq!(tts.model_version(), "seed-icl-2.0-alt");
    }

    #[test]
    fn the_missing_credentials_error_names_the_variables_only() {
        // Test 8.
        let error = VolcCredentials::from_lookup(|_| None).expect_err("no credentials");
        let rendered = error.to_string();
        assert!(rendered.contains("VOLC_TTS_APP_ID"), "{rendered}");
        assert!(rendered.contains("VOLC_TTS_ACCESS_TOKEN"), "{rendered}");
    }

    #[test]
    fn the_client_never_renders_its_access_token() {
        // Test 8: the credential must not survive a Debug or an error string.
        let client = VolcTts::new(
            VolcCredentials {
                app_id: "app-1".to_string(),
                access_token: Secret::new("volc-secret-token-value"),
                resource_id: RESOURCE_CLONE.to_string(),
                preset_voice: None,
                clone_speaker: None,
            },
            Endpoints::defaults(),
        );
        let rendered = format!("{client:?}");
        assert!(!rendered.contains("volc-secret-token-value"), "{rendered}");
        assert!(!client.model_version().contains("volc-secret-token-value"));
    }

    #[test]
    fn a_failed_session_status_is_terminal_failure() {
        // Test 5: `status_code != 20000000` ends the request in failure.
        assert!(session_status(STATUS_SUCCESS).is_ok());
        let error = session_status(45_001_109).expect_err("45_001_109 is not success");
        assert_eq!(error.provider, PROVIDER);
        assert_eq!(error.retry_class, super::super::error::RetryClass::Terminal);
        assert!(error.to_string().contains("45001109"), "{error}");
    }

    #[test]
    fn the_usage_frame_is_read_from_the_finished_body() {
        // Test 6 (D-13).
        let usage = usage_from(&json!({
            "status_code": STATUS_SUCCESS,
            "usage": { "characters": 42, "text_words": 11 }
        }))
        .expect("usage");
        assert_eq!(
            usage,
            TtsUsage {
                characters: 42,
                text_words: 11,
            }
        );
        assert!(usage_from(&json!({ "status_code": STATUS_SUCCESS })).is_none());
    }
}
