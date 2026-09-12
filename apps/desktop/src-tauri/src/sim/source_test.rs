//! Behaviour tests for the full four-round SimSource engine (01-05 Task 1).
//!
//! The engine is deterministic by construction: `script_state` is a pure
//! function of elapsed time, and `SimSource` is driven by the elapsed values it
//! is polled with. Production supplies those from the real clock through the
//! injectable `TimeSource` trait; these tests supply scripted values. Nothing
//! here sleeps on a real clock or touches the network.

use std::cell::Cell;

use super::script;
use super::source::{run_loop, script_state, SimSource, TimeSource, INTERRUPT_LEAD_MS};
use crate::lan::server::{ServerEvent, SessionStatus, Speaker};
use crate::state::SessionState;

/// Clock double: hands out the scripted values in order, then keeps repeating
/// the last one (the loop stops on the session's ended status, not the clock).
struct ScriptedClock {
    values: Vec<u64>,
    cursor: Cell<usize>,
}

impl ScriptedClock {
    fn new(values: Vec<u64>) -> Self {
        assert!(!values.is_empty(), "a scripted clock needs values");
        Self {
            values,
            cursor: Cell::new(0),
        }
    }
}

impl TimeSource for ScriptedClock {
    fn elapsed_ms(&self) -> u64 {
        let index = self.cursor.get();
        self.cursor.set(index + 1);
        let last = self.values.len() - 1;
        self.values[index.min(last)]
    }
}

fn subtitles(events: &[ServerEvent]) -> Vec<&ServerEvent> {
    events
        .iter()
        .filter(|event| matches!(event, ServerEvent::Subtitle { .. }))
        .collect()
}

fn strategies(events: &[ServerEvent]) -> Vec<&ServerEvent> {
    events
        .iter()
        .filter(|event| matches!(event, ServerEvent::Strategy { .. }))
        .collect()
}

fn seqs(events: &[ServerEvent]) -> Vec<u64> {
    subtitles(events)
        .iter()
        .filter_map(|event| match event {
            ServerEvent::Subtitle { seq, .. } => Some(*seq),
            _ => None,
        })
        .collect()
}

fn statuses(events: &[ServerEvent]) -> Vec<SessionStatus> {
    events
        .iter()
        .filter_map(|event| match event {
            ServerEvent::Status { session } => Some(*session),
            _ => None,
        })
        .collect()
}

fn subtitle_at(events: &[ServerEvent], index: usize) -> &ServerEvent {
    subtitles(events)
        .get(index)
        .copied()
        .unwrap_or_else(|| panic!("no subtitle at index {index} in {events:#?}"))
}

fn subtitle_id(event: &ServerEvent) -> &str {
    match event {
        ServerEvent::Subtitle { id, .. } => id,
        other => panic!("expected a subtitle, got {other:?}"),
    }
}

// ---------------------------------------------------------------- Test 1 ---

#[test]
fn script_state_is_deterministic_for_a_given_clock() {
    for elapsed in [
        0u64,
        1_000,
        2_500,
        6_000,
        6_500,
        8_500,
        15_000,
        31_900,
        u64::MAX,
    ] {
        assert_eq!(
            script_state(elapsed),
            script_state(elapsed),
            "script_state({elapsed}) must be byte-identical on every call"
        );
    }

    // The same elapsed values chunked differently must produce the same
    // sequence: the output depends only on the clock, never on the poll cadence.
    let mut coarse = SimSource::new();
    let coarse_events = coarse.poll(u64::MAX);

    let mut fine = SimSource::new();
    let mut fine_events = Vec::new();
    for step in 0..=40u64 {
        fine_events.extend(fine.poll(step * 1_000));
    }

    assert_eq!(
        coarse_events, fine_events,
        "poll granularity must not matter"
    );
    assert_eq!(
        coarse_events,
        script_state(u64::MAX),
        "the engine and the pure evaluator must agree"
    );
    assert!(coarse.ended(), "the script is exhausted after u64::MAX");
}

// ---------------------------------------------------------------- Test 2 ---

#[test]
fn all_four_rounds_play_in_order_and_statuses_transition_once_per_phase() {
    let events = script_state(u64::MAX);
    let rounds = script::ROUNDS.len();
    assert_eq!(rounds, 4, "the demo interview is four rounds long");

    // One question + one answer per round, numbered 1..=8 with no gaps.
    assert_eq!(seqs(&events), (1..=8).collect::<Vec<u64>>());
    assert_eq!(subtitles(&events).len(), rounds * 2);
    assert_eq!(strategies(&events).len(), rounds);

    // Round 1 content is followed by r2..r4 — the script is not an r1 loop.
    for (index, round) in script::ROUNDS.iter().enumerate() {
        let question = subtitle_at(&events, index * 2);
        let answer = subtitle_at(&events, index * 2 + 1);
        assert_eq!(subtitle_id(question), format!("{}-q", round.id));
        assert_eq!(subtitle_id(answer), format!("{}-a", round.id));

        let ServerEvent::Subtitle { en, speaker, .. } = question else {
            panic!("expected the r{index} question subtitle");
        };
        assert_eq!(en.as_deref(), Some(round.interviewer_en));
        assert!(matches!(speaker, Speaker::Interviewer));

        let ServerEvent::Subtitle { zh, .. } = answer else {
            panic!("expected the r{index} answer subtitle");
        };
        assert_eq!(zh.as_deref(), Some(round.user_zh));
    }

    // Statuses: listening opens each round, generating follows its answer, and
    // the session ends exactly once.
    let statuses = statuses(&events);
    assert_eq!(statuses.first(), Some(&SessionStatus::Listening));
    assert_eq!(statuses.last(), Some(&SessionStatus::Ended));
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == SessionStatus::Ended)
            .count(),
        1,
        "the session ends exactly once"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == SessionStatus::Listening)
            .count(),
        rounds,
        "one listening phase per round"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == SessionStatus::Generating)
            .count(),
        rounds,
        "one generating phase per round"
    );
}

// ---------------------------------------------------------------- Test 3 ---

#[test]
fn round_one_content_is_byte_exact() {
    // LOCKED r1 content — copied verbatim from the reference mockup; the demo
    // script must never paraphrase it.
    let r1 = &script::ROUNDS[0];
    assert_eq!(
        r1.interviewer_en,
        "Could you walk me through the specific steps you took to optimize the database?"
    );
    assert_eq!(
        r1.interviewer_zh,
        "你能详细说一下你优化数据库的具体步骤吗？"
    );
    assert_eq!(
        r1.user_zh,
        "首先，我们分析了慢查询日志，发现主要瓶颈在商品详情页的连表查询上。"
    );
    assert_eq!(r1.strategy.title, "数据库优化");
    assert_eq!(
        r1.strategy.bullets,
        ["慢查询日志定位", "拆连表查询", "Redis 缓存层"]
    );

    // The same content reaches the wire on the round-1 events.
    let events = script_state(script::ROUNDS[0].timing.end_at_ms);
    let ServerEvent::Subtitle {
        zh, en, final_flag, ..
    } = subtitle_at(&events, 0)
    else {
        panic!("expected the r1 question subtitle");
    };
    assert_eq!(en.as_deref(), Some(r1.interviewer_en));
    assert_eq!(zh.as_deref(), Some(r1.interviewer_zh));
    assert!(*final_flag, "scripted subtitles are already final");

    let ServerEvent::Subtitle {
        zh, en, speaker, ..
    } = subtitle_at(&events, 1)
    else {
        panic!("expected the r1 answer subtitle");
    };
    assert_eq!(zh.as_deref(), Some(r1.user_zh));
    assert_eq!(
        en.as_deref(),
        Some(r1.user_en),
        "the user bubble carries the cloned English line in 01-05"
    );
    assert!(matches!(speaker, Speaker::User));

    let ServerEvent::Strategy {
        id,
        round_id,
        title,
        bullets,
        answer_zh,
        answer_en,
    } = strategies(&events)[0]
    else {
        panic!("expected the r1 strategy card");
    };
    assert_eq!(id.as_str(), "s-r1");
    assert_eq!(round_id.as_str(), "r1");
    assert_eq!(title.as_str(), "数据库优化");
    let expected: Vec<String> = r1.strategy.bullets.iter().map(|b| b.to_string()).collect();
    assert_eq!(bullets, &expected);
    // UAT-8: the strategy carries the complete bilingual answer, not just
    // the prompt outline.
    assert_eq!(answer_zh.as_deref(), Some(r1.strategy.answer_zh));
    assert_eq!(answer_en.as_deref(), Some(r1.strategy.answer_en));
}

#[test]
fn every_round_carries_the_mock_badge_and_a_well_formed_strategy() {
    for round in &script::ROUNDS {
        assert_eq!(
            round.tag,
            script::TAG_MOCK,
            "D-03: the demo is always labelled"
        );
        assert_eq!(round.strategy.tone, script::Tone::AiStrategy);
        assert!(
            !round.strategy.bullets.is_empty(),
            "round {} ships an empty strategy card",
            round.id
        );
        assert!(
            round.timing.strategy_at_ms < round.timing.answer_at_ms
                && round.timing.answer_at_ms < round.timing.generating_at_ms
                && round.timing.generating_at_ms < round.timing.end_at_ms,
            "round {} timing must be strictly ordered",
            round.id
        );
    }
}

// ---------------------------------------------------------------- Test 4 ---

#[test]
fn interrupt_cuts_the_answer_and_opens_the_next_round_one_second_later() {
    let mut sim = SimSource::new();
    let r1 = script::ROUNDS[0].timing;

    // Drive r1 into its generating phase (the answer is on screen).
    let played = sim.poll(r1.generating_at_ms);
    assert_eq!(subtitle_id(subtitle_at(&played, 1)), "r1-a");
    assert_eq!(sim.round_index(), 0);
    assert!(!sim.ended());

    // 打断: the answer is cut immediately and the next round is scheduled for
    // t + INTERRUPT_LEAD_MS.
    let cut = sim.interrupt();
    assert_eq!(
        statuses(&cut),
        vec![SessionStatus::Listening],
        "the mic re-opens immediately after the cut"
    );
    assert_eq!(sim.round_index(), 1, "the cut advances the round");
    assert!(
        sim.poll(r1.generating_at_ms + INTERRUPT_LEAD_MS - 1)
            .is_empty(),
        "no round-2 content may leak in before t + 1s"
    );

    let resumed = sim.poll(r1.generating_at_ms + INTERRUPT_LEAD_MS);
    assert_eq!(
        subtitle_id(subtitle_at(&resumed, 0)),
        "r2-q",
        "round 2 opens exactly one second after the cut"
    );
    assert!(
        resumed.len() < 3,
        "the new round starts at its first milestone only"
    );

    // The cut skipped r1's tail rather than replaying it: numbering continues.
    let mut all = played;
    all.extend(resumed);
    assert_eq!(seqs(&all), vec![1, 2, 3]);
}

#[test]
fn interrupt_on_the_final_round_ends_the_session() {
    let mut sim = SimSource::new();
    let before_last: u64 = script::ROUNDS[..3]
        .iter()
        .map(|round| round.timing.end_at_ms)
        .sum();
    let last = &script::ROUNDS[3];
    sim.poll(before_last + last.timing.generating_at_ms);
    assert_eq!(sim.round_index(), 3, "the engine sits in the final round");

    let cut = sim.interrupt();
    assert_eq!(statuses(&cut), vec![SessionStatus::Ended]);
    assert!(sim.ended());
    assert!(
        sim.repeat().is_empty(),
        "an ended session cannot be replayed"
    );

    // The pure evaluator agrees the session is over at that point.
    let tail = script_state(before_last + last.timing.end_at_ms);
    assert!(matches!(
        tail.last(),
        Some(ServerEvent::Status {
            session: SessionStatus::Ended
        })
    ));
}

// ---------------------------------------------------------------- Test 5 ---

#[test]
fn repeat_replays_the_current_round_with_fresh_seq_and_ids() {
    let mut sim = SimSource::new();
    let r1 = script::ROUNDS[0].timing;
    let played = sim.poll(r1.generating_at_ms);
    let played_seqs = seqs(&played);
    let played_ids: Vec<String> = subtitles(&played)
        .iter()
        .map(|event| subtitle_id(event).to_string())
        .collect();

    // 重听: the current round's content (question + strategy + answer) comes
    // back — content only, because re-firing the round's statuses would drag the
    // UI back from generating to listening mid-answer.
    let replay = sim.repeat();
    assert_eq!(replay.len(), 3, "question + strategy + answer");
    assert_eq!(strategies(&replay).len(), 1);
    assert_eq!(statuses(&replay), Vec::<SessionStatus>::new());

    let replay_seqs = seqs(&replay);
    assert_eq!(replay_seqs.len(), 2);
    for seq in &replay_seqs {
        assert!(
            !played_seqs.contains(seq),
            "a replayed subtitle must not reuse seq {seq} (the phone dedupes on it)"
        );
    }
    let replay_ids: Vec<String> = subtitles(&replay)
        .iter()
        .map(|event| subtitle_id(event).to_string())
        .collect();
    for id in &replay_ids {
        assert!(
            !played_ids.contains(id),
            "replayed ids must be fresh: {id} (the desktop keys bubbles by id)"
        );
    }
    let ServerEvent::Strategy { id, .. } = strategies(&replay)[0] else {
        panic!("expected a replayed strategy card");
    };
    assert_ne!(
        id.as_str(),
        "s-r1",
        "a replayed strategy card needs a fresh id"
    );

    // Numbering keeps climbing: the next round's subtitle follows the replay.
    let resumed = sim.poll(r1.end_at_ms);
    let next_seq = seqs(&resumed)[0];
    assert!(
        replay_seqs.iter().all(|seq| *seq < next_seq),
        "seq must stay monotonic across a repeat"
    );
}

// ------------------------------------------------------- scheduler wiring ---

/// The state's timeline for a started session: the identity marker opens it
/// (CR-01), then the engine's canonical sequence follows.
fn started_script_state(epoch: u64) -> Vec<ServerEvent> {
    let mut events = vec![ServerEvent::SessionStarted { epoch }];
    events.extend(script_state(script::total_duration_ms()));
    events
}

#[tokio::test]
async fn scheduler_plays_the_script_through_a_scripted_clock() {
    let state = SessionState::new(8787);
    assert_eq!(state.session_status(), SessionStatus::Idle);

    let epoch = state.start_session().expect("a fresh session starts");
    assert_eq!(state.session_status(), SessionStatus::Listening);

    // Three ticks carry the demo: r1 opens, r1 answers, the script runs to its
    // end (and the loop exits on the ended status).
    run_loop(
        state.clone(),
        epoch,
        ScriptedClock::new(vec![0, 6_500, script::total_duration_ms()]),
    )
    .await;

    assert_eq!(state.session_status(), SessionStatus::Ended);
    let timeline = state.timeline();
    assert_eq!(
        timeline,
        started_script_state(epoch),
        "the engine appends the canonical sequence to the timeline"
    );
    assert!(matches!(
        timeline.last(),
        Some(ServerEvent::Status {
            session: SessionStatus::Ended
        })
    ));
}

#[tokio::test]
async fn scheduler_stops_when_its_epoch_is_superseded() {
    let state = SessionState::new(8787);
    let epoch = state.start_session().expect("a fresh session starts");

    // stop_session bumps the epoch; the running scheduler must notice on its
    // next tick and exit without appending anything further.
    state.stop_session();
    let stale = run_loop(
        state.clone(),
        epoch,
        ScriptedClock::new(vec![script::total_duration_ms()]),
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), stale)
        .await
        .expect("a superseded scheduler must exit on its next tick");

    assert_eq!(state.session_status(), SessionStatus::Ended);
    assert_eq!(
        state.timeline(),
        vec![
            ServerEvent::SessionStarted { epoch },
            ServerEvent::Status {
                session: SessionStatus::Ended
            }
        ],
        "a superseded scheduler must not append script events"
    );
}
