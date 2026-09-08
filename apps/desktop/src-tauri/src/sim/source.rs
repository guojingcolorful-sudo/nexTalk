//! Deterministic simulation source (01-02 walking skeleton).
//!
//! `script_state(elapsed_ms)` is a pure evaluator: given elapsed wall-clock
//! time since session start it derives the complete event sequence — same
//! input, same events; no clock, no I/O. The scheduler ticks every 100 ms,
//! appends the newly-matured events to `SessionState` and mirrors them on the
//! Tauri `session` emit so desktop and H5 observe the same timeline.

use crate::lan::server::{ServerEvent, SessionStatus, Speaker};
use crate::sim::script;
use crate::state::SessionState;
use tauri::Emitter;

/// Scheduler tick (matches the 100 ms cadence of the protocol research).
const TICK_MS: u64 = 100;

/// Pure evaluator: all script events whose offset is `<= elapsed_ms`, in
/// emission order. No clock, no I/O — deterministic for a given input.
pub fn script_state(elapsed_ms: u64) -> Vec<ServerEvent> {
    let mut events = Vec::new();
    // The question opens the round at offset 0, so it is present for every
    // non-negative elapsed time.
    events.push(ServerEvent::Subtitle {
        id: script::QUESTION_ID.into(),
        speaker: Speaker::Interviewer,
        seq: script::QUESTION_SEQ,
        zh: Some(script::QUESTION_ZH.into()),
        en: Some(script::QUESTION_EN.into()),
        final_flag: true,
    });
    if elapsed_ms >= script::STRATEGY_AT_MS {
        events.push(ServerEvent::Strategy {
            id: script::STRATEGY_ID.into(),
            round_id: script::ROUND_ID.into(),
            title: script::STRATEGY_TITLE.into(),
            bullets: script::STRATEGY_BULLETS
                .iter()
                .map(|b| b.to_string())
                .collect(),
        });
    }
    if elapsed_ms >= script::ANSWER_AT_MS {
        events.push(ServerEvent::Subtitle {
            id: script::ANSWER_ID.into(),
            speaker: Speaker::User,
            seq: script::ANSWER_SEQ,
            zh: Some(script::ANSWER_ZH.into()),
            en: None, // translation arrives in a later phase (01-05)
            final_flag: true,
        });
    }
    if elapsed_ms >= script::GENERATING_AT_MS {
        events.push(ServerEvent::Status {
            session: SessionStatus::Generating,
        });
    }
    if elapsed_ms >= script::LISTENING_AT_MS {
        events.push(ServerEvent::Status {
            session: SessionStatus::Listening,
        });
    }
    events
}

/// Total number of events the script produces once fully elapsed.
pub fn script_total_events() -> usize {
    script_state(u64::MAX).len()
}

/// Spawns the 100 ms scheduler: appends matured events to `state` and emits
/// each on the Tauri `session` channel so every transport sees the timeline.
/// Script status transitions also go out on `session_status` (drives the
/// console status dot); after the last event the session returns to idle so
/// the demo round can be re-run.
pub fn spawn_scheduler(state: SessionState, app: tauri::AppHandle) {
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(TICK_MS));
    tauri::async_runtime::spawn(async move {
        let total = script_total_events();
        let mut emitted = 0usize;
        let mut elapsed_ms = 0u64;
        while emitted < total {
            tick.tick().await;
            elapsed_ms = elapsed_ms.saturating_add(TICK_MS);
            let events = script_state(elapsed_ms);
            for event in &events[emitted..] {
                state.append_event(event.clone());
                let _ = app.emit("session", event);
                if let ServerEvent::Status { session } = event {
                    let _ = state.set_session_status(*session);
                    let _ = app.emit("session_status", serde_json::json!({ "session": session }));
                }
            }
            emitted = events.len();
        }
        // Script played to the end: back to idle so the demo can restart.
        let _ = state.set_session_status(SessionStatus::Idle);
        let _ = app.emit("session_status", serde_json::json!({ "session": "idle" }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lan::server::{ServerEvent, Speaker};
    use crate::sim::script;

    #[test]
    fn script_state_at_zero_emits_the_question_first() {
        let events = script_state(0);
        let first = events.first().expect("first event exists");
        match first {
            ServerEvent::Subtitle { speaker, seq, .. } => {
                assert_eq!(*seq, script::QUESTION_SEQ);
                assert!(matches!(speaker, Speaker::Interviewer));
            }
            other => panic!("expected the question subtitle first, got {other:?}"),
        }
    }

    #[test]
    fn script_state_is_deterministic() {
        assert_eq!(script_state(1_000), script_state(1_000));
        assert_eq!(
            script_state(script::LISTENING_AT_MS),
            script_state(script::LISTENING_AT_MS)
        );
    }

    #[test]
    fn script_state_grows_monotonically_with_elapsed_time() {
        let before = script_state(script::STRATEGY_AT_MS - 1);
        let at = script_state(script::STRATEGY_AT_MS);
        assert!(
            at.len() > before.len(),
            "strategy event must fire at its offset"
        );
        assert!(
            at.starts_with(&before),
            "later states must be prefix extensions of earlier ones"
        );
        let final_events = script_state(u64::MAX);
        assert_eq!(final_events.len(), script_total_events());
    }

    #[test]
    fn user_answer_subtitle_has_no_en_yet() {
        // The skeleton answer is Chinese-only (translation arrives in later
        // phases); the H5 must render the "translating" cue instead of an
        // empty English bubble.
        let answer = script_state(u64::MAX)
            .into_iter()
            .find(|e| matches!(e, ServerEvent::Subtitle { seq: 2, .. }))
            .expect("answer subtitle exists");
        match answer {
            ServerEvent::Subtitle { en, zh, .. } => {
                assert!(en.is_none(), "answer must not carry an en yet");
                assert!(zh.is_some(), "answer must carry zh");
            }
            _ => unreachable!("filtered for subtitle"),
        }
    }
}
