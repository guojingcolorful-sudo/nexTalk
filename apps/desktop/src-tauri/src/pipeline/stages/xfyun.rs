//! 讯飞 iat — the user's Chinese line (T2.2).
//!
//! Ported from `tools/vendor-experiments/stt-ab.mjs`, which is the behavioural
//! reference for every constant in this file.
//!
//! # What the protocol actually does
//!
//! A session is one signed WebSocket (`wss://…/v2/iat?authorization=…&date=…`),
//! opened per fragment. Every frame the client sends carries a `data.status`:
//! `0` opens, `1` continues, `2` closes. Every frame the service sends carries
//! its own `data.status`, and only `2` means *"this transcript is done"* — that
//! is the commit gate (D-16): 0 and 1 are interim, no matter how confident the
//! text looks.
//!
//! With `dwa: "wpgs"` enabled the service also *revises* what it already said:
//! `pgs: "apd"` appends, `pgs: "rpl"` replaces the frames whose `sn` lie in
//! `rg: [from, to]` (1-based, inclusive) — and later frames' `sn` shift
//! accordingly. Rebuilding the sentence by string concatenation therefore
//! produces text that never existed; [`TranscriptBuilder`] keeps an ordered
//! `Vec<(sn, text)>` instead.
//!
//! # Deliberate omissions
//!
//! - The `sc` field is **never** read as a confidence score: it is a reserved
//!   field that stays 0. `confidence` is `None` with
//!   `ConfidenceSource::ProxyUnavailable` until 02-03's local proxy lands
//!   (research correction 3).
//! - `eos` is configured but not driven locally: silence detection is 02-03's
//!   job (T3.2). This file only guarantees the *semantics* are right.
//! - Nothing here logs a full URL or a header. The signed URL embeds the API
//!   key; only `sanitize_endpoint(url)` (host + path) may reach a log or an
//!   error.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use crate::pipeline::budget::Stage;

use super::config::{Endpoint, Endpoints, XfyunCredentials};
use super::error::{classify_http_status, classify_xfyun_code, ErrorKind, StageError};
use super::traits::{
    ConfidenceSource, MarkHandle, SttEvent, SttPartial, SttSource, SttStream, SttUpstream,
    AUDIO_QUEUE_FRAMES, EVENT_QUEUE_ITEMS,
};

// ---------------------------------------------------------------------------
// protocol constants
// ---------------------------------------------------------------------------

/// 40 ms of 16 kHz / 16-bit mono PCM — one upstream frame.
pub const AUDIO_FRAME_BYTES: usize = 1_280;

/// The service rejects an oversize `data.audio` with code 10163; base64 grows
/// 4/3, so the *encoded* frame must stay under this many characters.
pub const MAX_BASE64_CHARS: usize = 13_000;

/// The service caps a session at 60 s and then just stops answering.
pub const DEFAULT_SESSION_CAP_MS: u64 = 60_000;

/// `eos`: how long a silence may last before the service finalises. Passed as
/// a hint; the cadence is enforced by the service.
pub const DEFAULT_EOS_MS: u32 = 2_000;

/// The iat endpoint accepts 16 kHz L16 mono only.
pub const SAMPLE_RATE_HZ: u32 = 16_000;

/// Rotate at 90% of the cap, leaving room for the terminal frame.
const ROTATE_AT_NUMERATOR: u64 = 9;
const ROTATE_AT_DENOMINATOR: u64 = 10;

const PROVIDER: &str = "xfyun";
const MODEL_VERSION: &str = "iat";

// ---------------------------------------------------------------------------
// handshake
// ---------------------------------------------------------------------------

/// The exact string the HMAC runs over.
///
/// Its field order is part of the protocol, not a formatting choice: the
/// service rebuilds this string server-side, so any drift is a 403 that looks
/// like a clock problem. Test 1 pins it byte-for-byte.
pub fn signature_string(host: &str, path: &str, date: &str) -> String {
    format!("host: {host}\ndate: {date}\nGET {path} HTTP/1.1")
}

/// base64(HMAC-SHA256(api_secret, [`signature_string`])).
pub fn sign(api_secret: &str, host: &str, path: &str, date: &str) -> String {
    // HMAC accepts a key of any length — the only error `new_from_slice` can
    // return is unreachable for SHA-256.
    let mut mac = Hmac::<Sha256>::new_from_slice(api_secret.as_bytes())
        .expect("HMAC-SHA256 accepts keys of any length");
    mac.update(signature_string(host, path, date).as_bytes());
    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

/// base64 of the `api_key="…", algorithm="hmac-sha256", …` pair list the
/// `authorization` query parameter carries.
pub fn authorization_header(api_key: &str, signature: &str) -> String {
    let raw = format!(
        "api_key=\"{api_key}\", algorithm=\"hmac-sha256\", headers=\"host date request-line\", signature=\"{signature}\""
    );
    base64::engine::general_purpose::STANDARD.encode(raw)
}

/// Build the signed handshake URL for `endpoint`.
///
/// `host` in the signature is the URL authority (`host:port` for a test
/// server) and the path is what the socket actually dials — the service
/// rebuilds both from the request line, so they must match the URL exactly.
pub fn signed_url(
    endpoint: &str,
    api_key: &str,
    api_secret: &str,
    app_id: &str,
    date: &str,
) -> Result<String, StageError> {
    let (scheme, rest) = endpoint.split_once("://").ok_or_else(|| {
        StageError::new(
            PROVIDER,
            ErrorKind::Config,
            super::error::RetryClass::Terminal,
            format!(
                "not a websocket endpoint: {}",
                super::error::sanitize_endpoint(endpoint)
            ),
        )
    })?;
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    if authority.is_empty() {
        return Err(StageError::new(
            PROVIDER,
            ErrorKind::Config,
            super::error::RetryClass::Terminal,
            "the websocket endpoint has no host",
        ));
    }

    let signature = sign(api_secret, authority, &path, date);
    Ok(format!(
        "{scheme}://{authority}{path}?authorization={}&date={}&host={}&appid={}",
        encode_component(&authorization_header(api_key, &signature)),
        encode_component(date),
        encode_component(authority),
        encode_component(app_id),
    ))
}

/// Percent-encoding for the query values.
///
/// The reference script uses `encodeURIComponent`, which renders a space as
/// `%20`; that is what the service's samples do, so we do the same rather than
/// the form-encoding `+` (accepted by our mock's parser, but ambiguous to a
/// strict query reader).
fn encode_component(raw: &str) -> String {
    let mut encoded = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

/// A handshake that the service refused. The URL is sanitised first: it
/// carries the API key and the signature.
pub fn handshake_error(status: u16, url: &str) -> StageError {
    let kind = if matches!(status, 401 | 403) {
        ErrorKind::Auth
    } else {
        ErrorKind::Http
    };
    let message = if matches!(status, 401 | 403) {
        format!(
            "handshake rejected (HTTP {status}): the signed date was refused — check the clock on this machine"
        )
    } else {
        format!("handshake rejected (HTTP {status})")
    };
    StageError::new(PROVIDER, kind, classify_http_status(status), message).with_endpoint(url)
}

/// RFC 1123 UTC — the `date` parameter's format (`Thu, 01 Jan 2026 00:00:00 GMT`).
pub fn rfc1123(unix_secs: i64) -> String {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = unix_secs.div_euclid(86_400);
    let secs_of_day = unix_secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let weekday = WEEKDAYS[(days + 4).rem_euclid(7) as usize];
    format!(
        "{weekday}, {day:02} {} {year} {:02}:{:02}:{:02} GMT",
        MONTHS[(month - 1) as usize],
        secs_of_day / 3_600,
        (secs_of_day % 3_600) / 60,
        secs_of_day % 60
    )
}

/// Days since the Unix epoch → civil date (Howard Hinnant's algorithm, the
/// same one the mock uses — the two implementations cross-check each other).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// transcript reconstruction
// ---------------------------------------------------------------------------

/// One result frame, reduced to what the sentence builder needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WpgsFrame {
    pub sn: u32,
    pub text: String,
    /// `apd` (append) or `rpl` (replace); absent means append.
    pub pgs: Option<String>,
    /// The 1-based, inclusive `sn` range a `rpl` rewrites.
    pub rg: Option<(u32, u32)>,
}

impl WpgsFrame {
    pub fn append(sn: u32, text: impl Into<String>) -> Self {
        Self {
            sn,
            text: text.into(),
            pgs: None,
            rg: None,
        }
    }

    pub fn replace(sn: u32, from: u32, to: u32, text: impl Into<String>) -> Self {
        Self {
            sn,
            text: text.into(),
            pgs: Some("rpl".to_string()),
            rg: Some((from, to)),
        }
    }
}

/// Rebuilds the sentence from `sn`-keyed frames instead of concatenating
/// strings, so a `rpl` can rewrite text that was already emitted.
#[derive(Debug, Default, Clone)]
pub struct TranscriptBuilder {
    entries: Vec<(u32, String)>,
}

impl TranscriptBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.entries.clear();
    }

    pub fn text(&self) -> String {
        self.entries.iter().map(|(_, text)| text.as_str()).collect()
    }

    /// Apply one frame; returns whether it rewrote emitted text.
    pub fn apply(&mut self, frame: WpgsFrame) -> bool {
        let is_replace = frame.pgs.as_deref() == Some("rpl");
        match (is_replace, frame.rg) {
            (true, Some((from, to))) => {
                match self
                    .entries
                    .iter()
                    .position(|(sn, _)| *sn >= from && *sn <= to)
                {
                    Some(index) => {
                        self.entries.retain(|(sn, _)| *sn < from || *sn > to);
                        self.entries
                            .insert(index.min(self.entries.len()), (frame.sn, frame.text));
                    }
                    // A replace for a range we never emitted: append rather
                    // than drop — losing text is worse than an odd sentence.
                    None => self.entries.push((frame.sn, frame.text)),
                }
                true
            }
            _ => {
                self.entries.push((frame.sn, frame.text));
                false
            }
        }
    }
}

/// `data.status` → the frame's finality. Only `2` commits (D-16).
pub fn frame_is_final(status: u32) -> bool {
    status == 2
}

/// Build the [`SttPartial`] a caller sees for one result frame.
pub fn partial_from_frame(
    status: u32,
    text: impl Into<String>,
    revision_applied: bool,
    model_version: &str,
) -> SttPartial {
    let is_final = frame_is_final(status);
    SttPartial {
        text: text.into(),
        is_final,
        committed: is_final,
        revision_applied,
        // `sc` is reserved and always 0 — never a score (research correction 3).
        confidence: None,
        confidence_source: ConfidenceSource::ProxyUnavailable,
        model_version: model_version.to_string(),
        provider: PROVIDER.to_string(),
    }
}

// ---------------------------------------------------------------------------
// audio framing
// ---------------------------------------------------------------------------

/// Split PCM16 bytes into upstream frames, each safely under the base64 limit
/// that produces error 10163.
pub fn audio_frames(pcm16_le: &[u8]) -> Vec<Vec<u8>> {
    pcm16_le
        .chunks(AUDIO_FRAME_BYTES)
        .map(<[u8]>::to_vec)
        .collect()
}

/// `-1.0`…`1.0` f32 samples → PCM16 little-endian bytes.
pub fn f32_to_pcm16_le(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let scaled = (clamped * 32_767.0).round() as i16;
        bytes.extend_from_slice(&scaled.to_le_bytes());
    }
    bytes
}

// ---------------------------------------------------------------------------
// wire shapes
// ---------------------------------------------------------------------------

/// Result frame from the service. Unknown fields are ignored on purpose: the
/// vendor adds fields without notice, and a strict parse would turn a new
/// field into a dropped transcript (the opposite of our own wire types, which
/// are `deny_unknown_fields` because *we* control both ends).
#[derive(Debug, Deserialize)]
struct ServerFrame {
    #[serde(default)]
    code: i32,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<FrameData>,
}

#[derive(Debug, Deserialize)]
struct FrameData {
    #[serde(default)]
    sn: u32,
    #[serde(default)]
    status: u32,
    #[serde(default)]
    pgs: Option<String>,
    #[serde(default)]
    rg: Option<Vec<u32>>,
    #[serde(default)]
    result: Option<FrameResult>,
}

#[derive(Debug, Deserialize)]
struct FrameResult {
    #[serde(default)]
    ws: Vec<WsChunk>,
}

#[derive(Debug, Deserialize)]
struct WsChunk {
    #[serde(default)]
    cw: Vec<CandidateWord>,
}

#[derive(Debug, Deserialize)]
struct CandidateWord {
    #[serde(default)]
    w: String,
}

impl FrameData {
    fn text(&self) -> String {
        self.result
            .as_ref()
            .map(|result| {
                result
                    .ws
                    .iter()
                    .flat_map(|chunk| chunk.cw.iter())
                    .map(|word| word.w.as_str())
                    .collect::<String>()
            })
            .unwrap_or_default()
    }

    fn to_wpgs(&self) -> WpgsFrame {
        WpgsFrame {
            sn: self.sn,
            text: self.text(),
            pgs: self.pgs.clone(),
            rg: self.rg.as_ref().and_then(|range| match range.as_slice() {
                [from, to] => Some((*from, *to)),
                _ => None,
            }),
        }
    }
}

fn request_frame(app_id: &str, eos_ms: u32, status: u32, audio: &[u8]) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(audio);
    let mut frame = json!({
        "data": {
            "status": status,
            "format": format!("audio/L16;rate={SAMPLE_RATE_HZ}"),
            "encoding": "raw",
            "audio": encoded,
        }
    });
    if status == 0 {
        // `dwa: wpgs` is what makes `rpl` arrive at all; without it the
        // service never revises and Test 4 could not exist.
        frame["common"] = json!({ "app_id": app_id });
        frame["business"] = json!({
            "language": "zh_cn",
            "domain": "iat",
            "accent": "mandarin",
            "dwa": "wpgs",
            "eos": eos_ms,
        });
    }
    frame.to_string()
}

// ---------------------------------------------------------------------------
// the client
// ---------------------------------------------------------------------------

/// The 讯飞 streaming-dictation stage.
#[derive(Debug, Clone)]
pub struct XfyunStt {
    credentials: XfyunCredentials,
    endpoints: Endpoints,
    marks: MarkHandle,
    eos_ms: u32,
    session_cap_ms: u64,
    /// Test seam: sign with a fixed date (the stale-clock path).
    signed_date: Option<String>,
}

impl XfyunStt {
    pub fn new(credentials: XfyunCredentials, endpoints: Endpoints) -> Self {
        Self {
            credentials,
            endpoints,
            marks: MarkHandle::disabled(),
            eos_ms: DEFAULT_EOS_MS,
            session_cap_ms: DEFAULT_SESSION_CAP_MS,
            signed_date: None,
        }
    }

    /// Shorten the 60 s rotation for a test (mirrors the mock's cap).
    pub fn with_session_cap_ms(mut self, session_cap_ms: u64) -> Self {
        self.session_cap_ms = session_cap_ms;
        self
    }

    pub fn with_eos_ms(mut self, eos_ms: u32) -> Self {
        self.eos_ms = eos_ms;
        self
    }

    /// Sign with an explicit date instead of the clock — the 403 path.
    pub fn with_signed_date(mut self, date: impl Into<String>) -> Self {
        self.signed_date = Some(date.into());
        self
    }

    /// The endpoint this client dials (the base URL, before signing).
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoints.xfyun_ws
    }

    fn build_url(&self) -> Result<String, StageError> {
        let date = self
            .signed_date
            .clone()
            .unwrap_or_else(|| rfc1123(now_unix_secs()));
        signed_url(
            self.endpoints.xfyun_ws.url(),
            self.credentials.api_key.expose(),
            self.credentials.api_secret.expose(),
            &self.credentials.app_id,
            &date,
        )
    }
}

impl SttSource for XfyunStt {
    fn provider(&self) -> &'static str {
        PROVIDER
    }

    fn model_version(&self) -> String {
        MODEL_VERSION.to_string()
    }

    fn set_marks(&mut self, marks: MarkHandle) {
        self.marks = marks;
    }

    fn start(&mut self, _epoch: u64) -> Result<SttStream, StageError> {
        // Sign once, up front: a configuration error is returned from
        // `start()` rather than surfacing asynchronously.
        let url = self.build_url()?;
        let credentials = self.credentials.clone();
        let marks = self.marks.clone();
        let eos_ms = self.eos_ms;
        let session_cap_ms = self.session_cap_ms;
        let endpoint = self.endpoints.xfyun_ws.clone();

        let (upstream_tx, upstream_rx) = mpsc::channel(AUDIO_QUEUE_FRAMES);
        let (events_tx, events_rx) = mpsc::channel(EVENT_QUEUE_ITEMS);

        tokio::spawn(async move {
            run_session(
                Session {
                    url,
                    endpoint,
                    app_id: credentials.app_id.clone(),
                    eos_ms,
                    session_cap_ms,
                    marks,
                },
                upstream_rx,
                events_tx,
            )
            .await;
        });

        Ok(SttStream::new(PROVIDER, upstream_tx, events_rx))
    }
}

struct Session {
    url: String,
    endpoint: Endpoint,
    app_id: String,
    eos_ms: u32,
    session_cap_ms: u64,
    marks: MarkHandle,
}

impl Session {
    fn rotate_deadline(&self) -> Duration {
        Duration::from_millis(
            self.session_cap_ms.saturating_mul(ROTATE_AT_NUMERATOR) / ROTATE_AT_DENOMINATOR,
        )
    }

    /// Re-sign and dial. Logs and errors carry the sanitised endpoint only.
    async fn connect(&self) -> Result<ClientSocket, StageError> {
        let request = self.url.clone().into_client_request().map_err(|error| {
            StageError::new(
                PROVIDER,
                ErrorKind::Config,
                super::error::RetryClass::Terminal,
                format!("the signed URL is not a valid request: {error}"),
            )
        })?;
        match tokio_tungstenite::connect_async(request).await {
            Ok((socket, _response)) => Ok(socket),
            Err(tokio_tungstenite::tungstenite::Error::Http(response)) => Err(handshake_error(
                response.status().as_u16(),
                self.endpoint.url(),
            )),
            Err(error) => Err(
                StageError::transport(PROVIDER, format!("connect failed: {error}"))
                    .with_endpoint(self.endpoint.url()),
            ),
        }
    }
}

type ClientSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn fail(events: &mpsc::Sender<SttEvent>, error: StageError) {
    // A send failure means the caller is gone; nothing left to report to.
    let _ = events.try_send(SttEvent::Failed(error));
}

async fn send_text(socket: &mut ClientSocket, body: String) -> Result<(), StageError> {
    socket
        .send(Message::Text(body.into()))
        .await
        .map_err(|error| StageError::transport(PROVIDER, format!("send failed: {error}")))
}

async fn run_session(
    session: Session,
    mut upstream: mpsc::Receiver<SttUpstream>,
    events: mpsc::Sender<SttEvent>,
) {
    let mut socket = match session.connect().await {
        Ok(socket) => socket,
        Err(error) => return fail(&events, error),
    };

    let mut builder = TranscriptBuilder::new();
    // Text committed by earlier sessions of the same fragment (rotation).
    let mut carried_text = String::new();
    let mut session_started = Instant::now();
    let mut session_open = false;
    let mut fragment_ended = false;
    let mut marked = false;

    loop {
        tokio::select! {
            upstream_item = upstream.recv(), if !fragment_ended => {
                match upstream_item {
                    Some(SttUpstream::Audio(pcm16)) => {
                        // Rotate before the service's 60 s cap: re-sign, dial,
                        // and let the next frame reopen with `status: 0`.
                        if session_open && session_started.elapsed() >= session.rotate_deadline() {
                            carried_text = builder.text();
                            builder.reset();
                            match session.connect().await {
                                Ok(fresh) => {
                                    socket = fresh;
                                    session_started = Instant::now();
                                    session_open = false;
                                }
                                Err(error) => return fail(&events, error),
                            }
                        }
                        let bytes = pcm16_to_le_bytes(&pcm16);
                        for chunk in audio_frames(&bytes) {
                            let status = if session_open { 1 } else { 0 };
                            let frame = request_frame(&session.app_id, session.eos_ms, status, &chunk);
                            if let Err(error) = send_text(&mut socket, frame).await {
                                return fail(&events, error);
                            }
                            session_open = true;
                        }
                    }
                    Some(SttUpstream::End) | None => {
                        fragment_ended = true;
                        let frame = request_frame(&session.app_id, session.eos_ms, 2, &[]);
                        if let Err(error) = send_text(&mut socket, frame).await {
                            return fail(&events, error);
                        }
                    }
                }
            }
            message = socket.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        match handle_frame(text.as_str(), &mut builder, &mut carried_text, &session, &events, &mut marked).await {
                            FrameOutcome::Continue => {}
                            FrameOutcome::Finished => {
                                let _ = socket.close(None).await;
                                return;
                            }
                            FrameOutcome::Fatal(error) => {
                                let _ = socket.close(None).await;
                                return fail(&events, error);
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        let error = if fragment_ended {
                            StageError::transport(PROVIDER, "the session closed before the final transcript")
                        } else {
                            StageError::transport(PROVIDER, "the session closed before the fragment ended")
                        };
                        return fail(&events, error);
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        return fail(&events, StageError::transport(PROVIDER, format!("socket error: {error}")));
                    }
                }
            }
        }
    }
}

enum FrameOutcome {
    Continue,
    Finished,
    Fatal(StageError),
}

async fn handle_frame(
    raw: &str,
    builder: &mut TranscriptBuilder,
    carried_text: &mut String,
    session: &Session,
    events: &mpsc::Sender<SttEvent>,
    marked: &mut bool,
) -> FrameOutcome {
    let frame: ServerFrame = match serde_json::from_str(raw) {
        Ok(frame) => frame,
        Err(error) => {
            return FrameOutcome::Fatal(StageError::protocol(
                PROVIDER,
                format!("unparseable frame: {error}"),
            ))
        }
    };
    if frame.code != 0 {
        let message = frame
            .message
            .clone()
            .unwrap_or_else(|| "no detail".to_string());
        let error = StageError::new(
            PROVIDER,
            ErrorKind::Vendor,
            classify_xfyun_code(frame.code),
            format!("service code {}: {message}", frame.code),
        );
        return FrameOutcome::Fatal(error);
    }
    let Some(data) = frame.data else {
        return FrameOutcome::Continue;
    };

    let revision_applied = builder.apply(data.to_wpgs());
    let is_final = frame_is_final(data.status);
    // Text from sessions this one rotated away from is still part of the
    // fragment; a final frame closes the fragment, so it is not carried on.
    let text = if is_final {
        carried_text.clear();
        builder.text()
    } else {
        format!("{carried_text}{}", builder.text())
    };
    let partial = partial_from_frame(data.status, text, revision_applied, MODEL_VERSION);
    if !*marked {
        *marked = true;
        session.marks.mark(Stage::SttFirstPartial);
    }
    if events.send(SttEvent::Partial(partial)).await.is_err() {
        return FrameOutcome::Finished;
    }
    if is_final {
        FrameOutcome::Finished
    } else {
        FrameOutcome::Continue
    }
}

/// `Vec<i16>` (already the API's sample format) → little-endian bytes.
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
    use crate::pipeline::stages::traits::ConfidenceSource;
    use base64::Engine;

    const TEST_SECRET: &str = "test-secret-value";
    const TEST_API_KEY: &str = "test-api-key";
    const TEST_HOST: &str = "iat-api.xfyun.cn";
    const TEST_DATE: &str = "Thu, 01 Jan 2026 00:00:00 GMT";

    /// Precomputed with the reference script (`stt-ab.mjs`, node 21):
    ///
    /// ```text
    /// origin    = "host: iat-api.xfyun.cn\ndate: Thu, 01 Jan 2026 00:00:00 GMT\nGET /v2/iat HTTP/1.1"
    /// signature = base64(HMAC-SHA256("test-secret-value", origin))
    /// ```
    const EXPECTED_SIGNATURE: &str = "aI+0YqF5gES5iuTV7rujfAVT4Gg4+Dqn7fnsMJkmVpw=";
    /// `Buffer.from('api_key="…", algorithm="hmac-sha256", headers="host date request-line", signature="…"')`
    /// — the separator before `signature=` is a comma **and a space**.
    const EXPECTED_AUTHORIZATION: &str = "YXBpX2tleT0idGVzdC1hcGkta2V5IiwgYWxnb3JpdGhtPSJobWFjLXNoYTI1NiIsIGhlYWRlcnM9Imhvc3QgZGF0ZSByZXF1ZXN0LWxpbmUiLCBzaWduYXR1cmU9ImFJKzBZcUY1Z0VTNWl1VFY3cnVqZkFWVDRHZzQrRHFuN2Zuc01Ka21WcHc9Ig==";

    #[test]
    fn signature_matches_the_precomputed_reference_bytes() {
        // Test 1: the signing string's order is fixed — this is the regression
        // gate against a silent drift in linkage testing.
        assert_eq!(
            signature_string(TEST_HOST, "/v2/iat", TEST_DATE),
            "host: iat-api.xfyun.cn\ndate: Thu, 01 Jan 2026 00:00:00 GMT\nGET /v2/iat HTTP/1.1"
        );
        assert_eq!(
            sign(TEST_SECRET, TEST_HOST, "/v2/iat", TEST_DATE),
            EXPECTED_SIGNATURE
        );
        assert_eq!(
            authorization_header(TEST_API_KEY, EXPECTED_SIGNATURE),
            EXPECTED_AUTHORIZATION
        );
    }

    #[test]
    fn the_signed_url_carries_every_parameter_the_service_checks() {
        let url = signed_url(
            "wss://iat-api.xfyun.cn/v2/iat",
            TEST_API_KEY,
            TEST_SECRET,
            "appid-test",
            TEST_DATE,
        )
        .expect("the default endpoint parses");

        assert!(url.starts_with("wss://iat-api.xfyun.cn/v2/iat?authorization="));
        assert!(url.contains(&format!("&host={TEST_HOST}")));
        assert!(url.contains("&appid=appid-test"));
        assert!(url.contains("date=Thu%2C%2001%20Jan%202026%2000%3A00%3A00%20GMT"));
    }

    #[test]
    fn the_signed_url_uses_the_authority_and_path_of_a_loopback_mock() {
        let url = signed_url(
            "ws://127.0.0.1:41234/v2/iat",
            TEST_API_KEY,
            TEST_SECRET,
            "appid-test",
            TEST_DATE,
        )
        .expect("parses");
        // The `host` parameter must match what the socket actually dials.
        assert!(url.contains("host=127.0.0.1%3A41234"), "{url}");
    }

    #[test]
    fn an_unparseable_endpoint_is_a_configuration_error() {
        let error = signed_url("nonsense", TEST_API_KEY, TEST_SECRET, "app", TEST_DATE)
            .expect_err("no scheme, no host");
        assert_eq!(
            error.retry_class,
            crate::pipeline::stages::error::RetryClass::Terminal
        );
        assert_eq!(
            error.kind,
            crate::pipeline::stages::error::ErrorKind::Config
        );
    }

    #[test]
    fn wpgs_append_builds_the_sentence_in_order() {
        // Test 3
        let mut builder = TranscriptBuilder::new();
        assert!(!builder.apply(WpgsFrame::append(1, "你能")));
        assert!(!builder.apply(WpgsFrame::append(2, "详细")));
        assert_eq!(builder.text(), "你能详细");
    }

    #[test]
    fn wpgs_replace_rewrites_the_covered_range() {
        // Test 4: `rg:[1,2]` is 1-based and inclusive.
        let mut builder = TranscriptBuilder::new();
        builder.apply(WpgsFrame::append(1, "你能"));
        builder.apply(WpgsFrame::append(2, "详细"));
        assert!(
            builder.apply(WpgsFrame::replace(3, 1, 2, "你能详述")),
            "a replace is reported as a revision"
        );
        assert_eq!(builder.text(), "你能详述");
    }

    #[test]
    fn a_partial_replace_keeps_the_frames_outside_the_range() {
        let mut builder = TranscriptBuilder::new();
        builder.apply(WpgsFrame::append(1, "你能"));
        builder.apply(WpgsFrame::append(2, "详细"));
        builder.apply(WpgsFrame::append(3, "说一下"));
        assert!(builder.apply(WpgsFrame::replace(4, 1, 1, "您能")));
        assert_eq!(builder.text(), "您能详细说一下");
    }

    #[test]
    fn a_replace_of_an_unknown_range_appends_instead_of_dropping_text() {
        let mut builder = TranscriptBuilder::new();
        builder.apply(WpgsFrame::append(1, "你能"));
        assert!(builder.apply(WpgsFrame::replace(2, 7, 9, "了吗")));
        assert_eq!(builder.text(), "你能了吗", "no text is ever lost");
    }

    #[test]
    fn only_status_two_commits() {
        // Test 5 — 02-03's GOV-15 depends on this contract.
        for status in [0, 1] {
            let partial = partial_from_frame(status, "你能", false, "iat");
            assert!(!partial.is_final, "status {status} is interim");
            assert!(!partial.committed, "status {status} must not be spoken");
        }
        let final_frame = partial_from_frame(2, "你能详细说说优化步骤吗？", true, "iat");
        assert!(final_frame.is_final);
        assert!(final_frame.committed);
        assert_eq!(final_frame.text, "你能详细说说优化步骤吗？");
    }

    #[test]
    fn confidence_is_never_invented_from_the_reserved_sc_field() {
        let partial = partial_from_frame(1, "你能", false, "iat");
        assert_eq!(partial.confidence, None);
        assert_eq!(
            partial.confidence_source,
            ConfidenceSource::ProxyUnavailable,
            "讯飞 has no score until 02-03's proxy"
        );
        assert_eq!(partial.provider, "xfyun");
        assert_eq!(partial.model_version, "iat");
    }

    #[test]
    fn audio_is_split_below_the_10163_threshold() {
        // Test 6: 9751 bytes would encode to 13004 chars — over the limit.
        let oversized = vec![7u8; 9_751];
        let frames = audio_frames(&oversized);
        assert_eq!(
            frames.iter().map(Vec::len).sum::<usize>(),
            9_751,
            "no audio is dropped"
        );
        for frame in &frames {
            assert!(frame.len() <= AUDIO_FRAME_BYTES, "one frame per 40 ms");
            let encoded = base64::engine::general_purpose::STANDARD.encode(frame);
            assert!(
                encoded.len() < MAX_BASE64_CHARS,
                "encoded frame is {} chars",
                encoded.len()
            );
        }
        assert_eq!(
            frames.iter().map(Vec::len).collect::<Vec<_>>(),
            [1_280, 1_280, 1_280, 1_280, 1_280, 1_280, 1_280, 791]
        );
    }

    #[test]
    fn an_empty_audio_push_produces_no_frame() {
        assert!(audio_frames(&[]).is_empty());
    }

    #[test]
    fn handshake_failures_never_render_a_signed_url_or_a_key() {
        // Test 9
        let error = handshake_error(
            403,
            "wss://iat-api.xfyun.cn/v2/iat?authorization=secret-abc&date=now&appid=x",
        );
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains("authorization="), "{rendered}");
            assert!(!rendered.contains("secret-abc"), "{rendered}");
            assert!(!rendered.contains("appid=x"), "{rendered}");
        }
        // `Display` carries no endpoint at all (see the `error` module); the
        // sanitised host/path lives on the `Debug` rendering.
        assert!(
            format!("{error:?}").contains("iat-api.xfyun.cn/v2/iat"),
            "the host survives for diagnostics: {error:?}"
        );
        assert!(error.to_string().contains("clock"), "{}", error);
        assert_eq!(
            error.retry_class,
            crate::pipeline::stages::error::RetryClass::Client,
            "a stale clock is our problem, not a transient vendor fault"
        );
    }
}
