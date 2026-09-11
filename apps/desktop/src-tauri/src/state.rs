//! Session state shared by the Tauri commands, the LAN server and the
//! simulation source (01-02 walking skeleton, extended in 01-05).
//!
//! `SessionState` is cheaply clonable (an `Arc<RwLock<Inner>>` plus a tokio
//! broadcast channel) so Tauri can manage it, axum can use it as `State`, and
//! the SimSource scheduler can append events to it. Appending to the timeline
//! always broadcasts the event, so desktop (Tauri `session` emit) and H5 (WS)
//! observe the same timeline — ONE event model, two transports.
//!
//! Session lifecycle (01-05): `start_session` swaps in a fresh engine, bumps the
//! session epoch and announces `listening`; a scheduler holding an older epoch
//! exits on its next tick, which is how stop/restart tears a session down
//! without any cancellation plumbing.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use serde_json::json;
use tauri::Emitter;
use tokio::sync::broadcast;

use crate::lan::server::{LanguagePref, ServerEvent, SessionStatus};
use crate::sim::source::SimSource;

/// Pairing token entropy: 16 bytes = 128 bits → 32 hex chars (T-01-01).
const TOKEN_BYTES: usize = 16;

/// Cheaply clonable, Tauri-manageable session state.
#[derive(Clone)]
pub struct SessionState {
    inner: Arc<RwLock<SessionStateInner>>,
    broadcast_tx: broadcast::Sender<ServerEvent>,
    /// WS clients currently attached. Source of the console `phone_count`
    /// telemetry (desktop-only — it is not a ServerEvent).
    connected_clients: Arc<AtomicUsize>,
    /// Bumped on every session start/stop; a scheduler whose epoch no longer
    /// matches exits on its next tick.
    session_epoch: Arc<AtomicU64>,
}

struct SessionStateInner {
    /// 32 hex chars; gates the LAN WS upgrade (pairing-as-auth).
    pairing_token: String,
    /// Ordered event log appended by the SimSource; replayed on resume.
    timeline: Vec<ServerEvent>,
    language_prefs: LanguagePref,
    session_status: SessionStatus,
    port: u16,
    /// The deterministic sim engine. The `Mutex` lives here so the scheduler
    /// and the 打断/重听 commands serialize on it; it is never locked while
    /// holding the state lock (lock order: engine → state).
    sim: Arc<Mutex<SimSource>>,
    /// Tauri handle used to mirror events onto the desktop webviews. `None` in
    /// cargo tests, where emission is skipped but state still changes.
    app_handle: Option<tauri::AppHandle>,
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
                sim: Arc::new(Mutex::new(SimSource::new())),
                app_handle: None,
            })),
            broadcast_tx,
            connected_clients: Arc::new(AtomicUsize::new(0)),
            session_epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Stores the Tauri app handle (called once from `setup`); every emit path
    /// becomes live after this.
    pub fn set_app_handle(&self, app: tauri::AppHandle) {
        self.inner.write().expect("state lock poisoned").app_handle = Some(app);
    }

    /// Mirrors an event onto the desktop webviews. A session with no app handle
    /// (cargo tests) or no window is not an error — the timeline stays the
    /// source of truth.
    fn emit_to_webviews(&self, event: &str, payload: serde_json::Value) {
        let handle = self
            .inner
            .read()
            .expect("state lock poisoned")
            .app_handle
            .clone();
        if let Some(app) = handle {
            let _ = app.emit(event, payload);
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

    /// Read by the H5 copilot language controls.
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

    /// Publishes one event to every surface: the timeline (resume replay), the
    /// WS broadcast (phone) and the Tauri `session` emit (desktop webviews).
    pub fn publish(&self, event: ServerEvent) {
        let Ok(payload) = serde_json::to_value(&event) else {
            return;
        };
        self.append_event(event);
        self.emit_to_webviews("session", payload);
    }

    /// Sets the status and mirrors it onto `session_status` (drives the console
    /// status dot and the dual pane's pill) without touching the timeline.
    fn announce_status(&self, status: SessionStatus) {
        self.set_session_status(status);
        self.emit_to_webviews("session_status", json!({ "session": status }));
    }

    /// Announces a status AND records it as a `status` ServerEvent, so a phone
    /// that reconnects later replays the session state it missed.
    pub fn publish_status(&self, status: SessionStatus) {
        self.announce_status(status);
        self.publish(ServerEvent::Status { session: status });
    }

    /// Current session epoch (see the field docs).
    pub fn session_epoch(&self) -> u64 {
        self.session_epoch.load(Ordering::SeqCst)
    }

    /// Starts a fresh simulated session and returns the epoch the caller must
    /// hand to the scheduler. Rejects a session that is already running.
    pub fn start_session(&self) -> Result<u64, String> {
        let current = self.session_status();
        if current == SessionStatus::Listening || current == SessionStatus::Generating {
            return Err("a simulated session is already running".into());
        }
        // Bump the epoch first: a scheduler still running from the previous
        // session stops on its next tick instead of appending into the new
        // timeline.
        let epoch = self.session_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        self.replace_sim();
        self.reset_timeline();
        self.announce_status(SessionStatus::Listening);
        Ok(epoch)
    }

    /// Ends the session: cancels any running scheduler and publishes the
    /// terminal status (timeline + WS + both Tauri channels). The timeline is
    /// kept for review.
    pub fn stop_session(&self) {
        self.session_epoch.fetch_add(1, Ordering::SeqCst);
        self.publish_status(SessionStatus::Ended);
    }

    /// Advances the engine to `elapsed_ms` and publishes whatever matured — the
    /// scheduler tick body. `None` when `epoch` has been superseded (the caller
    /// must then stop: a newer session owns the state).
    pub fn advance_sim_for(&self, epoch: u64, elapsed_ms: u64) -> Option<Vec<ServerEvent>> {
        let engine = self.sim_handle();
        let mut sim = engine.lock().expect("sim lock poisoned");
        // Re-check under the engine lock: a stop/restart between the tick and
        // here must not land events in the new session's timeline.
        if self.session_epoch() != epoch {
            return None;
        }
        let events = sim.poll(elapsed_ms);
        self.publish_all(&events);
        Some(events)
    }

    /// [`Self::advance_sim_for`] against the current epoch (tests, manual step).
    pub fn advance_sim(&self, elapsed_ms: u64) -> Vec<ServerEvent> {
        let epoch = self.session_epoch();
        self.advance_sim_for(epoch, elapsed_ms).unwrap_or_default()
    }

    /// 打断 (D-03): cut the current answer and open the next round one second
    /// later. Only meaningful while the answer is generating.
    pub fn interrupt_session(&self) -> Result<(), String> {
        if self.session_status() != SessionStatus::Generating {
            return Err("打断 only applies while the answer is generating".into());
        }
        let engine = self.sim_handle();
        let mut sim = engine.lock().expect("sim lock poisoned");
        let events = sim.interrupt();
        self.publish_all(&events);
        Ok(())
    }

    /// 重听 (D-03): replay the current round with fresh sequence numbers. Only
    /// meaningful while the answer is generating.
    pub fn repeat_session(&self) -> Result<(), String> {
        if self.session_status() != SessionStatus::Generating {
            return Err("重听 only applies while the answer is generating".into());
        }
        let engine = self.sim_handle();
        let mut sim = engine.lock().expect("sim lock poisoned");
        let events = sim.repeat();
        self.publish_all(&events);
        Ok(())
    }

    /// WS client connected: increments the counter and emits `phone_count`.
    pub fn client_connected(&self) -> usize {
        let count = self.connected_clients.fetch_add(1, Ordering::SeqCst) + 1;
        self.emit_to_webviews("phone_count", json!({ "count": count }));
        count
    }

    /// WS client gone: decrements (never below zero) and emits `phone_count`.
    pub fn client_disconnected(&self) -> usize {
        let count = self
            .connected_clients
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                Some(current.saturating_sub(1))
            })
            .unwrap_or(0)
            .saturating_sub(1);
        self.emit_to_webviews("phone_count", json!({ "count": count }));
        count
    }

    /// Live WS client count (asserted by the integration test).
    pub fn connected_clients(&self) -> usize {
        self.connected_clients.load(Ordering::SeqCst)
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

    /// The engine handle (cloned out of the lock, then locked by the caller).
    fn sim_handle(&self) -> Arc<Mutex<SimSource>> {
        self.inner.read().expect("state lock poisoned").sim.clone()
    }

    /// Swaps in a fresh engine (new session). Never locks the engine, so it
    /// cannot deadlock against a tick already holding it.
    fn replace_sim(&self) {
        let mut inner = self.inner.write().expect("state lock poisoned");
        inner.sim = Arc::new(Mutex::new(SimSource::new()));
    }

    /// Publishes a whole batch: statuses go through [`Self::publish_status`] so
    /// the session status stays in step with the timeline.
    fn publish_all(&self, events: &[ServerEvent]) {
        for event in events {
            match event {
                ServerEvent::Status { session } => self.publish_status(*session),
                other => self.publish(other.clone()),
            }
        }
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
        assert_eq!(state.connected_clients(), 0);
        assert_eq!(state.session_epoch(), 0);
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

    #[test]
    fn session_lifecycle_start_stop_and_restart() {
        let state = SessionState::new(8787);
        let first = state.start_session().expect("first start");
        assert_eq!(state.session_status(), SessionStatus::Listening);
        assert_eq!(state.session_epoch(), first);

        // A second start while running is refused (the console CTA depends on it).
        assert!(state.start_session().is_err());

        state.stop_session();
        assert_eq!(state.session_status(), SessionStatus::Ended);
        assert!(state.session_epoch() > first, "stop cancels the scheduler");
        assert_eq!(
            state.timeline(),
            vec![ServerEvent::Status {
                session: SessionStatus::Ended
            }],
            "stop records the terminal status for a late phone resume"
        );

        // Restarting after stop is allowed and clears the previous timeline.
        let third = state.start_session().expect("restart after stop");
        assert!(third > first);
        assert!(state.timeline().is_empty());
    }

    #[test]
    fn interrupt_and_repeat_require_a_generating_phase() {
        let state = SessionState::new(8787);
        state.start_session().expect("start");
        assert!(state.interrupt_session().is_err());
        assert!(state.repeat_session().is_err());

        // Drive the engine into the generating phase of round 1.
        state.advance_sim(crate::sim::script::ROUNDS[0].timing.generating_at_ms);
        assert_eq!(state.session_status(), SessionStatus::Generating);
        assert!(state.interrupt_session().is_ok());
        assert_eq!(state.session_status(), SessionStatus::Listening);
    }

    #[test]
    fn connected_client_counter_never_goes_negative() {
        let state = SessionState::new(8787);
        assert_eq!(state.client_connected(), 1);
        assert_eq!(state.client_connected(), 2);
        assert_eq!(state.connected_clients(), 2);
        assert_eq!(state.client_disconnected(), 1);
        assert_eq!(state.client_disconnected(), 0);
        assert_eq!(
            state.client_disconnected(),
            0,
            "a stray close must not wrap"
        );
    }
}
