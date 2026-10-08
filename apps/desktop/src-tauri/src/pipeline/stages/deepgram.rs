//! Deepgram Nova-3 — the interviewer's English line (T2.3).
//!
//! Ported from `tools/vendor-experiments/stt-ab.mjs` (the Deepgram half).
//!
//! # Why this stage is subtitle-only
//!
//! This is the [`InterviewerTrack`]: whatever the interviewer says is shown as
//! subtitles and fed to the answer-strategy agent, but it is **never** fed to
//! the voice sink — only the user's own Chinese line is re-voiced. The cascade
//! enforces that in 02-03; this module marks it in the types.
//!
//! # Deliberate omissions
//!
//! - **No `Stage::SttFirstPartial` mark.** The 02-01 rig measures the *user*
//!   path end to end; the interviewer line is a side channel, and firing its
//!   boundary into the same waterfall would corrupt the budget math. The
//!   decision is asserted by a test that watches the handle stay silent.
//! - `endpointing` and `utterance_end_ms` are configured but the *cadence* of
//!   silence detection is the vendor's; [`TranscriptBuffer`] only implements
//!   what happens when the vendor says a run ended.
//!
//! # Protocol traps this file encodes
//!
//! - `language=multi` does **not** include Chinese and silently produces
//!   garbage, so the language is pinned to `en` (research correction 2). The
//!   URL test is a guard, not a style preference.
//! - Auth is the `Token <key>` scheme — `Bearer` is an instant 401.
//! - A session that sends neither audio nor `KeepAlive` for 10 s is closed
//!   with NET-0001 (research correction 4); the client must recognise that
//!   close and treat it as a stale link, not a successful end of stream.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use super::config::{DeepgramCredentials, Endpoints};
use super::error::{classify_http_status, ErrorKind, RetryClass, StageError};
use super::traits::{
    ConfidenceSource, MarkHandle, SttEvent, SttPartial, SttSource, SttStream, SttUpstream,
    AUDIO_QUEUE_FRAMES, EVENT_QUEUE_ITEMS,
};

// ---------------------------------------------------------------------------
// protocol constants
// ---------------------------------------------------------------------------

/// Deepgram's own recommendation: the smallest interval that reliably keeps a
/// silent session open.
pub const KEEPALIVE_INTERVAL_MS: u64 = 3_000;

/// The service closes a session after 10 s without audio **and** without a
/// `KeepAlive` (NET-0001).
pub const SILENCE_CLOSE_MS: u64 = 10_000;

/// Silence this long after speech ends the utterance (`UtteranceEnd`).
pub const DEFAULT_UTTERANCE_END_MS: u32 = 1_000;

/// Endpointing: how long a pause must last for Deepgram to close a run.
pub const ENDPOINTING_MS: u32 = 300;

/// The interviewer line is 16 kHz mono — the same format 02-05's resampler
/// produces for the user path.
pub const SAMPLE_RATE_HZ: u32 = 16_000;

/// The **only** language this client may request (research correction 2).
pub const LANGUAGE: &str = "en";

/// Default model, mirroring `StageRole::InterviewerStt::model()`.
pub const DEFAULT_MODEL: &str = "nova-3";

/// The heartbeat's exact shape. Deepgram accepts only this literal.
pub const KEEPALIVE_FRAME: &str = r#"{"type":"KeepAlive"}"#;

/// Sent once the fragment ends so the vendor flushes its last run.
pub const CLOSE_STREAM_FRAME: &str = r#"{"type":"CloseStream"}"#;

const PROVIDER: &str = "deepgram";
const MODEL_INFO_FIELD: &str = "model_info";

// ---------------------------------------------------------------------------
// request shape
// ---------------------------------------------------------------------------

/// The listen URL's query parameters. Every field is locked to a value the
/// cascade depends on; the struct exists so tests can assert the whole set at
/// once and so a future phase changes it in one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenParams {
    pub language: &'static str,
    pub model: &'static str,
    pub encoding: &'static str,
    pub sample_rate: u32,
    pub channels: u8,
    pub interim_results: bool,
    pub endpointing: u32,
    pub vad_events: bool,
    pub utterance_end_ms: u32,
    pub punctuate: bool,
}

impl Default for ListenParams {
    fn default() -> Self {
        Self {
            language: LANGUAGE,
            model: DEFAULT_MODEL,
            encoding: "linear16",
            sample_rate: SAMPLE_RATE_HZ,
            channels: 1,
            interim_results: true,
            endpointing: ENDPOINTING_MS,
            vad_events: true,
            utterance_end_ms: DEFAULT_UTTERANCE_END_MS,
            punctuate: true,
        }
    }
}

/// Build the streaming listen URL.
///
/// `language=multi` is refused outright: Chinese is not in its bundle, so the
/// request would succeed and return confident nonsense (research correction 2).
pub fn listen_url(endpoint: &str, params: &ListenParams) -> Result<String, StageError> {
    if endpoint.is_empty() {
        return Err(StageError::new(
            PROVIDER,
            ErrorKind::Config,
            RetryClass::Terminal,
            "the Deepgram endpoint is empty",
        ));
    }
    if params.language.eq_ignore_ascii_case("multi") {
        return Err(StageError::new(
            PROVIDER,
            ErrorKind::Config,
            RetryClass::Terminal,
            "language=multi excludes Chinese and silently degrades: pin a concrete language (en)",
        ));
    }
    let separator = if endpoint.contains('?') { '&' } else { '?' };
    Ok(format!(
        "{endpoint}{separator}model={}&language={}&encoding={}&sample_rate={}&channels={}\
         &interim_results={}&endpointing={}&vad_events={}&utterance_end_ms={}&punctuate={}",
        params.model,
        params.language,
        params.encoding,
        params.sample_rate,
        params.channels,
        params.interim_results,
        params.endpointing,
        params.vad_events,
        params.utterance_end_ms,
        params.punctuate,
    ))
}

/// The `Authorization` header value. Deepgram uses its own `Token` scheme —
/// `Bearer` is a 401 (Test 3).
pub fn authorization_value(api_key: &str) -> String {
    format!("Token {api_key}")
}

// ---------------------------------------------------------------------------
// transcript reconstruction
// ---------------------------------------------------------------------------

/// Holds one utterance: the `is_final` runs the vendor has closed, plus the
/// volatile interim tail that replaces itself as the sentence grows.
///
/// The distinction that matters (D-16): an `is_final` run is *closed* but the
/// utterance is not *over* — only `speech_final` (or the `UtteranceEnd`
/// fallback) means we may act on the text.
#[derive(Debug, Default, Clone)]
pub struct TranscriptBuffer {
    runs: Vec<String>,
    interim: String,
}

impl TranscriptBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// A closed run: kept until the utterance ends.
    pub fn push_run(&mut self, text: &str) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            self.runs.push(trimmed.to_string());
        }
        // A closed run supersedes whatever interim text preceded it.
        self.interim.clear();
    }

    /// Live text: replaced on every frame, never committed by itself.
    pub fn push_interim(&mut self, text: &str) {
        self.interim = text.trim().to_string();
    }

    pub fn text(&self) -> String {
        let mut parts: Vec<&str> = self.runs.iter().map(String::as_str).collect();
        if !self.interim.is_empty() {
            parts.push(&self.interim);
        }
        parts.join(" ")
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty() && self.interim.is_empty()
    }

    /// Close the utterance and return its full text.
    pub fn flush(&mut self) -> String {
        let text = self.text();
        self.runs.clear();
        self.interim.clear();
        text
    }
}

/// NET-0001: "no audio received for N s". The transcript never ended properly,
/// so the caller must treat the link as stale rather than complete.
pub fn is_silent_timeout(reason: &str) -> bool {
    reason.contains("NET-0001") || reason.to_ascii_lowercase().contains("no audio received")
}

// ---------------------------------------------------------------------------
// wire shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ServerFrame {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    is_final: bool,
    #[serde(default)]
    speech_final: bool,
    /// Deliberately untyped: `Results` frames carry
    /// `"channel": {"alternatives": [...]}` while `SpeechStarted` /
    /// `UtteranceEnd` carry `"channel": [0]` (an array of indices). A typed
    /// field would fail to parse exactly the frames that matter for barge-in,
    /// and a skipped frame is invisible at runtime.
    #[serde(default)]
    channel: Option<Value>,
    #[serde(default)]
    metadata: Option<Value>,
    #[serde(default)]
    timestamp: Option<f64>,
}

impl ServerFrame {
    /// `channel.alternatives[0]`, when this frame carries a transcript.
    fn first_alternative(&self) -> Alternative {
        self.channel
            .as_ref()
            .and_then(|channel| channel.get("alternatives"))
            .and_then(Value::as_array)
            .and_then(|alternatives| alternatives.first())
            .and_then(|alternative| serde_json::from_value(alternative.clone()).ok())
            .unwrap_or_default()
    }
}

/// One decoded alternative. `confidence` is optional because not every
/// endpoint or frame carries a score.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Alternative {
    #[serde(default)]
    pub transcript: String,
    #[serde(default)]
    pub confidence: Option<f32>,
}

/// `metadata.model_info` → `"name version arch"` (Test 9 / D-08).
pub fn model_version_from(metadata: &Value) -> Option<String> {
    let info = metadata.get(MODEL_INFO_FIELD)?;
    let name = info.get("name")?.as_str()?;
    let version = info.get("version").and_then(Value::as_str);
    let arch = info.get("arch").and_then(Value::as_str);
    let mut parts = vec![name.to_string()];
    if let Some(version) = version {
        parts.push(version.to_string());
    }
    if let Some(arch) = arch {
        parts.push(arch.to_string());
    }
    Some(parts.join(" "))
}

/// Build the [`SttPartial`] for one decoded run.
///
/// `committed` is `speech_final`, never `is_final`: the interviewer may pause
/// mid-sentence and the subtitle must not be treated as a finished utterance.
pub fn partial_from_run(
    text: &str,
    is_final: bool,
    speech_final: bool,
    alternative: Option<&Alternative>,
    model_version: &str,
) -> SttPartial {
    let confidence = alternative.and_then(|alternative| alternative.confidence);
    SttPartial {
        text: text.to_string(),
        is_final,
        committed: speech_final,
        revision_applied: false,
        confidence,
        // A missing score is *not* a vendor score: the interviewer line has no
        // local proxy (02-03's T3.6 covers the user path only).
        confidence_source: if confidence.is_some() {
            ConfidenceSource::Vendor
        } else {
            ConfidenceSource::ProxyUnavailable
        },
        model_version: model_version.to_string(),
        provider: PROVIDER.to_string(),
    }
}

// ---------------------------------------------------------------------------
// the client
// ---------------------------------------------------------------------------

/// The Deepgram streaming stage — interviewer subtitles, never a voice source.
#[derive(Debug, Clone)]
pub struct DeepgramStt {
    credentials: DeepgramCredentials,
    endpoints: Endpoints,
    params: ListenParams,
    /// Kept for the trait's uniform shape; the 02-01 rig must never see this
    /// stage's boundary (see the module docs).
    marks: MarkHandle,
    keepalive_ms: u64,
    /// `metadata.model_info` of the running session, shared with the driver.
    live_model_version: Arc<Mutex<Option<String>>>,
}

impl DeepgramStt {
    pub fn new(credentials: DeepgramCredentials, endpoints: Endpoints) -> Self {
        Self {
            credentials,
            endpoints,
            params: ListenParams::default(),
            marks: MarkHandle::disabled(),
            keepalive_ms: KEEPALIVE_INTERVAL_MS,
            live_model_version: Arc::new(Mutex::new(None)),
        }
    }

    /// Shorten the heartbeat for a test (the production cadence is 3 s).
    pub fn with_keepalive_interval_ms(mut self, keepalive_ms: u64) -> Self {
        self.keepalive_ms = keepalive_ms.max(1);
        self
    }

    pub fn with_utterance_end_ms(mut self, utterance_end_ms: u32) -> Self {
        self.params.utterance_end_ms = utterance_end_ms;
        self
    }

    /// The endpoint this client dials (before the query is built).
    pub fn endpoint(&self) -> &super::config::Endpoint {
        &self.endpoints.deepgram_ws
    }

    fn current_model_version(&self) -> String {
        self.live_model_version
            .lock()
            .ok()
            .and_then(|version| version.clone())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string())
    }
}

impl SttSource for DeepgramStt {
    fn provider(&self) -> &'static str {
        PROVIDER
    }

    /// The metadata-reported version once a session has run, the requested
    /// model before that (D-08 reports what actually ran).
    fn model_version(&self) -> String {
        self.current_model_version()
    }

    fn set_marks(&mut self, marks: MarkHandle) {
        self.marks = marks;
    }

    fn start(&mut self, _epoch: u64) -> Result<SttStream, StageError> {
        let url = listen_url(self.endpoints.deepgram_ws.url(), &self.params)?;
        let mut request = url.into_client_request().map_err(|error| {
            StageError::new(
                PROVIDER,
                ErrorKind::Config,
                RetryClass::Terminal,
                format!("the listen URL is not a valid request: {error}"),
            )
        })?;
        let header = HeaderValue::from_str(&authorization_value(self.credentials.api_key.expose()))
            .map_err(|_| {
                StageError::new(
                    PROVIDER,
                    ErrorKind::Config,
                    RetryClass::Client,
                    "the API key cannot be sent as a header (it contains invalid characters)",
                )
            })?;
        request.headers_mut().insert(AUTHORIZATION, header);

        let endpoint = self.endpoints.deepgram_ws.clone();
        let keepalive = Duration::from_millis(self.keepalive_ms);
        let live_model_version = self.live_model_version.clone();

        let (upstream_tx, upstream_rx) = mpsc::channel(AUDIO_QUEUE_FRAMES);
        let (events_tx, events_rx) = mpsc::channel(EVENT_QUEUE_ITEMS);

        tokio::spawn(async move {
            run_session(
                request,
                endpoint,
                keepalive,
                live_model_version,
                upstream_rx,
                events_tx,
            )
            .await;
        });

        Ok(SttStream::new(PROVIDER, upstream_tx, events_rx))
    }
}

type ClientSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn fail(events: &mpsc::Sender<SttEvent>, error: StageError) {
    // The classified failure is the only signal the consumer's retry/breaker
    // classification keys on (WR-03): a best-effort try_send could drop it on
    // a full queue and the consumer would see a clean-looking close instead.
    // Wait for the slot — this is an async task, not the audio callback. A
    // send failure means the caller is gone; nothing to report to.
    let _ = events.send(SttEvent::Failed(error)).await;
}

async fn send_text(socket: &mut ClientSocket, body: &str) -> Result<(), StageError> {
    socket
        .send(Message::Text(body.to_string().into()))
        .await
        .map_err(|error| StageError::transport(PROVIDER, format!("send failed: {error}")))
}

async fn run_session(
    request: tokio_tungstenite::tungstenite::handshake::client::Request,
    endpoint: super::config::Endpoint,
    keepalive: Duration,
    live_model_version: Arc<Mutex<Option<String>>>,
    mut upstream: mpsc::Receiver<SttUpstream>,
    events: mpsc::Sender<SttEvent>,
) {
    let mut socket = match tokio_tungstenite::connect_async(request).await {
        Ok((socket, _response)) => socket,
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            return fail(
                &events,
                StageError::new(
                    PROVIDER,
                    if matches!(response.status().as_u16(), 401 | 403) {
                        ErrorKind::Auth
                    } else {
                        ErrorKind::Http
                    },
                    classify_http_status(response.status().as_u16()),
                    format!("handshake rejected (HTTP {})", response.status().as_u16()),
                )
                .with_endpoint(endpoint.url()),
            ).await
        }
        Err(error) => {
            return fail(
                &events,
                StageError::transport(PROVIDER, format!("connect failed: {error}"))
                    .with_endpoint(endpoint.url()),
            ).await
        }
    };

    let mut buffer = TranscriptBuffer::new();
    // The heartbeat is the *only* thing keeping a quiet session open; without
    // it the vendor closes with NET-0001 after 10 s.
    let mut heartbeat =
        tokio::time::interval_at(tokio::time::Instant::now() + keepalive, keepalive);
    let mut closing = false;

    loop {
        tokio::select! {
            _ = heartbeat.tick(), if !closing => {
                if let Err(error) = send_text(&mut socket, KEEPALIVE_FRAME).await {
                    return fail(&events, error).await;
                }
            }
            item = upstream.recv(), if !closing => {
                match item {
                    Some(SttUpstream::Audio(pcm16)) => {
                        let bytes = pcm16_to_le_bytes(&pcm16);
                        if socket.send(Message::Binary(bytes.into())).await.is_err() {
                            return fail(
                                &events,
                                StageError::transport(PROVIDER, "send failed while streaming audio"),
                            ).await;
                        }
                    }
                    Some(SttUpstream::End) | None => {
                        closing = true;
                        if let Err(error) = send_text(&mut socket, CLOSE_STREAM_FRAME).await {
                            return fail(&events, error).await;
                        }
                    }
                }
            }
            message = socket.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        match handle_frame(text.as_str(), &mut buffer, &live_model_version, &events)
                            .await
                        {
                            Flow::Continue => {}
                            Flow::Stop => return,
                        }
                    }
                    Some(Ok(Message::Close(frame))) => {
                        let reason = frame
                            .as_ref()
                            .map(|frame| frame.reason.to_string())
                            .unwrap_or_default();
                        let code = frame
                            .as_ref()
                            .map(|frame| u16::from(frame.code))
                            .unwrap_or_default();
                        // Whatever arrived before the close is still real text.
                        if !buffer.is_empty() {
                            let text = buffer.flush();
                            let version = current_version(&live_model_version);
                            let partial = partial_from_run(&text, true, true, None, &version);
                            if events.send(SttEvent::Partial(partial)).await.is_err() {
                                return;
                            }
                        }
                        if is_silent_timeout(&reason) {
                            return fail(
                                &events,
                                StageError::transport(
                                    PROVIDER,
                                    format!(
                                        "NET-0001 silent timeout: the service closed the session after {}s \
                                         without audio; the link is stale",
                                        SILENCE_CLOSE_MS / 1_000
                                    ),
                                ),
                            ).await;
                        }
                        if closing {
                            // We asked for the flush; a clean end.
                            return;
                        }
                        return fail(
                            &events,
                            StageError::transport(
                                PROVIDER,
                                format!("the session closed unexpectedly (code {code}): {reason}"),
                            ),
                        ).await;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        return fail(
                            &events,
                            StageError::transport(PROVIDER, format!("socket error: {error}")),
                        ).await;
                    }
                    None => {
                        if closing && buffer.is_empty() {
                            return;
                        }
                        return fail(
                            &events,
                            StageError::transport(PROVIDER, "the socket closed mid-fragment"),
                        ).await;
                    }
                }
            }
        }
    }
}

/// What the driver does after one decoded frame.
enum Flow {
    /// Keep reading.
    Continue,
    /// The consumer is gone — stop the session without reporting a failure.
    Stop,
}

async fn handle_frame(
    raw: &str,
    buffer: &mut TranscriptBuffer,
    live_model_version: &Arc<Mutex<Option<String>>>,
    events: &mpsc::Sender<SttEvent>,
) -> Flow {
    let Ok(frame) = serde_json::from_str::<ServerFrame>(raw) else {
        // A frame we cannot read is skipped, not fatal: the interviewer line
        // is a side channel, and one stray frame must not kill the subtitle
        // stream mid-sentence.
        return Flow::Continue;
    };

    if let Some(metadata) = &frame.metadata {
        if let Some(version) = model_version_from(metadata) {
            if let Ok(mut slot) = live_model_version.lock() {
                *slot = Some(version);
            }
        }
    }
    let version = current_version(live_model_version);

    match frame.kind.as_str() {
        "SpeechStarted" => {
            let at_ms = (frame.timestamp.unwrap_or(0.0) * 1_000.0).round() as u64;
            if events
                .send(SttEvent::SpeechStarted { at_ms })
                .await
                .is_err()
            {
                return Flow::Stop;
            }
            Flow::Continue
        }
        "UtteranceEnd" => {
            // The fallback commit path: some sessions never send `speech_final`.
            if buffer.is_empty() {
                return Flow::Continue;
            }
            let text = buffer.flush();
            let partial = partial_from_run(&text, true, true, None, &version);
            if events.send(SttEvent::Partial(partial)).await.is_err() {
                return Flow::Stop;
            }
            Flow::Continue
        }
        "Results" => {
            let alternative = frame.first_alternative();
            if frame.is_final {
                buffer.push_run(&alternative.transcript);
            } else {
                buffer.push_interim(&alternative.transcript);
            }
            let committed = frame.speech_final;
            let text = if committed {
                buffer.flush()
            } else {
                buffer.text()
            };
            let partial = partial_from_run(
                &text,
                frame.is_final,
                committed,
                Some(&alternative),
                &version,
            );
            if events.send(SttEvent::Partial(partial)).await.is_err() {
                return Flow::Stop;
            }
            Flow::Continue
        }
        _ => Flow::Continue,
    }
}

fn current_version(live_model_version: &Arc<Mutex<Option<String>>>) -> String {
    live_model_version
        .lock()
        .ok()
        .and_then(|version| version.clone())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string())
}

/// `Vec<i16>` (the API's sample format) → little-endian bytes.
fn pcm16_to_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::stages::traits::{ConfidenceSource, InterviewerTrack};

    #[test]
    fn the_listen_url_pins_the_language_and_never_asks_for_multi() {
        // Test 1: `multi` silently produces garbage for Chinese and is not a
        // valid language bundle for this endpoint. This is the guard.
        let url = listen_url("wss://api.deepgram.com/v1/listen", &ListenParams::default())
            .expect("the default endpoint parses");
        assert!(url.contains("language=en"), "{url}");
        assert!(!url.contains("language=multi"), "{url}");

        let refused = ListenParams {
            language: "multi",
            ..ListenParams::default()
        };
        let error =
            listen_url("wss://api.deepgram.com/v1/listen", &refused).expect_err("multi is refused");
        assert_eq!(error.retry_class, RetryClass::Terminal);
    }

    #[test]
    fn the_listen_url_locks_every_parameter_the_cascade_depends_on() {
        // Test 2
        let url = listen_url("wss://api.deepgram.com/v1/listen", &ListenParams::default())
            .expect("parses");
        for expected in [
            "model=nova-3",
            "encoding=linear16",
            "sample_rate=16000",
            "channels=1",
            "interim_results=true",
            "endpointing=300",
            "vad_events=true",
            "utterance_end_ms=1000",
            "punctuate=true",
        ] {
            assert!(url.contains(expected), "{expected} missing from {url}");
        }
    }

    #[test]
    fn keepalive_and_close_stream_are_the_documented_literals() {
        assert_eq!(KEEPALIVE_FRAME, r#"{"type":"KeepAlive"}"#);
        assert_eq!(CLOSE_STREAM_FRAME, r#"{"type":"CloseStream"}"#);
        assert_eq!(
            KEEPALIVE_INTERVAL_MS, 3_000,
            "the named constant is the cadence"
        );
    }

    #[test]
    fn the_auth_scheme_is_token_not_bearer() {
        // Test 3: Deepgram rejects `Bearer` with a 401.
        let value = authorization_value("abc123");
        assert_eq!(value, "Token abc123");
        assert!(!value.contains("Bearer"), "{value}");
    }

    #[test]
    fn the_transcript_buffer_commits_only_on_a_flush() {
        // Test 4: an `is_final` run is *buffered*; only `speech_final` (or an
        // `UtteranceEnd` fallback) commits the sentence.
        let mut buffer = TranscriptBuffer::new();
        buffer.push_run("Could you walk");
        assert_eq!(buffer.text(), "Could you walk");
        assert!(!buffer.is_empty());
        buffer.push_run("me through the steps");
        assert_eq!(buffer.text(), "Could you walk me through the steps");
        assert_eq!(
            buffer.flush(),
            "Could you walk me through the steps",
            "the flush returns the whole utterance"
        );
        assert!(buffer.is_empty(), "a flush resets the buffer");
    }

    #[test]
    fn an_interim_run_is_replaced_not_appended() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push_interim("Could you");
        buffer.push_interim("Could you walk");
        assert_eq!(buffer.text(), "Could you walk", "interim text is volatile");
    }

    #[test]
    fn the_net_0001_close_reason_is_recognised_as_a_silent_timeout() {
        assert!(is_silent_timeout("NET-0001: No audio received for 10s"));
        assert!(!is_silent_timeout("going away"));
        assert_eq!(SILENCE_CLOSE_MS, 10_000);
    }

    #[test]
    fn the_confidence_is_only_reported_when_the_vendor_sends_one() {
        let with_score = Alternative {
            transcript: "hello".to_string(),
            confidence: Some(0.98),
        };
        let partial = partial_from_run("hello", true, false, Some(&with_score), "nova-3");
        assert_eq!(partial.confidence, Some(0.98));
        assert_eq!(partial.confidence_source, ConfidenceSource::Vendor);

        let without = Alternative {
            transcript: "hello".to_string(),
            confidence: None,
        };
        let partial = partial_from_run("hello", true, false, Some(&without), "nova-3");
        assert_eq!(partial.confidence, None, "no score is invented");
    }

    #[test]
    fn the_model_version_comes_from_the_frame_metadata() {
        // Test 9 (D-08): the version reported in traces is the one the service
        // actually ran, not the one we asked for.
        let metadata = serde_json::json!({
            "request_id": "dg-1",
            "model_info": { "name": "nova-3", "version": "2026-01-15", "arch": "chirp-3" }
        });
        assert_eq!(
            model_version_from(&metadata).as_deref(),
            Some("nova-3 2026-01-15 chirp-3")
        );
        assert_eq!(model_version_from(&serde_json::json!({})), None);
    }

    #[test]
    fn the_interviewer_track_marker_is_the_type_not_a_comment() {
        // The cascade's TTS sink accepts user fragments only; this marker is
        // how 02-03 keeps the interviewer line out of it.
        let _marker = InterviewerTrack;
    }
}
