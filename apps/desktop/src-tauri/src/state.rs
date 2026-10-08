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

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use serde_json::json;
use tauri::Emitter;
use tokio::sync::broadcast;

use crate::audio::playout::PlayoutQueue;
use crate::lan::server::{LanguagePref, ServerEvent, SessionStatus};
use crate::sim::source::SimSource;
use crate::trace::jsonl::{TraceRecord, TraceWriter, TraceWriterConfig};
use crate::trace::UsageSummary;

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
    /// The synthesized-voice playout buffer (02-03 T3.3). Deliberately *not*
    /// the session epoch: a barge-in moves the playout generation without
    /// cancelling the running driver, so the two counters move independently.
    /// Every clone shares one buffer (the queue is internally `Arc`-ed).
    playout: PlayoutQueue,
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
    /// The `traces` root (set once from `setup`); tracing is off until then,
    /// so cargo tests and headless runs write nothing.
    trace_dir: Option<PathBuf>,
    /// The live session's single trace writer (T3.7). `None` between sessions
    /// and whenever [`Self::trace_dir`] is unset.
    trace_writer: Option<TraceWriter>,
    /// What the last closed writer reported (WR-03). The writer is dropped at
    /// 停止, so its live atomics go with it; this snapshot is what keeps
    /// 「丢了多少条」 readable after the fact.
    trace_health: TraceHealth,
}

/// The trace writer's loss counters, snapshotted when it closes (WR-03).
///
/// A counter with no reader is a claim the code does not keep: these two ride
/// the diagnostics payload so a dropped or failed record is visible instead of
/// inferred from an empty file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TraceHealth {
    pub dropped_records: u64,
    pub write_failures: u64,
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
                trace_dir: None,
                trace_writer: None,
                trace_health: TraceHealth::default(),
            })),
            broadcast_tx,
            connected_clients: Arc::new(AtomicUsize::new(0)),
            session_epoch: Arc::new(AtomicU64::new(0)),
            playout: PlayoutQueue::new(),
        }
    }

    /// The synthesized-voice playout queue (02-03 T3.3). The cascade plays into
    /// it and 02-05 reads from it; its epoch is independent of the session
    /// epoch (a barge-in must not cancel the session's driver).
    pub fn playout(&self) -> &PlayoutQueue {
        &self.playout
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
    /// (LAN clients, Tauri-side forwards). A closed sentence is simultaneously
    /// handed to the session's trace writer (T3.7) — the timeline and the
    /// JSONL file are one model, so there is no second instrumentation path
    /// (D-18/GOV-10). Broadcast errors are expected when no client is
    /// connected and are intentionally ignored — the timeline remains the
    /// source of truth for later replays.
    pub fn append_event(&self, event: ServerEvent) {
        let writer = {
            let mut inner = self.inner.write().expect("state lock poisoned");
            inner.timeline.push(event.clone());
            inner.trace_writer.clone()
        };
        if let Some(writer) = writer {
            if let Some(record) = TraceRecord::from_event(
                writer.session_id(),
                crate::trace::unix_millis_now(),
                &event,
            ) {
                writer.append(record);
            }
        }
        let _ = self.broadcast_tx.send(event);
    }

    /// Sets the trace root — called once from `setup` with the app data dir.
    /// Until it runs no file is written: tracing is supplementary, never a
    /// precondition for a session.
    pub fn set_trace_dir(&self, dir: impl Into<PathBuf>) {
        self.inner.write().expect("state lock poisoned").trace_dir = Some(dir.into());
    }

    /// The live session's trace writer (diagnostics and tests). `None` outside
    /// a session or when tracing is unconfigured.
    pub fn trace_writer(&self) -> Option<TraceWriter> {
        self.inner
            .read()
            .expect("state lock poisoned")
            .trace_writer
            .clone()
    }

    /// The trace writer's loss counters as the diagnostics panel reads them
    /// (WR-03): live while a session runs, the last snapshot after it stops —
    /// so 「丢了多少条」 stays readable once the writer is gone.
    pub fn trace_health(&self) -> TraceHealth {
        let inner = self.inner.read().expect("state lock poisoned");
        match &inner.trace_writer {
            Some(writer) => TraceHealth {
                dropped_records: writer.dropped_records(),
                write_failures: writer.write_failures(),
            },
            None => inner.trace_health,
        }
    }

    /// The current month's usage, read from the JSONL traces on demand
    /// (T3.7/D-13) — there is no counter that could drift from the files.
    pub fn usage_summary(&self) -> UsageSummary {
        let dir = self
            .inner
            .read()
            .expect("state lock poisoned")
            .trace_dir
            .clone();
        match dir {
            Some(dir) => {
                UsageSummary::load(&dir, crate::trace::unix_millis_now()).unwrap_or_else(|err| {
                    eprintln!("usage summary unavailable: {err}");
                    UsageSummary::default()
                })
            }
            None => UsageSummary::default(),
        }
    }

    /// Opens the session's writer; a disk failure degrades to "no trace file"
    /// — the session itself never fails because tracing did.
    fn open_trace_writer(&self, epoch: u64) {
        let dir = self
            .inner
            .read()
            .expect("state lock poisoned")
            .trace_dir
            .clone();
        let Some(dir) = dir else {
            return;
        };
        let start_time_ms = crate::trace::unix_millis_now();
        let config = TraceWriterConfig::new(
            dir,
            format!("session-{epoch}-{start_time_ms}"),
            start_time_ms,
        );
        match TraceWriter::open(config) {
            Ok(writer) => {
                let mut inner = self.inner.write().expect("state lock poisoned");
                inner.trace_writer = Some(writer);
                // A fresh session starts a fresh scoreboard (WR-03): the panel's
                // numbers describe this session, never the previous one.
                inner.trace_health = TraceHealth::default();
            }
            Err(err) => eprintln!("trace writer unavailable, continuing without: {err}"),
        }
    }

    /// Closes the session's trace: the writer drops, its queued records drain
    /// into the file and the task exits (T3.7).
    ///
    /// The drop would take the counters with it, so they are snapshotted first
    /// (WR-03) — a session that lost records must still be able to say so.
    fn close_trace_writer(&self) {
        let mut inner = self.inner.write().expect("state lock poisoned");
        if let Some(writer) = inner.trace_writer.take() {
            inner.trace_health = TraceHealth {
                dropped_records: writer.dropped_records(),
                write_failures: writer.write_failures(),
            };
        }
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
        // The new session owns a fresh playout generation: whatever the last
        // session left buffered is refused from here on, and any chunk still
        // in flight cannot land in the new session's audio (T3.3).
        self.playout.begin_session();
        self.replace_sim();
        self.reset_timeline();
        // The durable trace opens with the session (T3.7): everything appended
        // from here on can land in `traces/<date>/<session>.jsonl`.
        self.open_trace_writer(epoch);
        // Session identity goes on the wire before any content: both desktop
        // webviews and every paired phone drop their cursors and rendered
        // events on it, so the restarted session can never be mistaken for a
        // replay of the previous one (CR-01 / WR-02).
        self.publish(ServerEvent::SessionStarted { epoch });
        self.announce_status(SessionStatus::Listening);
        Ok(epoch)
    }

    /// Ends the session: cancels any running scheduler and publishes the
    /// terminal status (timeline + WS + both Tauri channels). The timeline is
    /// kept for review.
    pub fn stop_session(&self) {
        self.session_epoch.fetch_add(1, Ordering::SeqCst);
        // 停止 cuts the voice immediately: the buffer is cleared and the
        // playout generation moves, so an in-flight chunk from the stopped
        // session can never be played after the fact (T3.3).
        self.playout.end_session();
        // The durable trace closes with it: the writer drains what is queued
        // and exits — no line lands after the session ended (T3.7).
        self.close_trace_writer();
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

    /// 打断 (D-03): cut the current round and open the next one second
    /// later. Live through the whole active session (UAT-7).
    pub fn interrupt_session(&self) -> Result<(), String> {
        self.interrupt_session_for(self.session_epoch())
    }

    /// [`Self::interrupt_session`] against `epoch` — the session the caller's
    /// intent belongs to, re-checked *under the engine lock* because a
    /// stop/restart can land between reading it and acquiring the lock (the
    /// same guard `advance_sim_for` uses).
    fn interrupt_session_for(&self, epoch: u64) -> Result<(), String> {
        self.guard_active("打断")?;
        let engine = self.sim_handle();
        let mut sim = engine.lock().expect("sim lock poisoned");
        // Re-check under the engine lock: 停止 / 开始模拟会话 between reading
        // the epoch and getting here must not land a mutation on the engine
        // the new session just swapped in.
        if self.session_epoch() != epoch {
            return Err("the session ended before the command landed".into());
        }
        let events = sim.interrupt();
        self.publish_all(&events);
        Ok(())
    }

    /// 重听 (D-03): replay the current round with fresh sequence numbers. Live
    /// through the whole active session (UAT-7).
    pub fn repeat_session(&self) -> Result<(), String> {
        self.repeat_session_for(self.session_epoch())
    }

    /// [`Self::repeat_session`] against `epoch` (see `interrupt_session_for`).
    fn repeat_session_for(&self, epoch: u64) -> Result<(), String> {
        self.guard_active("重听")?;
        let engine = self.sim_handle();
        let mut sim = engine.lock().expect("sim lock poisoned");
        if self.session_epoch() != epoch {
            return Err("the session ended before the command landed".into());
        }
        let events = sim.repeat();
        self.publish_all(&events);
        Ok(())
    }

    /// The status half of the 打断/重听 guard.
    ///
    /// UAT-7: the controls stay live through the whole active session —
    /// gating them to the brief generating window made them invisible in the
    /// demo (disabled most of the time). 打断 also makes sense while the
    /// question plays: cut straight to the next round.
    fn guard_active(&self, command: &str) -> Result<(), String> {
        let status = self.session_status();
        if status != SessionStatus::Listening && status != SessionStatus::Generating {
            return Err(format!("{command} only applies while a session is live"));
        }
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
    ///
    /// A cursor above every subtitle this timeline holds can only come from a
    /// previous session (subtitle `seq` restarts at 1), so it replays the whole
    /// timeline — marker included — instead of slicing the new session away.
    pub fn replay_after_subtitle_seq(&self, since_seq: u64) -> Vec<ServerEvent> {
        let timeline = self.timeline();
        if since_seq == 0 {
            return timeline;
        }
        let mut start = 0usize;
        let mut highest_seq = 0u64;
        for (i, event) in timeline.iter().enumerate() {
            if let ServerEvent::Subtitle { seq, .. } = event {
                highest_seq = highest_seq.max(*seq);
                if *seq <= since_seq {
                    start = i + 1;
                }
            }
        }
        if since_seq > highest_seq {
            return timeline;
        }
        timeline[start..].to_vec()
    }

    /// The resume reply for a reconnecting client.
    ///
    /// `since_epoch` is the session the client believes it is in (`None` for a
    /// client that does not track one). When it no longer matches, the whole
    /// timeline is replayed: the session restarted, its subtitle numbering
    /// restarted with it, and no `seq` cursor can express that. Otherwise the
    /// ordinary seq tail is returned.
    pub fn resume_events(&self, since_seq: u64, since_epoch: Option<u64>) -> Vec<ServerEvent> {
        if let Some(epoch) = since_epoch {
            if epoch != self.session_epoch() {
                return self.timeline();
            }
        }
        self.replay_after_subtitle_seq(since_seq)
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
    use crate::lan::server::{ConfidenceLevel, ConfidenceSource, Speaker, SubtitleTrace};
    use crate::trace::jsonl::{SegmentStatus, TraceRecord};

    /// The session-identity marker exactly as it lands on the wire.
    fn session_started(epoch: u64) -> ServerEvent {
        serde_json::from_str(&format!(r#"{{"t":"session_started","epoch":{epoch}}}"#))
            .expect("session_started wire shape")
    }

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
    fn start_session_publishes_the_session_marker_first() {
        let state = SessionState::new(8787);
        let epoch = state.start_session().expect("start");
        assert_eq!(
            state.timeline().first(),
            Some(&session_started(epoch)),
            "the new session's identity opens the new timeline (CR-01)"
        );
    }

    /// Subtitle sequence numbers in an event list (the phone's cursor unit).
    fn subtitle_seqs(events: &[ServerEvent]) -> Vec<u64> {
        events
            .iter()
            .filter_map(|event| match event {
                ServerEvent::Subtitle { seq, .. } => Some(*seq),
                _ => None,
            })
            .collect()
    }

    /// Drives one full round so the timeline carries subtitles 1..=2.
    fn play_first_round(state: &SessionState) {
        state.advance_sim(crate::sim::script::ROUNDS[0].timing.generating_at_ms);
    }

    #[test]
    fn a_cursor_above_this_session_replays_it_from_the_start() {
        let state = SessionState::new(8787);
        state.start_session().expect("start");
        play_first_round(&state); // the new session's highest seq is 15

        // A client that does not track the epoch resumes with the previous
        // session's high-water mark (56): the stale cursor must not slice the
        // new session away.
        let replay = state.replay_after_subtitle_seq(56);
        assert_eq!(subtitle_seqs(&replay), (1..=15).collect::<Vec<u64>>());
    }

    #[test]
    fn resume_replays_the_whole_new_session_when_the_epoch_moved() {
        let state = SessionState::new(8787);
        let first = state.start_session().expect("start");
        play_first_round(&state);
        state.stop_session();
        let second = state.start_session().expect("restart");
        play_first_round(&state);

        // The cursor (15) is indistinguishable from a fresh one — the epoch is
        // what tells the server this client sat out the restart entirely.
        let replay = state.resume_events(15, Some(first));
        assert_eq!(
            replay.first(),
            Some(&session_started(second)),
            "the replay crosses the restart: marker first"
        );
        assert_eq!(subtitle_seqs(&replay), (1..=15).collect::<Vec<u64>>());
    }

    #[test]
    fn resume_inside_one_session_still_replays_only_the_tail() {
        let state = SessionState::new(8787);
        let epoch = state.start_session().expect("start");
        play_first_round(&state);

        let replay = state.resume_events(1, Some(epoch));
        assert_eq!(
            subtitle_seqs(&replay),
            (2..=15).collect::<Vec<u64>>(),
            "a current-epoch cursor keeps the ordinary tail semantics"
        );
        // Epoch-less clients (older H5) fall back to the seq cursor.
        assert_eq!(
            subtitle_seqs(&state.resume_events(15, None)),
            Vec::<u64>::new()
        );
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
            vec![
                session_started(first),
                ServerEvent::Status {
                    session: SessionStatus::Ended
                }
            ],
            "stop records the terminal status for a late phone resume"
        );

        // Restarting after stop is allowed and clears the previous timeline,
        // which the new session's marker immediately opens (CR-01).
        let third = state.start_session().expect("restart after stop");
        assert!(third > first);
        assert_eq!(state.timeline(), vec![session_started(third)]);
    }

    #[test]
    fn interrupt_and_repeat_apply_through_the_active_session() {
        let state = SessionState::new(8787);
        // No live session: nothing to interrupt or replay.
        assert!(state.interrupt_session().is_err());
        assert!(state.repeat_session().is_err());

        state.start_session().expect("start");
        assert_eq!(state.session_status(), SessionStatus::Listening);
        // UAT-7: the controls are live from the question phase on, not only
        // while the answer generates — 打断 cuts to the next round right away.
        assert!(state.interrupt_session().is_ok());
        assert_eq!(state.session_status(), SessionStatus::Listening);

        // Drive the engine into the generating phase of round 2: both controls
        // stay live there, and 重听 replays the current round with fresh ids.
        state.advance_sim(
            crate::sim::source::INTERRUPT_LEAD_MS
                + crate::sim::script::ROUNDS[1].timing.generating_at_ms,
        );
        assert_eq!(state.session_status(), SessionStatus::Generating);
        assert!(state.repeat_session().is_ok());
        assert!(state.interrupt_session().is_ok());
    }

    #[test]
    fn a_stale_interrupt_or_repeat_never_mutates_the_new_session() {
        let state = SessionState::new(8787);
        let epoch = state.start_session().expect("start");
        state.advance_sim(crate::sim::script::ROUNDS[0].timing.generating_at_ms);
        assert_eq!(state.session_status(), SessionStatus::Generating);
        let before = state.timeline();

        // 停止 / 开始模拟会话 land after a 打断 / 重听 read the epoch but
        // before it reaches the engine — the command belongs to the *old*
        // session and only the re-check under the engine lock can refuse it
        // (WR-04; the public entry points capture the epoch themselves, so
        // the seam is what makes the window reachable here).
        state.session_epoch.fetch_add(1, Ordering::SeqCst);
        assert_ne!(state.session_epoch(), epoch);

        assert!(
            state.interrupt_session_for(epoch).is_err(),
            "a stale 打断 must not jump the engine the new session owns"
        );
        assert!(
            state.repeat_session_for(epoch).is_err(),
            "a stale 重听 must not replay a round into the new session"
        );
        assert_eq!(
            state.session_status(),
            SessionStatus::Generating,
            "a refused command must not publish"
        );
        assert_eq!(
            state.timeline(),
            before,
            "a refused command must not append replay events"
        );
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

    /// One user sentence with the per-segment provenance attached — what the
    /// cascade leaves behind on every real segment (T3.7).
    fn user_sentence(final_flag: bool) -> ServerEvent {
        ServerEvent::Subtitle {
            id: "u1".into(),
            speaker: Speaker::User,
            seq: 1,
            zh: Some("这句话进了轨迹".into()),
            en: Some("This sentence reached the trace.".into()),
            final_flag,
            confidence: Some(ConfidenceLevel::Low),
            trace: Some(SubtitleTrace {
                segment_start_ms: 300,
                term_hits: vec![],
                provider: "volc".into(),
                model_version: "icl-2.0".into(),
                confidence_source: ConfidenceSource::Proxy,
                error_code: None,
            }),
        }
    }

    /// Every line under `root/<date>/*.jsonl` (the session's durable trace).
    fn trace_lines(root: &std::path::Path) -> Vec<String> {
        let mut lines = Vec::new();
        for date in std::fs::read_dir(root)
            .expect("trace root exists")
            .flatten()
        {
            for file in std::fs::read_dir(date.path()).expect("date dir").flatten() {
                let path = file.path();
                if path.extension().is_some_and(|ext| ext == "jsonl") {
                    lines.extend(
                        std::fs::read_to_string(path)
                            .expect("trace file readable")
                            .lines()
                            .map(str::to_string),
                    );
                }
            }
        }
        lines
    }

    /// T3.7 (D-05/D-18): the timeline and the durable trace are one model —
    /// every closed sentence lands as exactly one JSONL line, bridged in
    /// `append_event` so no second instrumentation path exists (GOV-10).
    #[tokio::test]
    async fn sentence_events_reach_the_trace_writer() {
        let root = std::env::temp_dir().join(format!("nextalk-state-trace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let state = SessionState::new(8787);
        state.set_trace_dir(root.clone());
        state.start_session().expect("start");
        // Non-sentence noise and a live frame never create lines.
        state.append_event(ServerEvent::Status {
            session: SessionStatus::Listening,
        });
        state.append_event(user_sentence(false));
        // Two closed sentences = two lines.
        state.append_event(user_sentence(true));
        state.append_event(user_sentence(true));

        let writer = state
            .trace_writer()
            .expect("the live session owns a writer");
        writer.flush().await.expect("flush");
        let lines = trace_lines(&root);
        assert_eq!(lines.len(), 2, "one line per closed sentence");
        let records: Vec<TraceRecord> = lines
            .iter()
            .map(|line| serde_json::from_str(line).expect("no torn line"))
            .collect();
        for record in &records {
            assert_eq!(record.session_id, writer.session_id());
            assert_eq!(record.status, SegmentStatus::Ok);
            assert_eq!(record.provider, "volc");
        }

        // 停止 closes the writer: nothing new lands after the session ends.
        state.stop_session();
        assert!(state.trace_writer().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
