//! Deterministic mocks of the four cloud vendor endpoints (T2.6).
//!
//! Every vendor behaviour this phase depends on is exercised offline: the suite
//! binds `127.0.0.1:0` (kernel-assigned port) and never touches the network.
//! Credentials are *shaped* like real ones but are never read from the
//! environment, so `cargo test` passes on a machine with no keys — the
//! real-key smoke paths are `#[ignore]`d next to the client that owns them.
//!
//! | mock | endpoint | replay / observations |
//! |------|----------|-----------------------|
//! | [`xfyun_mock`] | `/v2/iat` (WSS) | `status:0/1/2` frames incl. `pgs` corrections, error-code frames, 60 s session cap, clock-skew handshake rejection |
//! | [`deepgram_mock`] | `/v1/listen` (WSS) | `Results` / `UtteranceEnd` / `SpeechStarted` / `Metadata`, KeepAlive receipt, NET-0001 silent close |
//! | [`deepseek_mock`] | `/chat/completions` (SSE) | `data:` events as separate TCP chunks with the `\n\n` boundary deliberately split (failure case 0001), plus request capture |
//! | [`volc_mock`] | `/api/v3/tts/unidirectional/stream` (WSS) | binary frames (352 audio / 152 finished / `0b1111` error), handshake-header capture |
//!
//! The failure-injection tests live here too: `429 → Retryable`, a malformed
//! frame must not panic a parser, and a silent session must end with a close
//! reason the client can recognise (research correction 4: 10 s of silence is a
//! NET-0001 close).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;

use nextalk_desktop_lib::pipeline::stages::error::{classify_http_status, RetryClass, StageError};

/// How long a test waits for a mock before declaring the session stuck.
const MOCK_TIMEOUT: Duration = Duration::from_secs(10);

type ClientSocket = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

// ---------------------------------------------------------------------------
// server plumbing
// ---------------------------------------------------------------------------

/// A running mock on an ephemeral loopback port. Dropping it aborts the task.
struct Server {
    addr: SocketAddr,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl Server {
    async fn serve<St>(router: Router<Arc<St>>, state: Arc<St>) -> Self
    where
        St: Send + Sync + 'static,
    {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("mock binds an ephemeral loopback port");
        let addr = listener.local_addr().expect("mock has an address");
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, router.with_state(state)).await;
        });
        Self { addr, handle }
    }

    /// `ws://127.0.0.1:<port><path>` — the endpoint override clients take.
    fn ws_url(&self, path: &str) -> String {
        format!("ws://{}{path}", self.addr)
    }

    /// `http://127.0.0.1:<port><path>` for the SSE mock.
    fn http_url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }
}

/// One scripted frame plus the delay before it is written.
#[derive(Clone)]
struct TimedFrame {
    after_ms: u64,
    body: String,
}

impl TimedFrame {
    fn now(body: impl Into<String>) -> Self {
        Self {
            after_ms: 0,
            body: body.into(),
        }
    }

    fn after(ms: u64, body: impl Into<String>) -> Self {
        Self {
            after_ms: ms,
            body: body.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// 讯飞 iat
// ---------------------------------------------------------------------------

/// 讯飞 iat script. `Default` is the happy path: one partial right after the
/// upgrade, then — once the client sends audio — an `apd` correction and the
/// `status:2` final.
#[derive(Clone)]
struct XfyunMock {
    greeting: Vec<TimedFrame>,
    on_audio: Vec<TimedFrame>,
    /// A signed `date` further than this from now is rejected with HTTP 403.
    clock_skew_window_secs: u64,
    /// Hang up after this long (the real session cap is 60 s).
    session_cap_ms: Option<u64>,
    /// Accept audio but never answer it.
    ignore_audio: bool,
}

impl Default for XfyunMock {
    fn default() -> Self {
        Self {
            greeting: vec![TimedFrame::now(xfyun_partial_frame(1, 0, "你能"))],
            on_audio: vec![
                TimedFrame::now(xfyun_wpgs_frame(2, "apd", (2, 2), "详细")),
                TimedFrame::now(xfyun_final_frame(3, "说一下优化步骤吗？")),
            ],
            clock_skew_window_secs: 300,
            session_cap_ms: None,
            ignore_audio: false,
        }
    }
}

impl XfyunMock {
    fn with_greeting(mut self, frames: Vec<TimedFrame>) -> Self {
        self.greeting = frames;
        self
    }

    fn with_on_audio(mut self, frames: Vec<TimedFrame>) -> Self {
        self.on_audio = frames;
        self
    }

    fn with_session_cap_ms(mut self, ms: u64) -> Self {
        self.session_cap_ms = Some(ms);
        self
    }

    fn ignoring_audio(mut self) -> Self {
        self.ignore_audio = true;
        self
    }
}

struct XfyunState {
    cfg: XfyunMock,
    sessions: AtomicU32,
    audio_frames: AtomicU32,
    /// Every JSON frame the client sent, in order.
    received: Mutex<Vec<Value>>,
}

impl XfyunState {
    fn new(cfg: XfyunMock) -> Self {
        Self {
            cfg,
            sessions: AtomicU32::new(0),
            audio_frames: AtomicU32::new(0),
            received: Mutex::new(Vec::new()),
        }
    }
}

fn xfyun_partial_frame(sn: u32, status: u32, words: &str) -> String {
    json!({
        "code": 0,
        "sid": "iat0001",
        "data": {
            "sn": sn,
            "status": status,
            "result": { "ws": [{ "bg": 0, "cw": [{ "w": words, "sc": 0 }] }] }
        }
    })
    .to_string()
}

/// A `pgs` dynamic-correction frame: `apd` appends, `rpl` replaces `rg`.
fn xfyun_wpgs_frame(sn: u32, pgs: &str, rg: (u32, u32), words: &str) -> String {
    json!({
        "code": 0,
        "sid": "iat0001",
        "data": {
            "sn": sn,
            "pgs": pgs,
            "rg": [rg.0, rg.1],
            "status": 1,
            "result": { "ws": [{ "bg": 0, "cw": [{ "w": words, "sc": 0 }] }] }
        }
    })
    .to_string()
}

fn xfyun_final_frame(sn: u32, tail: &str) -> String {
    json!({
        "code": 0,
        "sid": "iat0001",
        "data": {
            "sn": sn,
            "status": 2,
            "result": { "ws": [{ "bg": 0, "cw": [{ "w": tail, "sc": 0 }] }] }
        }
    })
    .to_string()
}

fn xfyun_error_frame(code: i32, message: &str) -> String {
    json!({ "code": code, "message": message, "sid": "iat0001" }).to_string()
}

async fn xfyun_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<XfyunState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    // The handshake is the auth boundary: a signed URL carries `authorization`,
    // `date`, `host` and `appid`, and the date must be fresh (this is the
    // clock-skew path a stale local clock hits in production).
    let (Some(date), Some(authorization)) = (query.get("date"), query.get("authorization")) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !authorization.contains("signature=") || !query.contains_key("host") {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if signed_date_is_stale(date, state.cfg.clock_skew_window_secs) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let sessions = state.sessions.fetch_add(1, Ordering::SeqCst) + 1;
    let session_state = state.clone();
    ws.on_upgrade(move |socket| xfyun_session(socket, session_state, sessions))
}

async fn xfyun_session(mut socket: WebSocket, state: Arc<XfyunState>, session: u32) {
    for frame in &state.cfg.greeting {
        if frame.after_ms > 0 {
            tokio::time::sleep(Duration::from_millis(frame.after_ms)).await;
        }
        if socket.send(Message::Text(frame.body.clone().into())).await.is_err() {
            return;
        }
    }

    let deadline = state
        .cfg
        .session_cap_ms
        .map(|ms| tokio::time::Instant::now() + Duration::from_millis(ms));
    let mut answered = false;
    loop {
        let next = match deadline {
            Some(deadline) => match tokio::time::timeout_at(deadline, socket.next()).await {
                Ok(frame) => frame,
                Err(_) => {
                    // The real service hangs up at the 60 s cap without a final
                    // frame; the client is expected to rotate before that.
                    let _ = socket.send(Message::Close(None)).await;
                    return;
                }
            },
            None => socket.next().await,
        };
        let Some(Ok(message)) = next else { return };
        match message {
            Message::Text(text) => {
                let parsed: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                state.received.lock().unwrap().push(parsed);
                state.audio_frames.fetch_add(1, Ordering::SeqCst);
                if !state.cfg.ignore_audio && !answered {
                    answered = true;
                    for frame in &state.cfg.on_audio {
                        if frame.after_ms > 0 {
                            tokio::time::sleep(Duration::from_millis(frame.after_ms)).await;
                        }
                        if socket.send(Message::Text(frame.body.clone().into())).await.is_err() {
                            return;
                        }
                    }
                    let _ = session;
                }
            }
            Message::Close(_) => return,
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Deepgram Nova-3
// ---------------------------------------------------------------------------

/// Deepgram mock. `Default` = a `SpeechStarted`, one buffered `is_final` run,
/// then the `speech_final` flush.
#[derive(Clone)]
struct DeepgramMock {
    greeting: Vec<TimedFrame>,
    /// Hang up when no `KeepAlive` text frame arrives within this window.
    require_keepalive_within_ms: Option<u64>,
    /// Send an unparseable frame first (parser robustness).
    malformed: bool,
}

impl Default for DeepgramMock {
    fn default() -> Self {
        Self {
            greeting: vec![
                TimedFrame::now(deepgram_speech_started()),
                TimedFrame::now(deepgram_results("Could you walk", true, false, Some(0.98))),
                TimedFrame::after(
                    30,
                    deepgram_results("me through the steps", true, true, Some(0.94)),
                ),
            ],
            require_keepalive_within_ms: None,
            malformed: false,
        }
    }
}

impl DeepgramMock {
    fn with_malformed_frame(mut self) -> Self {
        self.malformed = true;
        self
    }

    fn requiring_keepalive(mut self, ms: u64) -> Self {
        self.require_keepalive_within_ms = Some(ms);
        self
    }

    /// A run that never sends `speech_final`: only `UtteranceEnd` closes it.
    fn with_utterance_end_only(mut self) -> Self {
        self.greeting = vec![
            TimedFrame::now(deepgram_results("just an is_final", true, false, None)),
            TimedFrame::after(50, deepgram_utterance_end()),
        ];
        self
    }
}

struct DeepgramState {
    cfg: DeepgramMock,
    sessions: AtomicU32,
    /// Every text frame the client sent (the KeepAlive cadence is asserted here).
    texts: Mutex<Vec<String>>,
    audio_frames: AtomicU32,
}

impl DeepgramState {
    fn new(cfg: DeepgramMock) -> Self {
        Self {
            cfg,
            sessions: AtomicU32::new(0),
            texts: Mutex::new(Vec::new()),
            audio_frames: AtomicU32::new(0),
        }
    }
}

fn deepgram_speech_started() -> String {
    json!({ "type": "SpeechStarted", "channel": [0], "timestamp": 0.42 }).to_string()
}

fn deepgram_results(
    transcript: &str,
    is_final: bool,
    speech_final: bool,
    confidence: Option<f32>,
) -> String {
    let mut alternative = json!({ "transcript": transcript, "words": [] });
    if let Some(confidence) = confidence {
        alternative["confidence"] = json!(confidence);
    }
    json!({
        "type": "Results",
        "channel_index": [0, 1],
        "duration": 0.8,
        "start": 0.0,
        "is_final": is_final,
        "speech_final": speech_final,
        "channel": { "alternatives": [alternative] },
        "metadata": { "request_id": "dg-mock-1", "model_info": deepgram_model_info() },
    })
    .to_string()
}

fn deepgram_utterance_end() -> String {
    json!({ "type": "UtteranceEnd", "channel": [0], "last_word_end": 1.62 }).to_string()
}

fn deepgram_model_info() -> Value {
    json!({ "name": "nova-3", "version": "2026-01-15", "arch": "chirp-3" })
}

async fn deepgram_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<DeepgramState>>,
    headers: HeaderMap,
) -> Response {
    // Deepgram authenticates with `Token <key>`, never `Bearer` (T2.3 Test 3).
    let authorized = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("Token ") && value.len() > "Token ".len());
    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    state.sessions.fetch_add(1, Ordering::SeqCst);
    let session_state = state.clone();
    ws.on_upgrade(move |socket| deepgram_session(socket, session_state))
}

async fn deepgram_session(mut socket: WebSocket, state: Arc<DeepgramState>) {
    if state.cfg.malformed {
        let _ = socket.send(Message::Text("{not json".into())).await;
        let _ = socket
            .send(Message::Text(deepgram_results("garbage", true, true, None).into()))
            .await;
        return;
    }
    for frame in &state.cfg.greeting {
        if frame.after_ms > 0 {
            tokio::time::sleep(Duration::from_millis(frame.after_ms)).await;
        }
        if socket.send(Message::Text(frame.body.clone().into())).await.is_err() {
            return;
        }
    }

    let deadline = state
        .cfg
        .require_keepalive_within_ms
        .map(|ms| tokio::time::Instant::now() + Duration::from_millis(ms));
    loop {
        let next = match deadline {
            Some(deadline) => match tokio::time::timeout_at(deadline, socket.next()).await {
                Ok(frame) => frame,
                Err(_) => {
                    // NET-0001: a session that sends neither audio nor KeepAlive
                    // is closed after 10 s. The reason must be readable so the
                    // client can classify it as a retryable link failure.
                    let _ = socket
                        .send(Message::Close(Some(CloseFrame {
                            code: 1000,
                            reason: "NET-0001: No audio received for 10s".into(),
                        })))
                        .await;
                    return;
                }
            },
            None => socket.next().await,
        };
        let Some(Ok(message)) = next else { return };
        match message {
            Message::Text(text) => state.texts.lock().unwrap().push(text.to_string()),
            Message::Binary(_) => {
                state.audio_frames.fetch_add(1, Ordering::SeqCst);
            }
            Message::Close(_) => return,
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// DeepSeek (SSE)
// ---------------------------------------------------------------------------

/// DeepSeek mock: `chunks` are written as separate body chunks, each preceded
/// by `chunk_delay_ms` so the runtime cannot coalesce them. The default script
/// splits a `\n\n` boundary across two chunks — failure case 0001, on purpose:
/// a parser that reads per TCP chunk drops the token that straddles it.
#[derive(Clone)]
struct DeepseekMock {
    chunks: Vec<String>,
    chunk_delay_ms: u64,
    status: u16,
}

impl Default for DeepseekMock {
    fn default() -> Self {
        Self {
            chunks: vec![
                sse_content("Could you walk "),
                // …half an event boundary…
                "data: {\"choices\":[{\"delta\":{\"content\":\"me through the steps. The spike \"}}]}"
                    .to_string(),
                // …completed by the next TCP chunk.
                "\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"was 800 ms.\"}}]}\n\n".to_string(),
                sse_usage(42, 19),
                "data: [DONE]\n\n".to_string(),
            ],
            chunk_delay_ms: 20,
            status: 200,
        }
    }
}

impl DeepseekMock {
    fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    /// The event JSON never closes: a lenient parser that fell back to an empty
    /// string would silently lose the fragment.
    fn with_malformed_event(mut self) -> Self {
        self.chunks = vec![
            "data: {\"choices\":[{\"delta\":{\"content\":\"half\"]}\n\n".to_string(),
            sse_usage(9, 9),
            "data: [DONE]\n\n".to_string(),
        ];
        self
    }

    /// The body ends mid-event (no trailing boundary).
    fn with_truncated_event(mut self) -> Self {
        self.chunks = vec![sse_content("partial answer ").trim_end().to_string()];
        self
    }

    fn with_structured_fragment(mut self) -> Self {
        self.chunks = vec![
            sse_content("{\"t\":\"fragment\",\"text\":\"The query was 800 ms.\",\"final_flag\":true}"),
            sse_usage(64, 12),
            "data: [DONE]\n\n".to_string(),
        ];
        self
    }

    fn with_abstain(mut self) -> Self {
        self.chunks = vec![
            sse_content("{\"t\":\"abstained\",\"reason\":\"silent_audio\"}"),
            sse_usage(12, 6),
            "data: [DONE]\n\n".to_string(),
        ];
        self
    }
}

fn sse_content(text: &str) -> String {
    format!(
        "data: {}\n\n",
        json!({
            "id": "chatcmpl-mock",
            "choices": [ { "index": 0, "delta": { "content": text } } ]
        })
    )
}

fn sse_usage(prompt: u32, completion: u32) -> String {
    format!(
        "data: {}\n\n",
        json!({
            "id": "chatcmpl-mock",
            "choices": [],
            "usage": {
                "prompt_tokens": prompt,
                "completion_tokens": completion,
                "total_tokens": prompt + completion
            }
        })
    )
}

struct DeepseekState {
    cfg: DeepseekMock,
    /// Request bodies the client sent, in order (asserted by T2.4).
    bodies: Mutex<Vec<Value>>,
    authorizations: Mutex<Vec<String>>,
}

impl DeepseekState {
    fn new(cfg: DeepseekMock) -> Self {
        Self {
            cfg,
            bodies: Mutex::new(Vec::new()),
            authorizations: Mutex::new(Vec::new()),
        }
    }
}

async fn deepseek_chat(
    State(state): State<Arc<DeepseekState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    state.bodies.lock().unwrap().push(body);
    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        state.authorizations.lock().unwrap().push(auth.to_string());
    }
    if state.cfg.status != 200 {
        return (
            StatusCode::from_u16(state.cfg.status).unwrap_or(StatusCode::BAD_REQUEST),
            "injected failure",
        )
            .into_response();
    }
    let delay = Duration::from_millis(state.cfg.chunk_delay_ms);
    let chunks = state.cfg.chunks.clone();
    let stream = futures_util::stream::iter(chunks).then(move |chunk| {
        let delay = delay;
        async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(chunk))
        }
    });
    (
        [
            (axum::http::header::CONTENT_TYPE, "text/event-stream"),
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// 火山 Seed-TTS 2.0 / ICL 2.0
// ---------------------------------------------------------------------------

/// 火山 mock. `Default` = two audio frames then a clean `SESSION_FINISHED`
/// (`status_code == 20000000`) carrying usage.
#[derive(Clone)]
struct VolcMock {
    replies: Vec<Vec<u8>>,
    error_frame: Option<(u32, String)>,
    /// `status_code` on the finished frame (20000000 is success).
    finish_status: u32,
}

impl Default for VolcMock {
    fn default() -> Self {
        Self {
            replies: vec![volc_audio_frame(&[0u8; 480]), volc_audio_frame(&[1u8; 480])],
            error_frame: None,
            finish_status: 20_000_000,
        }
    }
}

impl VolcMock {
    fn with_error(mut self, code: u32, message: &str) -> Self {
        self.error_frame = Some((code, message.to_string()));
        self
    }

    fn with_finish_status(mut self, status: u32) -> Self {
        self.finish_status = status;
        self
    }
}

struct VolcState {
    cfg: VolcMock,
    /// Handshake headers the client sent (X-Api-Key / Resource-Id / Request-Id).
    headers: Mutex<Vec<HashMap<String, String>>>,
    /// Decoded request frames.
    requests: Mutex<Vec<Value>>,
}

impl VolcState {
    fn new(cfg: VolcMock) -> Self {
        Self {
            cfg,
            headers: Mutex::new(Vec::new()),
            requests: Mutex::new(Vec::new()),
        }
    }
}

/// `[0x11, 0x10, 0x10, 0x00] + u32be(len) + JSON` — the client's request frame.
fn volc_request_frame(text: &str, speaker: &str) -> Vec<u8> {
    let payload = json!({
        "user": { "uid": "nexTalk-test" },
        "req_params": {
            "text": text,
            "speaker": speaker,
            "audio_params": { "format": "pcm", "sample_rate": 24000 }
        }
    })
    .to_string();
    let mut frame = vec![0x11, 0x10, 0x10, 0x00];
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload.as_bytes());
    frame
}

/// A server frame: `[0x11, <msgType><<4, 0x10, 0x00] + u32be(event) +
/// u32be(sid_len) + sid + u32be(pay_len) + payload` — the layout the
/// experiment script's `parse()` walks.
fn volc_frame(msg_type: u8, event: u32, payload: &[u8]) -> Vec<u8> {
    let sid = b"sid-mock";
    let mut frame = vec![0x11, msg_type << 4, 0x10, 0x00];
    frame.extend_from_slice(&event.to_be_bytes());
    frame.extend_from_slice(&(sid.len() as u32).to_be_bytes());
    frame.extend_from_slice(sid);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

fn volc_audio_frame(pcm: &[u8]) -> Vec<u8> {
    volc_frame(0b1011, 352, pcm)
}

fn volc_json_frame(event: u32, body: Value) -> Vec<u8> {
    volc_frame(0b1001, event, body.to_string().as_bytes())
}

/// `msgType == 0b1111`: `[0x11, 0xF0, 0x10, 0x00] + u32be(code) + u32be(size) + message`.
fn volc_error_frame(code: u32, message: &str) -> Vec<u8> {
    let mut frame = vec![0x11, 0xF0, 0x10, 0x00];
    frame.extend_from_slice(&code.to_be_bytes());
    frame.extend_from_slice(&(message.len() as u32).to_be_bytes());
    frame.extend_from_slice(message.as_bytes());
    frame
}

async fn volc_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<VolcState>>,
    headers: HeaderMap,
) -> Response {
    let mut captured = HashMap::new();
    for name in ["x-api-key", "x-api-resource-id", "x-api-request-id"] {
        if let Some(value) = headers.get(name).and_then(|v| v.to_str().ok()) {
            captured.insert(name.to_string(), value.to_string());
        }
    }
    if !captured.contains_key("x-api-key") {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    state.headers.lock().unwrap().push(captured);
    let session_state = state.clone();
    ws.on_upgrade(move |socket| volc_session(socket, session_state))
}

async fn volc_session(mut socket: WebSocket, state: Arc<VolcState>) {
    let Some(Ok(request)) = socket.next().await else {
        return;
    };
    let Message::Binary(bytes) = request else {
        return;
    };
    if bytes.len() < 8 {
        return;
    }
    let len = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    if let Some(payload) = bytes.get(8..8 + len) {
        if let Ok(value) = serde_json::from_slice::<Value>(payload) {
            state.requests.lock().unwrap().push(value);
        }
    }

    if let Some((code, message)) = &state.cfg.error_frame {
        let _ = socket
            .send(Message::Binary(volc_error_frame(*code, message).into()))
            .await;
        return;
    }
    for reply in &state.cfg.replies {
        if socket.send(Message::Binary(reply.clone().into())).await.is_err() {
            return;
        }
    }
    let finished = volc_json_frame(
        152,
        json!({
            "status_code": state.cfg.finish_status,
            "usage": { "characters": 42, "text_words": 11 }
        }),
    );
    let _ = socket.send(Message::Binary(finished.into())).await;
}

// ---------------------------------------------------------------------------
// mock entry points
// ---------------------------------------------------------------------------

async fn xfyun_mock(cfg: XfyunMock) -> (Server, Arc<XfyunState>) {
    let state = Arc::new(XfyunState::new(cfg));
    let router = Router::new().route("/v2/iat", get(xfyun_ws));
    let server = Server::serve(router, state.clone()).await;
    (server, state)
}

async fn deepgram_mock(cfg: DeepgramMock) -> (Server, Arc<DeepgramState>) {
    let state = Arc::new(DeepgramState::new(cfg));
    let router = Router::new().route("/v1/listen", get(deepgram_ws));
    let server = Server::serve(router, state.clone()).await;
    (server, state)
}

async fn deepseek_mock(cfg: DeepseekMock) -> (Server, Arc<DeepseekState>) {
    let state = Arc::new(DeepseekState::new(cfg));
    let router = Router::new().route("/chat/completions", post(deepseek_chat));
    let server = Server::serve(router, state.clone()).await;
    (server, state)
}

async fn volc_mock(cfg: VolcMock) -> (Server, Arc<VolcState>) {
    let state = Arc::new(VolcState::new(cfg));
    let router = Router::new().route("/api/v3/tts/unidirectional/stream", get(volc_ws));
    let server = Server::serve(router, state.clone()).await;
    (server, state)
}

// ---------------------------------------------------------------------------
// harness tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mock_servers_bind_ephemeral_ports_and_serve() {
    let (xfyun, _) = xfyun_mock(XfyunMock::default()).await;
    let (deepgram, _) = deepgram_mock(DeepgramMock::default()).await;
    let (deepseek, _) = deepseek_mock(DeepseekMock::default()).await;
    let (volc, _) = volc_mock(VolcMock::default()).await;
    for addr in [xfyun.addr, deepgram.addr, deepseek.addr, volc.addr] {
        assert!(addr.ip().is_loopback(), "mock binds loopback only");
        assert_ne!(addr.port(), 0, "kernel assigned a port");
    }
    assert!(xfyun.ws_url("/v2/iat").starts_with("ws://127.0.0.1:"));
    assert!(deepseek.http_url("/chat/completions").starts_with("http://127.0.0.1:"));
}

#[tokio::test]
async fn xfyun_mock_completes_a_signed_round_trip() {
    let (mock, state) = xfyun_mock(XfyunMock::default()).await;
    let mut socket = connect(&mock.ws_url("/v2/iat"), &signed_query(mock.addr, &now_rfc1123()))
        .await;

    let greeting = next_json(&mut socket).await;
    assert_eq!(greeting["data"]["status"], 0, "first frame is a partial");

    send_json(&mut socket, &request_frame()).await;
    let correction = next_json(&mut socket).await;
    assert_eq!(correction["data"]["pgs"], "apd");
    let final_frame = next_json(&mut socket).await;
    assert_eq!(final_frame["data"]["status"], 2, "session ends with status 2");

    assert_eq!(state.sessions.load(Ordering::SeqCst), 1);
    assert_eq!(state.audio_frames.load(Ordering::SeqCst), 1);
    assert!(!state.received.lock().unwrap().is_empty(), "audio captured");
}

#[tokio::test]
async fn xfyun_mock_rejects_a_skewed_signed_date() {
    let (mock, _state) = xfyun_mock(XfyunMock::default()).await;
    let stale = signed_query(mock.addr, "Thu, 01 Jan 2099 00:00:00 GMT");
    let err = connect_err(&mock.ws_url("/v2/iat"), &stale).await;
    assert_eq!(err, 403, "a stale signature is an HTTP error, not a stream");
    assert_eq!(classify_http_status(403), RetryClass::Client);
}

#[test]
fn fault_injection_http_status_classification() {
    assert_eq!(classify_http_status(429), RetryClass::Retryable);
    assert_eq!(classify_http_status(500), RetryClass::Retryable);
    assert_eq!(classify_http_status(502), RetryClass::Retryable);
    assert_eq!(classify_http_status(503), RetryClass::Retryable);
    assert_eq!(classify_http_status(504), RetryClass::Retryable);
    assert_eq!(classify_http_status(400), RetryClass::Client);
    assert_eq!(classify_http_status(401), RetryClass::Client);
    assert_eq!(classify_http_status(403), RetryClass::Client);
    assert_eq!(classify_http_status(404), RetryClass::Client);
}

#[tokio::test]
async fn deepseek_mock_returns_rate_limited_and_the_error_classifies_retryable() {
    let (mock, state) = deepseek_mock(DeepseekMock::default().with_status(429)).await;
    let response = reqwest::Client::new()
        .post(mock.http_url("/chat/completions"))
        .header("authorization", "Bearer test-key-not-a-secret")
        .json(&json!({ "model": "deepseek-chat", "messages": [] }))
        .send()
        .await
        .expect("mock answers");
    assert_eq!(response.status().as_u16(), 429);
    assert_eq!(classify_http_status(429), RetryClass::Retryable);
    assert_eq!(state.bodies.lock().unwrap().len(), 1, "body was captured");
}

#[tokio::test]
async fn deepgram_mock_delivers_a_malformed_frame_without_killing_the_parser() {
    let (mock, _state) = deepgram_mock(DeepgramMock::default().with_malformed_frame()).await;
    let mut socket = connect_with_token(&mock.ws_url("/v1/listen")).await;

    let garbage = next_text(&mut socket).await;
    assert!(serde_json::from_str::<Value>(&garbage).is_err(), "not JSON");

    // The classification every parser must use for unparseable input.
    assert_eq!(
        StageError::protocol("deepgram", "malformed frame").retry_class,
        RetryClass::Retryable
    );
    assert_eq!(
        StageError::protocol("deepseek", "truncated event").retry_class,
        RetryClass::Retryable
    );
}

#[tokio::test]
async fn deepgram_mock_closes_silent_sessions_with_a_recognisable_reason() {
    let (mock, _state) = deepgram_mock(DeepgramMock::default().requiring_keepalive(300)).await;
    let mut socket = connect_with_token(&mock.ws_url("/v1/listen")).await;

    let mut reason = None;
    for _ in 0..8 {
        match tokio::time::timeout(MOCK_TIMEOUT, socket.next()).await {
            Ok(Some(Ok(Message::Close(frame)))) => {
                reason = frame.map(|f| f.reason.to_string());
                break;
            }
            Ok(Some(Ok(_))) => continue,
            other => panic!("expected a close frame, got {other:?}"),
        }
    }
    let reason = reason.expect("server closes the silent session");
    assert!(reason.contains("NET-0001"), "readable reason, got {reason:?}");
}

#[tokio::test]
async fn volc_mock_replies_with_audio_then_a_finished_frame() {
    let (mock, state) = volc_mock(VolcMock::default()).await;
    let mut socket = connect_with_volc_headers(&mock.ws_url("/api/v3/tts/unidirectional/stream")).await;
    send_binary(&mut socket, &volc_request_frame("hello", "S_test")).await;

    let first = next_bytes(&mut socket).await;
    assert_eq!((first[1] >> 4) & 0x0f, 0b1011, "audio frame");
    assert_eq!(first[4..8], 352u32.to_be_bytes());

    let second = next_bytes(&mut socket).await;
    assert_eq!(second[4..8], 352u32.to_be_bytes());
    let last = next_bytes(&mut socket).await;
    assert_eq!(last[4..8], 152u32.to_be_bytes());

    let headers = state.headers.lock().unwrap();
    assert_eq!(headers[0]["x-api-resource-id"], "seed-icl-2.0");
    assert_eq!(headers[0]["x-api-key"], "test-token-not-a-secret");
    assert!(!headers[0]["x-api-request-id"].is_empty(), "request id present");
    let requests = state.requests.lock().unwrap();
    assert_eq!(requests[0]["req_params"]["speaker"], "S_test");
}

#[tokio::test]
async fn volc_mock_error_frame_carries_code_and_message() {
    let (mock, _state) =
        volc_mock(VolcMock::default().with_error(45_000_001, "invalid speaker")).await;
    let mut socket = connect_with_volc_headers(&mock.ws_url("/api/v3/tts/unidirectional/stream")).await;
    send_binary(&mut socket, &volc_request_frame("hello", "S_test")).await;

    let frame = next_bytes(&mut socket).await;
    assert_eq!((frame[1] >> 4) & 0x0f, 0b1111);
    let code = u32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]);
    let size = u32::from_be_bytes([frame[8], frame[9], frame[10], frame[11]]) as usize;
    assert_eq!(code, 45_000_001);
    assert_eq!(&frame[12..12 + size], b"invalid speaker");
}

// ---------------------------------------------------------------------------
// raw-client helpers (the harness's own client side)
// ---------------------------------------------------------------------------

async fn connect(url: &str, query: &str) -> ClientSocket {
    let (socket, _) = tokio_tungstenite::connect_async(format!("{url}?{query}"))
        .await
        .expect("handshake");
    socket
}

async fn connect_err(url: &str, query: &str) -> u16 {
    match tokio_tungstenite::connect_async(format!("{url}?{query}")).await {
        Ok(_) => panic!("handshake should have been rejected"),
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => response.status().as_u16(),
        Err(other) => panic!("expected an HTTP error, got {other:?}"),
    }
}

async fn connect_with_token(url: &str) -> ClientSocket {
    connect_with_headers(url, &[("authorization", "Token test-key-not-a-secret")]).await
}

async fn connect_with_volc_headers(url: &str) -> ClientSocket {
    connect_with_headers(
        url,
        &[
            ("X-Api-Key", "test-token-not-a-secret"),
            ("X-Api-Resource-Id", "seed-icl-2.0"),
            ("X-Api-Request-Id", "00000000-0000-4000-8000-000000000000"),
        ],
    )
    .await
}

async fn connect_with_headers(url: &str, headers: &[(&str, &str)]) -> ClientSocket {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut request = url.into_client_request().expect("request");
    for (name, value) in headers {
        request.headers_mut().insert(
            axum::http::HeaderName::from_bytes(name.as_bytes()).expect("header name"),
            HeaderValue::from_str(value).expect("header value"),
        );
    }
    let (socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("handshake");
    socket
}

async fn next_text(socket: &mut ClientSocket) -> String {
    match next_message(socket).await {
        Message::Text(text) => text.to_string(),
        other => panic!("expected text, got {other:?}"),
    }
}

async fn next_json(socket: &mut ClientSocket) -> Value {
    serde_json::from_str(&next_text(socket).await).expect("valid JSON frame")
}

async fn next_bytes(socket: &mut ClientSocket) -> Vec<u8> {
    match next_message(socket).await {
        Message::Binary(bytes) => bytes.to_vec(),
        other => panic!("expected binary, got {other:?}"),
    }
}

async fn next_message(socket: &mut ClientSocket) -> Message {
    tokio::time::timeout(MOCK_TIMEOUT, socket.next())
        .await
        .expect("mock answers within the timeout")
        .expect("frame")
        .expect("ok frame")
}

async fn send_json(socket: &mut ClientSocket, body: &str) {
    socket
        .send(Message::Text(body.to_string().into()))
        .await
        .expect("client frame");
}

async fn send_binary(socket: &mut ClientSocket, body: &[u8]) {
    socket
        .send(Message::Binary(body.to_vec().into()))
        .await
        .expect("client frame");
}

/// The signed handshake URL: `authorization`/`date`/`host`/`appid`, exactly the
/// four parameters the real service checks.
fn signed_query(addr: SocketAddr, date: &str) -> String {
    let authorization = "YXBpX2tleT0idGVzdCIsIGFsZ29yaXRobT0iaG1hYy1zaGEyNTYiLCBzaWduYXR1cmU9ImFiYyI=";
    format!(
        "authorization={}&date={}&host={}&appid=test",
        urlencode(authorization),
        urlencode(date),
        addr
    )
}

fn urlencode(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            other => other
                .to_string()
                .bytes()
                .map(|byte| format!("%{byte:02X}"))
                .collect::<Vec<_>>()
                .join(""),
        })
        .collect()
}

fn request_frame() -> String {
    json!({
        "common": { "app_id": "test" },
        "business": {
            "language": "zh_cn",
            "domain": "iat",
            "accent": "mandarin",
            "dwa": "wpgs"
        },
        "data": {
            "status": 1,
            "format": "audio/L16;rate=16000",
            "encoding": "raw",
            "audio": "AAAA"
        }
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// date helpers (the mock side of the 讯飞 signature)
// ---------------------------------------------------------------------------

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Civil date from a Unix day number (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = (year - era * 400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + day as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

fn format_rfc1123(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let secs_of_day = unix_secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let weekday = WEEKDAYS[(days + 4).rem_euclid(7) as usize];
    format!(
        "{weekday}, {day:02} {} {year} {:02}:{:02}:{:02} GMT",
        MONTHS[(month - 1) as usize],
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

fn now_rfc1123() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_rfc1123(secs)
}

fn unix_from_rfc1123(raw: &str) -> Option<i64> {
    let parts: Vec<&str> = raw.split_whitespace().collect();
    if parts.len() != 6 {
        return None;
    }
    let day: u32 = parts[1].parse().ok()?;
    let month = MONTHS.iter().position(|m| *m == parts[2])? as u32 + 1;
    let year: i64 = parts[3].parse().ok()?;
    let time: Vec<&str> = parts[4].split(':').collect();
    if time.len() != 3 {
        return None;
    }
    let hours: i64 = time[0].parse().ok()?;
    let minutes: i64 = time[1].parse().ok()?;
    let seconds: i64 = time[2].parse().ok()?;
    Some(days_from_civil(year, month, day) * 86_400 + hours * 3600 + minutes * 60 + seconds)
}

/// True when the signed date is more than `window_secs` from now. An
/// unparseable date counts as stale — the service would reject it anyway.
fn signed_date_is_stale(raw: &str, window_secs: u64) -> bool {
    let Some(signed) = unix_from_rfc1123(raw) else {
        return true;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    (now - signed).unsigned_abs() > window_secs
}
