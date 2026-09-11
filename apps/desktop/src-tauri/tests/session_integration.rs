//! Full-session integration test (01-05 Task 2): the demo path, end to end.
//!
//! Drives the real `SessionState` and the real LAN router the way the app does
//! — no mocks, no fake server:
//!
//! `start_session` → the deterministic engine plays round 1 → a real WS client
//! (the phone) receives the exact events the desktop renders → the phone's
//! language control round-trips into a `language` event → 打断 opens the next
//! round one second later → 重听 replays the round with fresh sequence numbers
//! → the connected-client counter walks 0 → 1 → 2 → 1 → 0.
//!
//! Loopback only: one ephemeral 127.0.0.1 port. No external network, no keys,
//! no clock sleeps beyond the bounded waits that observe a socket closing.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use nextalk_desktop_lib::lan::server::{
    router, teleprompter_dist_path, LanguagePref, ServerEvent, SessionStatus, Speaker,
};
use nextalk_desktop_lib::sim::script::ROUNDS;
use nextalk_desktop_lib::sim::source::INTERRUPT_LEAD_MS;
use nextalk_desktop_lib::state::SessionState;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as ClientFrame;

type Client =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

// ------------------------------------------------------------------ harness ---

/// Boots the real router on an ephemeral loopback port.
async fn spawn_server() -> (String, SessionState) {
    let state = SessionState::new(8787);
    let app = router(state.clone(), teleprompter_dist_path());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("ephemeral addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("axum serve");
    });
    (format!("ws://{addr}/ws"), state)
}

/// Pairs a phone against the running server (pairing-as-auth).
async fn connect(base: &str, token: &str) -> Client {
    let (ws, _) = connect_async(format!("{base}?token={token}"))
        .await
        .expect("upgrade accepted for the valid token");
    ws
}

/// One resume round-trip: proves the client loop (and its broadcast
/// subscription) is live before the test appends anything.
async fn pair(ws: &mut Client) {
    ws.send(ClientFrame::Text(r#"{"t":"resume","sinceSeq":0}"#.into()))
        .await
        .expect("send resume");
    assert_eq!(
        read_event(ws).await,
        ServerEvent::Timeline { events: vec![] },
        "a fresh session has an empty timeline"
    );
}

/// Reads the next text frame as a ServerEvent (5 s guard).
async fn read_event(ws: &mut Client) -> ServerEvent {
    let frame = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("a WS frame within 5s")
        .expect("the WS stream is still open")
        .expect("no transport error");
    match frame {
        ClientFrame::Text(text) => serde_json::from_str(&text).expect("a well-formed ServerEvent"),
        other => panic!("expected a text frame, got {other:?}"),
    }
}

/// Reads frames until `wanted` matches (bounded), skipping whatever else the
/// session interleaves on the same stream.
async fn read_until(
    ws: &mut Client,
    what: &str,
    wanted: impl Fn(&ServerEvent) -> bool,
) -> ServerEvent {
    for _ in 0..16 {
        let event = read_event(ws).await;
        if wanted(&event) {
            return event;
        }
    }
    panic!("no {what} on the WS stream within 16 frames");
}

/// Reads the next subtitle frame off the stream (skipping statuses/strategies).
async fn read_subtitle(ws: &mut Client, what: &str) -> ServerEvent {
    read_until(ws, what, |event| {
        matches!(event, ServerEvent::Subtitle { .. })
    })
    .await
}

/// The subtitle fields the assertions care about.
#[derive(Debug)]
struct Sub<'a> {
    id: &'a str,
    speaker: Speaker,
    seq: u64,
    zh: Option<&'a str>,
    en: Option<&'a str>,
}

fn sub(event: &ServerEvent) -> Sub<'_> {
    match event {
        ServerEvent::Subtitle {
            id,
            speaker,
            seq,
            zh,
            en,
            ..
        } => Sub {
            id,
            speaker: *speaker,
            seq: *seq,
            zh: zh.as_deref(),
            en: en.as_deref(),
        },
        other => panic!("expected a subtitle, got {other:?}"),
    }
}

fn subtitles_of(events: &[ServerEvent]) -> Vec<Sub<'_>> {
    events
        .iter()
        .filter_map(|event| match event {
            ServerEvent::Subtitle { .. } => Some(sub(event)),
            _ => None,
        })
        .collect()
}

/// Bounded wait for the socket-close bookkeeping to land (max 2 s).
async fn wait_for_clients(state: &SessionState, expected: usize) -> usize {
    for _ in 0..100 {
        if state.connected_clients() == expected {
            return expected;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    state.connected_clients()
}

// ------------------------------------------------------------- the demo path ---

#[tokio::test]
async fn full_demo_session_reaches_the_phone_and_applies_the_language_control() {
    let (base, state) = spawn_server().await;
    let token = state.pairing_token();

    // The phone pairs before the session starts.
    let mut phone = connect(&base, &token).await;
    pair(&mut phone).await;
    assert_eq!(state.connected_clients(), 1);

    // 开始模拟会话: idle -> listening, engine armed at this epoch.
    let epoch = state.start_session().expect("a fresh session starts");
    assert_eq!(state.session_status(), SessionStatus::Listening);

    // The first scheduler tick lands mid-round-1: the interviewer's question,
    // the strategy card, the user's answer, then the generating status.
    let r1 = &ROUNDS[0].timing;
    let played = state
        .advance_sim_for(epoch, r1.generating_at_ms)
        .expect("the epoch is current");
    assert_eq!(state.session_status(), SessionStatus::Generating);
    assert_eq!(
        state.timeline().len(),
        played.len(),
        "every published event lands in the timeline (resume replay)"
    );

    let r1_subtitles = subtitles_of(&played);
    assert_eq!(r1_subtitles.len(), 2, "question + answer");
    let question = &r1_subtitles[0];
    assert_eq!(question.id, "r1-q");
    assert_eq!(question.speaker, Speaker::Interviewer);
    assert_eq!(question.seq, 1);
    assert_eq!(question.en, Some(ROUNDS[0].interviewer_en));
    assert_eq!(question.zh, Some(ROUNDS[0].interviewer_zh));

    let answer = &r1_subtitles[1];
    assert_eq!(answer.id, "r1-a");
    assert_eq!(answer.speaker, Speaker::User);
    assert_eq!(answer.seq, 2);
    assert_eq!(answer.zh, Some(ROUNDS[0].user_zh));
    assert!(
        matches!(&played[2], ServerEvent::Strategy { title, .. } if title == "数据库优化"),
        "the strategy card follows the question: {played:#?}"
    );

    // The phone receives the identical sequence — one event model, two
    // transports (SYNC-01).
    assert_eq!(
        read_event(&mut phone).await,
        ServerEvent::Status {
            session: SessionStatus::Listening
        }
    );
    let question_frame = read_subtitle(&mut phone, "the r1 question").await;
    let question_frame = sub(&question_frame);
    assert_eq!(
        (question_frame.id, question_frame.seq),
        ("r1-q", 1),
        "the phone sees the r1 question in the same order and numbering"
    );
    let answer_frame = read_subtitle(&mut phone, "the r1 answer").await;
    let answer_frame = sub(&answer_frame);
    assert_eq!(answer_frame.id, "r1-a");
    assert_eq!(answer_frame.seq, 2);

    // SYNC-03 round-trip: the phone's mode is applied AND published back
    // through the same server event, so the desktop webviews observe it.
    phone
        .send(ClientFrame::Text(
            r#"{"t":"control","language":"all-en"}"#.into(),
        ))
        .await
        .expect("send control");
    let applied = read_until(&mut phone, "the applied language mode", |event| {
        matches!(event, ServerEvent::Language { .. })
    })
    .await;
    assert_eq!(
        applied,
        ServerEvent::Language {
            language: LanguagePref::AllEn
        },
        "the applied mode is broadcast, not just stored"
    );
    assert_eq!(state.language_prefs(), LanguagePref::AllEn);

    // 打断: the cut is immediate and the next round opens one second later.
    state
        .interrupt_session()
        .expect("打断 applies while the answer is generating");
    assert_eq!(state.session_status(), SessionStatus::Listening);

    let cut_at = r1.generating_at_ms;
    let early = state
        .advance_sim_for(epoch, cut_at + INTERRUPT_LEAD_MS - 1)
        .expect("the epoch is current");
    assert!(
        early.is_empty(),
        "no next-round content may leak in before the lead-in elapses"
    );

    let r2 = state
        .advance_sim_for(epoch, cut_at + INTERRUPT_LEAD_MS)
        .expect("the epoch is current");
    let r2_subtitles = subtitles_of(&r2);
    assert_eq!(
        r2_subtitles.len(),
        1,
        "the new round opens one milestone in"
    );
    assert_eq!(r2_subtitles[0].id, "r2-q");
    assert_eq!(
        r2_subtitles[0].seq, 3,
        "numbering continues across the cut — a dropped round never reuses a seq"
    );
    assert_eq!(
        read_event(&mut phone).await,
        ServerEvent::Status {
            session: SessionStatus::Listening
        },
        "the phone observes the cut too"
    );
    let r2_on_phone = read_subtitle(&mut phone, "the r2 question").await;
    assert_eq!(sub(&r2_on_phone).id, "r2-q");

    // 重听: the current round replays with fresh sequence numbers and ids, so
    // neither the phone (dedupe on seq) nor the desktop (bubble keyed by id)
    // mistakes the replay for a duplicate.
    let r2_start = cut_at + INTERRUPT_LEAD_MS;
    state
        .advance_sim_for(epoch, r2_start + ROUNDS[1].timing.generating_at_ms)
        .expect("the epoch is current");
    assert_eq!(state.session_status(), SessionStatus::Generating);

    state
        .repeat_session()
        .expect("重听 applies while the answer is generating");

    let mut replay = Vec::new();
    while replay.len() < 3 {
        let event = read_event(&mut phone).await;
        let is_replay = matches!(
            &event,
            ServerEvent::Subtitle { id, .. } | ServerEvent::Strategy { id, .. }
                if id.ends_with("-r1")
        );
        if is_replay {
            replay.push(event);
        }
    }
    let replayed = subtitles_of(&replay);
    assert_eq!(
        replayed.len(),
        2,
        "question + answer, strategy between them"
    );
    assert_eq!(replayed[0].id, "r2-q-r1", "a replay needs a fresh id");
    assert_eq!(replayed[1].id, "r2-a-r1");
    assert!(
        replayed.iter().all(|item| item.seq > r2_subtitles[0].seq),
        "replayed subtitles must not reuse a seq the phone already saw: {replayed:#?}"
    );

    // The whole session stays monotonically numbered, so a phone resuming with
    // its last seq can never skip or duplicate a line.
    let seqs: Vec<u64> = subtitles_of(&state.timeline())
        .iter()
        .map(|item| item.seq)
        .collect();
    let mut sorted = seqs.clone();
    sorted.sort_unstable();
    assert_eq!(seqs, sorted, "subtitle seq must never go backwards");

    assert_eq!(state.connected_clients(), 1, "one phone, throughout");
}

#[tokio::test]
async fn connected_client_counter_tracks_the_live_phone_count() {
    let (base, state) = spawn_server().await;
    let token = state.pairing_token();
    assert_eq!(state.connected_clients(), 0, "no phone paired yet");

    let mut first = connect(&base, &token).await;
    pair(&mut first).await;
    assert_eq!(wait_for_clients(&state, 1).await, 1);

    let mut second = connect(&base, &token).await;
    pair(&mut second).await;
    assert_eq!(wait_for_clients(&state, 2).await, 2);

    // A dropped socket must not leave a ghost in 已连接 N 台设备.
    drop(second);
    assert_eq!(wait_for_clients(&state, 1).await, 1);

    drop(first);
    assert_eq!(wait_for_clients(&state, 0).await, 0);
}
