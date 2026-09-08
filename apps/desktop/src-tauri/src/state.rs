//! Session state shared by the Tauri commands, the LAN server and the
//! simulation source (01-02 walking skeleton).
//!
//! `SessionState` is cheaply clonable (an `Arc<RwLock<Inner>>` plus a tokio
//! broadcast channel) so Tauri can manage it, axum can use it as `State`, and
//! the SimSource scheduler can append events to it. Appending to the timeline
//! always broadcasts the event, so desktop (Tauri `session` emit) and H5 (WS)
//! observe the same timeline.

use std::sync::{Arc, RwLock};

use tokio::sync::broadcast;

use crate::lan::server::{LanguagePref, ServerEvent, SessionStatus};

/// Pairing token entropy: 16 bytes = 128 bits → 32 hex chars (T-01-01).
const TOKEN_BYTES: usize = 16;

/// Cheaply clonable, Tauri-manageable session state.
#[derive(Clone)]
pub struct SessionState {
    inner: Arc<RwLock<SessionStateInner>>,
    broadcast_tx: broadcast::Sender<ServerEvent>,
}

struct SessionStateInner {
    /// 32 hex chars; gates the LAN WS upgrade (pairing-as-auth).
    pairing_token: String,
    /// Ordered event log appended by the SimSource; replayed on resume.
    timeline: Vec<ServerEvent>,
    language_prefs: LanguagePref,
    session_status: SessionStatus,
    port: u16,
}

impl SessionState {
    /// Fresh state with a new random pairing token (128-bit, 32 hex chars).
    pub fn new(port: u16) -> Self {
        use rand::rngs::SysRng;
        use rand::TryRng;

        let mut bytes = [0u8; TOKEN_BYTES];
        SysRng
            .try_fill_bytes(&mut bytes)
            .expect("OS entropy unavailable for pairing token");
        let pairing_token = bytes.iter().map(|b| format!("{b:02x}")).collect();

        let (broadcast_tx, _) = broadcast::channel(64);
        Self {
            inner: Arc::new(RwLock::new(SessionStateInner {
                pairing_token,
                timeline: Vec::new(),
                language_prefs: LanguagePref::Bilingual,
                session_status: SessionStatus::Idle,
                port,
            })),
            broadcast_tx,
        }
    }

    /// The 32-hex-char pairing token (T-01-01).
    pub fn pairing_token(&self) -> String {
        self.inner
            .read()
            .expect("state lock poisoned")
            .pairing_token
            .clone()
    }

    /// LAN port the H5 connects to.
    pub fn port(&self) -> u16 {
        self.inner.read().expect("state lock poisoned").port
    }

    /// QR-pairing URL: `http://{lan_ip}:{port}/?token={token}`.
    pub fn pairing_url(&self) -> String {
        let ip = local_ip_address::local_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| "127.0.0.1".to_string());
        format!(
            "http://{ip}:{}/?token={}",
            self.port(),
            self.pairing_token()
        )
    }

    pub fn session_status(&self) -> SessionStatus {
        self.inner
            .read()
            .expect("state lock poisoned")
            .session_status
    }

    /// Sets the status; returns the previous one (guards double-start).
    pub fn set_session_status(&self, status: SessionStatus) -> SessionStatus {
        let mut inner = self.inner.write().expect("state lock poisoned");
        let previous = inner.session_status;
        inner.session_status = status;
        previous
    }

    /// Read by the H5 copilot language controls landing in 01-05.
    #[allow(dead_code)]
    pub fn language_prefs(&self) -> LanguagePref {
        self.inner
            .read()
            .expect("state lock poisoned")
            .language_prefs
    }

    pub fn set_language_prefs(&self, pref: LanguagePref) {
        self.inner
            .write()
            .expect("state lock poisoned")
            .language_prefs = pref;
    }

    /// Snapshot of the full event timeline.
    pub fn timeline(&self) -> Vec<ServerEvent> {
        self.inner
            .read()
            .expect("state lock poisoned")
            .timeline
            .clone()
    }

    /// Clears the timeline (new session).
    pub fn reset_timeline(&self) {
        self.inner
            .write()
            .expect("state lock poisoned")
            .timeline
            .clear();
    }

    /// Appends an event to the timeline and broadcasts it to every subscriber
    /// (LAN clients, Tauri-side forwards). Broadcast errors are expected when
    /// no client is connected and are intentionally ignored — the timeline
    /// remains the source of truth for later replays.
    pub fn append_event(&self, event: ServerEvent) {
        self.inner
            .write()
            .expect("state lock poisoned")
            .timeline
            .push(event.clone());
        let _ = self.broadcast_tx.send(event);
    }

    /// Subscribes to the event broadcast stream.
    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.broadcast_tx.subscribe()
    }

    /// Events after the last subtitle with `seq <= since_seq`; `since_seq == 0`
    /// replays the whole timeline (fresh-client resume).
    pub fn replay_after_subtitle_seq(&self, since_seq: u64) -> Vec<ServerEvent> {
        let timeline = self.timeline();
        if since_seq == 0 {
            return timeline;
        }
        let mut start = 0usize;
        for (i, event) in timeline.iter().enumerate() {
            if let ServerEvent::Subtitle { seq, .. } = event {
                if *seq <= since_seq {
                    start = i + 1;
                }
            }
        }
        timeline[start..].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lan::server::test_events::{self, strategy_event, subtitle_question, user_answer};

    #[test]
    fn pairing_token_is_32_hex_chars() {
        let state = SessionState::new(8787);
        let token = state.pairing_token();
        assert_eq!(token.len(), 32, "token must be 32 hex chars");
        assert!(
            token.chars().all(|c| c.is_ascii_hexdigit()),
            "token must be hex"
        );
    }

    #[test]
    fn two_states_generate_different_tokens() {
        let a = SessionState::new(8787).pairing_token();
        let b = SessionState::new(8787).pairing_token();
        assert_ne!(a, b, "each SessionState must draw fresh entropy");
    }

    #[test]
    fn defaults_are_idle_bilingual() {
        let state = SessionState::new(8787);
        assert_eq!(state.session_status(), SessionStatus::Idle);
        assert_eq!(state.language_prefs(), LanguagePref::Bilingual);
        assert!(state.timeline().is_empty());
    }

    #[test]
    fn append_event_extends_timeline_and_broadcasts() {
        let state = SessionState::new(8787);
        let mut rx = state.subscribe();
        let ev = subtitle_question();
        state.append_event(ev.clone());
        assert_eq!(state.timeline(), vec![ev.clone()]);
        assert_eq!(rx.blocking_recv().expect("broadcast received"), ev);
    }

    #[test]
    fn set_session_status_returns_previous() {
        let state = SessionState::new(8787);
        assert_eq!(
            state.set_session_status(SessionStatus::Listening),
            SessionStatus::Idle
        );
        assert_eq!(state.session_status(), SessionStatus::Listening);
        assert_eq!(
            state.set_session_status(SessionStatus::Ended),
            SessionStatus::Listening
        );
    }

    #[test]
    fn replay_after_subtitle_seq_returns_only_new_events() {
        let state = SessionState::new(8787);
        let q = subtitle_question(); // seq 1
        let strat = strategy_event();
        let answer = user_answer(); // seq 2
        state.append_event(q.clone());
        state.append_event(strat.clone());
        state.append_event(answer.clone());

        assert_eq!(
            state.replay_after_subtitle_seq(0),
            vec![q, strat.clone(), answer.clone()]
        );
        assert_eq!(
            state.replay_after_subtitle_seq(1),
            vec![strat.clone(), answer.clone()]
        );
        assert_eq!(state.replay_after_subtitle_seq(2), vec![]);
    }

    #[test]
    fn reset_timeline_clears_history_for_new_session() {
        let state = SessionState::new(8787);
        state.append_event(subtitle_question());
        state.append_event(test_events::strategy_event());
        assert_eq!(state.timeline().len(), 2);
        state.reset_timeline();
        assert!(state.timeline().is_empty());
    }
}
