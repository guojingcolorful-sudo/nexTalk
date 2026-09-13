//! Deterministic simulation source (01-02 skeleton, completed in 01-05).
//!
//! Two pieces, one contract — same elapsed time, same events, no I/O:
//!
//! - [`script_state`] is the pure evaluator: given elapsed wall-clock time since
//!   session start it derives the complete event sequence. It is the
//!   determinism reference the tests assert against.
//! - [`SimSource`] is the stateful engine that plays that sequence
//!   incrementally, numbers the subtitles from a session-long counter, and
//!   applies the D-03 controls (打断 / 重听). It never reads a clock: the
//!   elapsed values come from an injectable [`TimeSource`], so the production
//!   scheduler uses the real instant and tests script the values.
//!
//! Events reach the app through `SessionState` (timeline append + broadcast +
//! Tauri emit); the engine only produces them.

use std::time::{Duration, Instant};

use crate::lan::server::{ServerEvent, SessionStatus, Speaker};
use crate::sim::script;
use crate::state::SessionState;

/// Scheduler tick: 100 ms (the cadence the latency research assumes).
pub const TICK_MS: u64 = 100;

/// 打断 lead-in: the next round's listening phase opens this long after the
/// cut, so the interviewer's next question never lands on the same frame as
/// the interruption.
pub const INTERRUPT_LEAD_MS: u64 = 1_000;

/// Injectable clock (research Pattern 3).
pub trait TimeSource: Send + 'static {
    /// Milliseconds since the session started.
    fn elapsed_ms(&self) -> u64;
}

/// Production clock: a monotonic instant captured when the session starts.
pub struct RealClock {
    start: Instant,
}

impl RealClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for RealClock {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeSource for RealClock {
    fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

/// One round's milestones that have matured by `local_ms` (round-relative), in
/// emission order: the listening status and the interviewer's question open the
/// round; the strategy card, the user's answer and the generating status follow
/// at their offsets.
///
/// Subtitle events carry a *round-relative rank* in `seq` (1 = question,
/// 2 = answer) — the callers renumber: [`script_state`] folds the rank into the
/// session-long sequence, [`SimSource`] overwrites it with its own counter.
fn round_events(round_index: usize, local_ms: u64) -> Vec<ServerEvent> {
    let round = &script::ROUNDS[round_index];
    let mut events = Vec::with_capacity(5);

    events.push(ServerEvent::Status {
        session: SessionStatus::Listening,
    });
    // UAT-11: the question opens the round after the lead-in — the session
    // never starts mid-sentence, and the strategy lands only after the
    // question has been fully read aloud.
    if local_ms >= round.timing.question_at_ms {
        events.push(ServerEvent::Subtitle {
            id: script::question_id(round_index),
            speaker: Speaker::Interviewer,
            seq: 1,
            zh: Some(round.interviewer_zh.to_string()),
            en: Some(round.interviewer_en.to_string()),
            final_flag: true,
        });
    }

    if local_ms >= round.timing.strategy_at_ms {
        events.push(ServerEvent::Strategy {
            id: script::strategy_id(round_index),
            round_id: round.id.to_string(),
            title: round.strategy.title.to_string(),
            bullets: round
                .strategy
                .bullets
                .iter()
                .map(|bullet| bullet.to_string())
                .collect(),
            answer_zh: Some(round.strategy.answer_zh.to_string()),
            answer_en: Some(round.strategy.answer_en.to_string()),
        });
    }

    if local_ms >= round.timing.answer_at_ms {
        events.push(ServerEvent::Subtitle {
            id: script::answer_id(round_index),
            speaker: Speaker::User,
            seq: 2,
            zh: Some(round.user_zh.to_string()),
            // The English line is what the cloned voice speaks (D-03): the
            // bubble renders it as the user's translation from 01-05 onwards.
            en: Some(round.user_en.to_string()),
            final_flag: true,
        });
    }

    if local_ms >= round.timing.generating_at_ms {
        events.push(ServerEvent::Status {
            session: SessionStatus::Generating,
        });
    }

    events
}

/// Pure evaluator: every script event whose offset is `<= elapsed_ms`, in
/// emission order, with session-long subtitle sequence numbers (1..=8 for the
/// canonical run). No clock, no I/O — deterministic for a given input.
pub fn script_state(elapsed_ms: u64) -> Vec<ServerEvent> {
    let mut events = Vec::new();
    let mut round_start_ms = 0u64;

    for (index, round) in script::ROUNDS.iter().enumerate() {
        if elapsed_ms < round_start_ms {
            break;
        }
        for mut event in round_events(index, elapsed_ms - round_start_ms) {
            if let ServerEvent::Subtitle { seq, .. } = &mut event {
                *seq += (index as u64) * 2;
            }
            events.push(event);
        }
        round_start_ms += round.timing.end_at_ms;
    }

    if elapsed_ms >= round_start_ms {
        events.push(ServerEvent::Status {
            session: SessionStatus::Ended,
        });
    }
    events
}

/// The stateful engine: plays [`script::ROUNDS`] in order, numbers the
/// subtitles from its own session counter, and applies the D-03 controls.
///
/// Every event it returns is a function of the elapsed values it was polled
/// with — the engine holds no clock and no I/O handle.
#[derive(Debug)]
pub struct SimSource {
    /// Index of the round that plays next (`script::ROUNDS.len()` once ended).
    round: usize,
    /// Session-relative instant the current round started at.
    round_start_ms: u64,
    /// How many of the current round's milestones have been emitted.
    emitted: usize,
    /// Session-long subtitle sequence: monotonic across rounds and replays, so
    /// the phone's `sinceSeq` resume cursor can never skip or duplicate a line.
    next_seq: u64,
    /// The current round's content events (question, strategy, answer), kept
    /// for 重听.
    current_round: Vec<ServerEvent>,
    /// How many times the current round has been replayed (fresh id suffix).
    replays: u64,
    /// Last instant observed by [`SimSource::poll`] — the point 打断 cuts from.
    now_ms: u64,
    ended: bool,
}

impl SimSource {
    pub fn new() -> Self {
        Self {
            round: 0,
            round_start_ms: 0,
            emitted: 0,
            next_seq: 0,
            current_round: Vec::new(),
            replays: 0,
            now_ms: 0,
            ended: false,
        }
    }

    /// Advances the simulated clock to `elapsed_ms` and returns the events that
    /// matured since the previous call, in emission order.
    pub fn poll(&mut self, elapsed_ms: u64) -> Vec<ServerEvent> {
        self.now_ms = elapsed_ms;
        let mut events = Vec::new();

        while !self.ended {
            if elapsed_ms < self.round_start_ms {
                // 打断 scheduled the next round ahead of the current instant.
                break;
            }
            let round = &script::ROUNDS[self.round];
            let local_ms = elapsed_ms - self.round_start_ms;

            let matured = round_events(self.round, local_ms);
            let matured_len = matured.len();
            for event in matured.into_iter().skip(self.emitted) {
                let event = self.number(event);
                self.remember(&event);
                events.push(event);
            }
            self.emitted = matured_len;

            if local_ms < round.timing.end_at_ms {
                break;
            }
            // The round is over: the next one starts at this round's end.
            self.round_start_ms += round.timing.end_at_ms;
            self.round += 1;
            self.emitted = 0;
            self.current_round.clear();
            self.replays = 0;

            if self.round >= script::ROUNDS.len() {
                self.ended = true;
                events.push(ServerEvent::Status {
                    session: SessionStatus::Ended,
                });
            }
        }

        events
    }

    /// 打断 (D-03): drop the rest of the current round and open the next one one
    /// second after the last observed instant. The cut is immediate — nothing
    /// from the interrupted round is emitted again — and the mic re-opens right
    /// away so the pill leaves the generating state without a one-second gap.
    pub fn interrupt(&mut self) -> Vec<ServerEvent> {
        if self.ended {
            return Vec::new();
        }
        self.round += 1;
        self.round_start_ms = self.now_ms.saturating_add(INTERRUPT_LEAD_MS);
        self.emitted = 0;
        self.current_round.clear();
        self.replays = 0;

        if self.round >= script::ROUNDS.len() {
            self.ended = true;
            return vec![ServerEvent::Status {
                session: SessionStatus::Ended,
            }];
        }
        vec![ServerEvent::Status {
            session: SessionStatus::Listening,
        }]
    }

    /// 重听 (D-03): re-emit the current round's content with fresh sequence
    /// numbers and fresh ids, so neither transport treats the replay as a
    /// duplicate (the phone dedupes subtitles by seq and strategies by id).
    ///
    /// Status events are deliberately not replayed: re-firing the round's
    /// listening status mid-answer would drag the UI back out of generating.
    pub fn repeat(&mut self) -> Vec<ServerEvent> {
        if self.ended || self.current_round.is_empty() {
            return Vec::new();
        }
        self.replays += 1;
        let suffix = format!("-r{}", self.replays);
        let mut events = Vec::with_capacity(self.current_round.len());

        for event in self.current_round.clone() {
            let mut event = self.number(event);
            match &mut event {
                ServerEvent::Subtitle { id, .. } | ServerEvent::Strategy { id, .. } => {
                    id.push_str(&suffix);
                }
                _ => {}
            }
            events.push(event);
        }
        events
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    /// Index of the round playing (or opening) next; equals
    /// `script::ROUNDS.len()` once the session has ended.
    pub fn round_index(&self) -> usize {
        self.round
    }

    /// Assigns the next session sequence number to a subtitle; every other
    /// event passes through untouched (only subtitles carry a `seq`).
    fn number(&mut self, mut event: ServerEvent) -> ServerEvent {
        if let ServerEvent::Subtitle { seq, .. } = &mut event {
            self.next_seq += 1;
            *seq = self.next_seq;
        }
        event
    }

    /// Keeps the current round's content for 重听.
    fn remember(&mut self, event: &ServerEvent) {
        if matches!(
            event,
            ServerEvent::Subtitle { .. } | ServerEvent::Strategy { .. }
        ) {
            self.current_round.push(event.clone());
        }
    }
}

impl Default for SimSource {
    fn default() -> Self {
        Self::new()
    }
}

/// The scheduler loop: ticks `clock` every 100 ms and publishes whatever
/// matured. It exits when the script ends, or as soon as a newer session
/// supersedes `epoch` (stop / restart) — a superseded scheduler never appends
/// to the new session's timeline.
///
/// Public because the tests drive it on their own runtime with a scripted
/// clock; production reaches it through [`spawn_scheduler`].
pub async fn run_loop(state: SessionState, epoch: u64, clock: impl TimeSource) {
    let mut tick = tokio::time::interval(Duration::from_millis(TICK_MS));
    loop {
        tick.tick().await;
        if state.session_epoch() != epoch {
            return;
        }
        let _ = state.advance_sim_for(epoch, clock.elapsed_ms());
        if state.session_status() == SessionStatus::Ended {
            return;
        }
    }
}

/// Spawns [`run_loop`] on the Tauri async runtime with the real clock — the
/// production entry point used by the `start_session` command.
pub fn spawn_scheduler(state: SessionState, epoch: u64, clock: impl TimeSource) {
    tauri::async_runtime::spawn(run_loop(state, epoch, clock));
}
