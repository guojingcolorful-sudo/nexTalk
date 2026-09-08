//! LAN server (01-02 walking skeleton): pairing-as-auth WebSocket endpoint and
//! static H5 bundle serving.
//!
//! Security model (threat register T-01-01/T-01-02/T-01-03):
//! - upgrade to `/ws` requires `?token=` matching the 128-bit session token
//!   (401 otherwise) — pairing-as-auth;
//! - inbound frames are capped at 64 KiB and parsed with `deny_unknown_fields`;
//!   any violation drops the connection;
//! - static responses carry `Cache-Control: no-store` (QR/h5 content must never
//!   be cached across sessions).
//!
//! This module also hosts the serde mirror of `packages/protocol` — the exact
//! wire union both transports (Tauri `session` emit + LAN WS broadcast) carry.

use std::path::PathBuf;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::header::{CACHE_CONTROL, HeaderValue};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Router;
use serde::{Deserialize, Serialize};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::services::ServeDir;

use crate::state::SessionState;

/// Max inbound WebSocket frame size (T-01-02): 64 KiB.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

/// Speaker side of a subtitle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    Interviewer,
    User,
}

/// Subtitle language preference (H5 copilot controls land in 01-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LanguagePref {
    AllZh,
    AllEn,
    Bilingual,
}

/// Session lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Idle,
    Listening,
    Generating,
    Ended,
}

/// Serde mirror of `packages/protocol` `ServerEvent`.
///
/// Wire-shape invariants (mirrored by `isServerEvent` on the TS side):
/// - `zh`/`en` are absent (never `null`) when unknown — hence
///   `skip_serializing_if = "Option::is_none"` and no default-less nulls;
/// - `final` (reserved word) is spelled out via an explicit rename;
/// - unknown fields are rejected on both sides (`deny_unknown_fields`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerEvent {
    #[serde(rename_all = "camelCase")]
    Subtitle {
        id: String,
        speaker: Speaker,
        seq: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        zh: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        en: Option<String>,
        #[serde(rename = "final")]
        final_flag: bool,
    },
    #[serde(rename_all = "camelCase")]
    Strategy {
        id: String,
        round_id: String,
        title: String,
        #[serde(default)]
        bullets: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    Language { language: LanguagePref },
    #[serde(rename_all = "camelCase")]
    Status { session: SessionStatus },
    #[serde(rename_all = "camelCase")]
    Timeline { events: Vec<ServerEvent> },
}

/// Serde mirror of `packages/protocol` `ClientMessage` (H5 → desktop).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    #[serde(rename_all = "camelCase")]
    Control { language: LanguagePref },
    #[serde(rename_all = "camelCase")]
    Resume { since_seq: u64 },
}

/// Query parameters on the `/ws` upgrade request.
#[derive(Debug, Deserialize)]
pub struct PairingParams {
    token: String,
}

/// Full LAN router: pairing WS endpoint + static H5 bundle (no-store).
pub fn router(state: SessionState, teleprompter_dist: PathBuf) -> Router {
    todo!("01-02 GREEN: /ws token gate + ServeDir with no-store header layer")
}

/// Upgrade handler: 401 on token mismatch, else run the client loop.
async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<PairingParams>,
    State(state): State<SessionState>,
) -> Response {
    todo!("01-02 GREEN: 401 on token mismatch, else on_upgrade(client_loop)")
}

/// Per-connection loop: broadcast fan-out to the H5 + strict inbound parse
/// (resume replay, language control, deny_unknown_fields drop).
async fn client_loop(socket: WebSocket, state: SessionState) {
    todo!("01-02 GREEN: broadcast fan-out + strict inbound parse")
}

/// Path to the teleprompter H5 build served over LAN (`apps/teleprompter/dist`).
pub fn teleprompter_dist_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("manifest lives under apps/<app>/src-tauri")
        .join("teleprompter")
        .join("dist")
}

#[cfg(test)]
pub(crate) mod test_events {
    //! Event builders shared by the server / state / sim test modules.

    use super::{ServerEvent, Speaker};

    pub fn subtitle_question() -> ServerEvent {
        ServerEvent::Subtitle {
            id: "r1-q".into(),
            speaker: Speaker::Interviewer,
            seq: 1,
            zh: Some("你能详细说一下你优化数据库的具体步骤吗？".into()),
            en: Some(
                "Could you walk me through the specific steps you took to optimize the database?"
                    .into(),
            ),
            final_flag: true,
        }
    }

    pub fn user_answer() -> ServerEvent {
        ServerEvent::Subtitle {
            id: "r1-a".into(),
            speaker: Speaker::User,
            seq: 2,
            zh: Some("首先，我们分析了慢查询日志，发现主要瓶颈在商品详情页的连表查询上。".into()),
            en: None,
            final_flag: true,
        }
    }

    pub fn strategy_event() -> ServerEvent {
        ServerEvent::Strategy {
            id: "s-r1".into(),
            round_id: "r1".into(),
            title: "数据库优化".into(),
            bullets: vec![
                "慢查询日志定位".into(),
                "拆连表查询".into(),
                "Redis 缓存层".into(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SessionState;
    use futures_util::stream::Stream;
    use futures_util::{SinkExt, StreamExt};
    use std::time::Duration;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message as ClientFrame;
    use tokio_tungstenite::tungstenite::Error as WsError;

    /// Boots the real router on an ephemeral port; returns base ws:// URL.
    async fn spawn_server() -> (String, SessionState) {
        let state = SessionState::new(8787);
        let app = router(state.clone(), teleprompter_dist_path());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("ephemeral addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("axum serve");
        });
        (format!("ws://{addr}/ws"), state)
    }

    fn ws_url(base: &str, token: &str) -> String {
        format!("{base}?token={token}")
    }

    /// Reads the next text frame and parses it as a ServerEvent (5s timeout).
    async fn read_event<S>(ws: &mut S) -> Result<ServerEvent, String>
    where
        S: Stream<Item = Result<ClientFrame, WsError>> + Unpin,
    {
        let frame = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .map_err(|_| "timeout waiting for ws frame".to_string())?
            .ok_or_else(|| "ws stream ended".to_string())?
            .map_err(|e| format!("frame error: {e}"))?;
        match frame {
            ClientFrame::Text(text) => {
                serde_json::from_str(&text).map_err(|e| format!("invalid ServerEvent json: {e}"))
            }
            other => Err(format!("expected text frame, got {other:?}")),
        }
    }

    #[tokio::test]
    async fn wrong_token_is_rejected_with_401() {
        let (base, _state) = spawn_server().await;
        let err = connect_async(&ws_url(&base, "wrong-token")).await;
        match err {
            Err(WsError::Http(response)) => {
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            }
            other => panic!("expected HTTP 401 on bad token, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn valid_token_upgrades_and_receives_broadcast() {
        let (base, state) = spawn_server().await;
        let (mut ws, _) = connect_async(&ws_url(&base, &state.pairing_token()))
            .await
            .expect("upgrade accepted for valid token");

        // Round-trip first: the resume reply proves the server's broadcast
        // subscription is live before we append the event (avoids a race).
        ws.send(ClientFrame::Text(r#"{"t":"resume","sinceSeq":0}"#.into()))
            .await
            .expect("send resume");
        let reply = read_event(&mut ws).await.expect("resume reply");
        assert_eq!(reply, ServerEvent::Timeline { events: vec![] });

        let ev = test_events::strategy_event();
        state.append_event(ev.clone());
        let received = read_event(&mut ws).await.expect("broadcast delivery");
        assert_eq!(received, ev);
    }

    #[tokio::test]
    async fn resume_replays_timeline_from_since_seq() {
        let (base, state) = spawn_server().await;
        let q = test_events::subtitle_question();
        let strat = test_events::strategy_event();
        state.append_event(q.clone());
        state.append_event(strat.clone());

        let (mut ws, _) = connect_async(&ws_url(&base, &state.pairing_token()))
            .await
            .expect("upgrade accepted");
        ws.send(ClientFrame::Text(r#"{"t":"resume","sinceSeq":1}"#.into()))
            .await
            .expect("send resume");
        let reply = read_event(&mut ws).await.expect("resume reply");
        assert_eq!(reply, ServerEvent::Timeline { events: vec![strat] });
    }

    #[tokio::test]
    async fn unknown_json_fields_drop_the_connection() {
        let (base, state) = spawn_server().await;
        let (mut ws, _) = connect_async(&ws_url(&base, &state.pairing_token()))
            .await
            .expect("upgrade accepted");
        ws.send(ClientFrame::Text(
            r#"{"t":"control","language":"bilingual","unknownField":1}"#.into(),
        ))
        .await
        .expect("send malformed control");

        let next = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("server should drop the connection within 5s");
        match next {
            Some(Ok(ClientFrame::Close(_))) => {}
            Some(Err(_)) => {}
            None => {}
            other => panic!("expected connection drop on unknown fields, got {other:?}"),
        }
    }

    #[test]
    fn client_message_rejects_unknown_fields() {
        let bad = r#"{"t":"control","language":"bilingual","extra":true}"#;
        assert!(serde_json::from_str::<ClientMessage>(bad).is_err());
    }

    #[test]
    fn wire_shapes_match_protocol_package() {
        // Exact shapes packages/protocol emits on the TS side; the Task-3 e2e
        // feeds the same strings from the mock WS server.
        let subtitle: ServerEvent = serde_json::from_str(
            r#"{"t":"subtitle","id":"r1-q","speaker":"interviewer","seq":1,"zh":"问题","en":"question","final":true}"#,
        )
        .expect("subtitle wire shape");
        assert!(matches!(subtitle, ServerEvent::Subtitle { seq: 1, .. }));

        let language: ServerEvent =
            serde_json::from_str(r#"{"t":"language","language":"all-zh"}"#)
                .expect("language wire shape");
        assert!(matches!(
            language,
            ServerEvent::Language {
                language: LanguagePref::AllZh
            }
        ));

        let status: ServerEvent =
            serde_json::from_str(r#"{"t":"status","session":"generating"}"#)
                .expect("status wire shape");
        assert!(matches!(
            status,
            ServerEvent::Status {
                session: SessionStatus::Generating
            }
        ));

        let timeline: ServerEvent = serde_json::from_str(r#"{"t":"timeline","events":[]}"#)
            .expect("timeline wire shape");
        assert!(matches!(timeline, ServerEvent::Timeline { .. }));

        // A subtitle without `en` must NOT serialize a null key — the TS
        // narrowing rejects null; absence expresses "unknown yet".
        let no_en = ServerEvent::Subtitle {
            id: "r1-a".into(),
            speaker: Speaker::User,
            seq: 2,
            zh: Some("答案".into()),
            en: None,
            final_flag: true,
        };
        let json = serde_json::to_value(&no_en).expect("serialize subtitle");
        assert!(json.get("en").is_none(), "absent en must not serialize as null");

        // H5 control wire parses back into the enum mirror.
        let control: ClientMessage =
            serde_json::from_str(r#"{"t":"control","language":"bilingual"}"#)
                .expect("control wire shape");
        assert_eq!(
            control,
            ClientMessage::Control {
                language: LanguagePref::Bilingual
            }
        );
    }
}
