//! 02-05 T5.4 集成套件 — 设备热切换（错误驱动的流重建）。
//!
//! The device layer's job is unglamorous and load-bearing: when CoreAudio pulls
//! a device out from under a live interview, the session has to survive it. The
//! tests here run entirely against a **scripted factory** — no devices, no
//! network, CI-safe. The real plug/unplug path is the `#[ignore]` test at the
//! bottom, run by hand once and recorded in the SUMMARY.
//!
//! | # | contract | why it matters |
//! |---|----------|----------------|
//! | 1 | a fault rebuilds both streams, exactly once | a rebuild loop is an outage the user did not have |
//! | 2 | one device object serves both directions | macOS refuses a second open of the same device |
//! | 3 | every build carries a timeout | an unresponsive device must be an error, not a hung session |
//! | 4 | the rebuild is silent and the sentence survives | the words must come back, and not as a click |
//! | 5 | `DeviceChanged` is not a rebuild | cpal says the stream is still active; reopening is the bug |
//! | 6 | no hot-plug notification is assumed | cpal 0.18 has none; the only signal is the error callback |
//! | 7 | 停止 stops the rebuilding | a fault after the session ended must reopen nothing |
//!
//! Threat model: T-02-22 (device names are untrusted display strings, used only
//! for display and by-name matching), T-02-23 (rebuilds run off the audio
//! callback, never inside it), T-02-26 (short backoff and a failure cap — a
//! plug/unplug storm must not become a rebuild storm).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nextalk_desktop_lib::audio::playout::PlayoutChain;
use nextalk_desktop_lib::audio::device::{
    DeviceFault, DeviceHandle, DeviceManager, DeviceRef, OpenRequest, OpenStream, RebuildGate,
    RebuildOutcome, RebuildPolicy, StreamDirection, StreamFactory,
};
use nextalk_desktop_lib::sim::source::TimeSource;

// ---------------------------------------------------------------------------
// the scripted backend
// ---------------------------------------------------------------------------

/// A clock the test drives by hand, so the backoff is deterministic and the
/// suite never sleeps.
struct TestClock(AtomicUsize);

impl TestClock {
    fn new() -> Self {
        Self(AtomicUsize::new(0))
    }

    fn advance(&self, ms: usize) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl TimeSource for TestClock {
    fn elapsed_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst) as u64
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct FactoryLog {
    /// How many times the device list was re-read. A rebuild re-enumerates
    /// **once** — twice would let the input and output lists disagree.
    enumerations: usize,
    /// `(direction, device id, timeout)` for every stream build attempted.
    opened: Vec<(StreamDirection, String, Duration)>,
    closed: Vec<String>,
}

struct ScriptedFactory {
    log: Arc<Mutex<FactoryLog>>,
    /// One shared handle per id — the point of Test 2. Enumerating twice hands
    /// back the *same* object, exactly as a `cpal::Device` cache would.
    handles: Vec<DeviceHandle>,
    /// Which device each direction opens. Names, never capability flags
    /// (T-02-22): a virtual driver reports both directions and lies about
    /// being a microphone.
    defaults: Mutex<(String, String)>,
    /// Devices that refuse to open, and how they refuse.
    broken: Mutex<Vec<(String, DeviceFault)>>,
}

impl ScriptedFactory {
    fn new() -> Self {
        Self {
            log: Arc::new(Mutex::new(FactoryLog::default())),
            handles: vec![
                DeviceRef::shared("BuiltInMic:0", "MacBook Pro 麦克风"),
                DeviceRef::shared("Headset:2", "Jabra Evolve2"),
                DeviceRef::shared("BuiltInOut:1", "MacBook Pro 扬声器"),
                DeviceRef::shared("BlackHole:3", "BlackHole 2ch"),
            ],
            defaults: Mutex::new(("BuiltInMic:0".into(), "BuiltInOut:1".into())),
            broken: Mutex::new(Vec::new()),
        }
    }

    fn handle(&self, id: &str) -> DeviceHandle {
        self.handles
            .iter()
            .find(|handle| handle.id() == id)
            .cloned()
            .unwrap_or_else(|| panic!("the script only names known devices, not {id}"))
    }

    /// Point both directions at the same physical device — a headset.
    fn use_one_device_for_both(&self, id: &str) {
        *self.defaults.lock().expect("test mutex") = (id.to_string(), id.to_string());
    }

    fn break_device(&self, id: &str, fault: DeviceFault) {
        let mut broken = self.broken.lock().expect("test mutex");
        broken.retain(|(broken, _)| broken != id);
        broken.push((id.to_string(), fault));
    }

    fn repair_device(&self, id: &str) {
        self.broken
            .lock()
            .expect("test mutex")
            .retain(|(broken, _)| broken != id);
    }

    fn log(&self) -> std::sync::MutexGuard<'_, FactoryLog> {
        self.log.lock().expect("test mutex")
    }
}

impl StreamFactory for ScriptedFactory {
    fn enumerate(&self) -> Result<Vec<DeviceHandle>, DeviceFault> {
        self.log().enumerations += 1;
        Ok(self.handles.clone())
    }

    fn default_device(&self, direction: StreamDirection) -> Option<DeviceHandle> {
        let defaults = self.defaults.lock().expect("test mutex");
        let id = match direction {
            StreamDirection::Input => &defaults.0,
            StreamDirection::Output => &defaults.1,
        };
        Some(self.handle(id))
    }

    fn open(&self, request: &OpenRequest<'_>) -> Result<Box<dyn OpenStream>, DeviceFault> {
        let id = request
            .device
            .as_ref()
            .map(|device| device.id().to_string())
            .unwrap_or_else(|| "<default>".to_string());
        self.log()
            .opened
            .push((request.direction, id.clone(), request.timeout));
        if let Some((_, fault)) = self
            .broken
            .lock()
            .expect("test mutex")
            .iter()
            .find(|(broken, _)| *broken == id)
        {
            return Err(fault.clone());
        }
        Ok(Box::new(ScriptedStream {
            device: request.device.clone(),
            log: Arc::clone(&self.log),
        }))
    }
}

struct ScriptedStream {
    device: Option<DeviceHandle>,
    log: Arc<Mutex<FactoryLog>>,
}

impl OpenStream for ScriptedStream {
    fn device(&self) -> Option<&DeviceHandle> {
        self.device.as_ref()
    }
}

impl Drop for ScriptedStream {
    fn drop(&mut self) {
        if let Ok(mut log) = self.log.lock() {
            let id = self
                .device
                .as_ref()
                .map(|device| device.id().to_string())
                .unwrap_or_else(|| "<default>".to_string());
            log.closed.push(id);
        }
    }
}

/// A `RebuildGate` that records what it was told, so the test can assert the
/// playout was silenced *before* the enumeration and released only once the
/// new streams were live.
struct TestGate {
    log: Arc<Mutex<Vec<&'static str>>>,
}

impl TestGate {
    fn new() -> (Self, Arc<Mutex<Vec<&'static str>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                log: Arc::clone(&log),
            },
            log,
        )
    }
}

impl RebuildGate for TestGate {
    fn suspend(&mut self) {
        self.log.lock().expect("test mutex").push("suspend");
    }

    fn resume(&mut self) {
        self.log.lock().expect("test mutex").push("resume");
    }
}

fn policy() -> RebuildPolicy {
    RebuildPolicy {
        backoff_ms: 250,
        max_consecutive_failures: 3,
        cooldown_ms: 5_000,
        open_timeout: Duration::from_secs(5),
    }
}

fn manager(factory: Arc<ScriptedFactory>, clock: Arc<TestClock>) -> DeviceManager {
    DeviceManager::with_policy(factory, policy(), clock)
}

fn unplugged() -> DeviceFault {
    DeviceFault::NotAvailable {
        detail: "设备已断开".to_string(),
    }
}

fn count_attempts(outcomes: &[RebuildOutcome]) -> usize {
    outcomes
        .iter()
        .filter(|outcome| {
            matches!(
                outcome,
                RebuildOutcome::Rebuilt { .. } | RebuildOutcome::Failed { .. }
            )
        })
        .count()
}

// ---------------------------------------------------------------------------
// T5.4 Test 2 — the rebuild path
// ---------------------------------------------------------------------------

#[test]
fn a_fault_rebuilds_both_streams_exactly_once() {
    let factory = Arc::new(ScriptedFactory::new());
    let clock = Arc::new(TestClock::new());
    let mut manager = manager(Arc::clone(&factory), Arc::clone(&clock));
    let (gate, gate_log) = TestGate::new();
    manager.attach_gate(Box::new(gate));

    manager.start_session().expect("the session opens");
    assert_eq!(factory.log().enumerations, 1, "one enumeration on open");
    assert_eq!(factory.log().opened.len(), 2, "one input, one output");

    // The headphones come out mid-interview.
    let outcome = manager.report_fault(unplugged());
    assert!(
        matches!(outcome, RebuildOutcome::Rebuilt { .. }),
        "the fault rebuilt the session: {outcome:?}"
    );
    assert_eq!(
        factory.log().enumerations,
        2,
        "the rebuild re-read the device list — the list is why we are here"
    );

    let opened = factory.log().opened.clone();
    let after = &opened[2..];
    assert_eq!(after.len(), 2, "exactly one input and one output: {after:?}");
    assert_eq!(
        after
            .iter()
            .filter(|(direction, _, _)| *direction == StreamDirection::Input)
            .count(),
        1,
        "no duplicate input open — macOS fails the second one"
    );
    assert_eq!(
        after
            .iter()
            .filter(|(direction, _, _)| *direction == StreamDirection::Output)
            .count(),
        1,
        "no duplicate output open"
    );

    // The gate was silenced before the enumeration and released after the new
    // streams were live — not the other way round.
    assert_eq!(
        *gate_log.lock().expect("test mutex"),
        vec!["suspend", "resume"],
        "silence first, sound only once the device is back"
    );

    let status = manager.status();
    assert!(status.in_session);
    assert_eq!(status.rebuilds, 1);
    assert_eq!(status.consecutive_failures, 0, "success cleared the tally");
    assert!(status.stalled.is_none());
}

// ---------------------------------------------------------------------------
// T5.4 Test 3 — one device object for both directions
// ---------------------------------------------------------------------------

#[test]
fn one_device_object_serves_both_directions_when_they_are_the_same_device() {
    let factory = Arc::new(ScriptedFactory::new());
    let clock = Arc::new(TestClock::new());
    factory.use_one_device_for_both("Headset:2");
    let mut manager = manager(Arc::clone(&factory), Arc::clone(&clock));
    manager.start_session().expect("the session opens");

    // A headset is one piece of hardware with a mic and a speaker. cpal 0.18
    // compares `Device` by `audio_device_id`, and macOS refuses a second open
    // of the same device — so the fix is to resolve it once and hand the
    // *same object* to both builds.
    let input = manager.current(StreamDirection::Input).expect("input");
    let output = manager.current(StreamDirection::Output).expect("output");
    assert_eq!(input.id(), output.id(), "one headset, both directions");
    assert!(
        Arc::ptr_eq(&input, &output),
        "the same device object, not two equal copies — identity is what the OS checks"
    );
    assert_eq!(
        factory.log().enumerations,
        1,
        "enumerated once and shared, not enumerated per direction"
    );

    // And the identity survives a rebuild: the new pair is still one object.
    manager.report_fault(unplugged());
    let input = manager.current(StreamDirection::Input).expect("input");
    let output = manager.current(StreamDirection::Output).expect("output");
    assert!(Arc::ptr_eq(&input, &output));
}

// ---------------------------------------------------------------------------
// T5.4 Test 4 — the timeout boundary and the failure cap
// ---------------------------------------------------------------------------

#[test]
fn every_build_carries_a_timeout_and_a_storm_of_faults_cannot_become_a_storm_of_rebuilds() {
    let factory = Arc::new(ScriptedFactory::new());
    let clock = Arc::new(TestClock::new());
    let mut manager = manager(Arc::clone(&factory), Arc::clone(&clock));

    // A device that never answers. The scripted backend reports it the way
    // CoreAudio does: the build returns, with an error.
    factory.break_device("BuiltInMic:0", unplugged());
    // The session opens anyway — the output stream is fine, and a mic fault
    // must not take the whole session down before the user even speaks.
    let _ = manager.start_session();

    for (_, _, timeout) in factory.log().opened.clone() {
        assert!(
            timeout > Duration::ZERO,
            "cpal takes Option<Duration>; None means 'wait forever', and forever is a hung interview"
        );
        assert_eq!(timeout, Duration::from_secs(5));
    }
    assert!(
        !factory.log().opened.is_empty(),
        "the boundary was actually exercised"
    );

    // Sixty faults arriving faster than the backoff must not become sixty
    // rebuilds. Ten milliseconds between faults, a 250 ms backoff and a cap of
    // three: exactly three attempts, then the session-level error.
    let mut outcomes = Vec::new();
    for _ in 0..60 {
        outcomes.push(manager.report_fault(unplugged()));
        clock.advance(10);
    }

    assert_eq!(
        count_attempts(&outcomes),
        policy().max_consecutive_failures as usize,
        "the cap is a cap, not a suggestion: {outcomes:?}"
    );
    assert!(
        outcomes
            .iter()
            .any(|outcome| matches!(outcome, RebuildOutcome::Deferred { .. })),
        "the backoff deferred the retries instead of hammering the device"
    );
    assert!(
        outcomes
            .iter()
            .any(|outcome| matches!(outcome, RebuildOutcome::Exhausted { .. })),
        "and the storm ended in a reported stall, not an endless loop"
    );

    let status = manager.status();
    assert!(status.stalled.is_some(), "the session-level error is raised");
    assert_eq!(
        status.stalled.as_ref().map(DeviceFault::code),
        Some("device_not_available"),
        "and it names the fault that caused it"
    );
    assert_eq!(
        status.consecutive_failures,
        policy().max_consecutive_failures
    );
}

#[test]
fn a_device_that_comes_back_ends_the_stall_and_the_session_recovers() {
    let factory = Arc::new(ScriptedFactory::new());
    let clock = Arc::new(TestClock::new());
    let mut manager = manager(Arc::clone(&factory), Arc::clone(&clock));
    factory.break_device("BuiltInMic:0", unplugged());
    let _ = manager.start_session();

    for _ in 0..60 {
        manager.report_fault(unplugged());
        clock.advance(10);
    }
    assert!(manager.status().stalled.is_some(), "the session is stalled");

    // The cap stops the *storm*, not the *session*: a device can only be
    // observed by trying again, so the recovery path must stay reachable after
    // the cooldown. Otherwise a headset that briefly slept would end the
    // interview for good.
    factory.repair_device("BuiltInMic:0");
    clock.advance(10_000);
    let outcome = manager.report_fault(unplugged());
    assert!(
        matches!(outcome, RebuildOutcome::Rebuilt { .. }),
        "after the cooldown the retry is allowed: {outcome:?}"
    );

    let status = manager.status();
    assert!(status.in_session, "the session is alive");
    assert!(status.input.is_some() && status.output.is_some());
    assert!(status.stalled.is_none(), "the stall was cleared by success");
    assert_eq!(status.consecutive_failures, 0, "and so was the tally");
}

// ---------------------------------------------------------------------------
// T5.4 Test 5 — silence during the rebuild, the sentence afterwards
// ---------------------------------------------------------------------------

/// Wraps the chain behind the gate trait while leaving the test a handle to it.
struct ChainGate(Arc<Mutex<PlayoutChain>>);

impl ChainGate {
    fn new(chain: PlayoutChain) -> (Self, Arc<Mutex<PlayoutChain>>) {
        let shared = Arc::new(Mutex::new(chain));
        (
            Self {
                handle: Arc::clone(&shared),
            },
            shared,
        )
    }
}

impl RebuildGate for ChainGate {
    fn suspend(&mut self) {
        self.handle.lock().expect("chain").suspend();
    }

    fn resume(&mut self) {
        self.handle.lock().expect("chain").resume();
    }
}

#[test]
fn the_rebuild_is_silent_and_the_sentence_survives_it() {
    use nextalk_desktop_lib::audio::capture::GRAPH_RATE_HZ;
    use nextalk_desktop_lib::audio::playout::JitterPolicy;

    let factory = Arc::new(ScriptedFactory::new());
    let clock = Arc::new(TestClock::new());
    let mut manager = manager(Arc::clone(&factory), Arc::clone(&clock));

    // A real playout chain, not a stub: the gate's behaviour *is* the contract.
    let mut chain = PlayoutChain::with_policy(JitterPolicy::default());
    let epoch = chain.begin_session();
    let block = 480usize;
    // 150 ms: above the 120 ms pre-roll target, below the 200 ms cap.
    let spoken: Vec<f32> = (0..block * 15)
        .map(|n| (n as f32 / 48_000.0 * 440.0 * std::f32::consts::TAU).sin() * 0.5)
        .collect();
    chain
        .push(epoch, 1, &spoken, GRAPH_RATE_HZ)
        .expect("epoch 1");

    let mut tick = vec![0.0f32; block];
    assert_eq!(chain.tick(&mut tick), block, "playing before the fault");
    let depth_before = chain.buffered_ms();
    assert_eq!(depth_before, 140);

    let (gate, handle) = ChainGate::new(chain);
    manager.attach_gate(Box::new(gate));
    let _ = manager.start_session();

    // The device vanishes and the rebuild fails (nothing to open), so the gate
    // stays suspended: the chain must hold, not drain into a dead device.
    factory.break_device("BuiltInMic:0", unplugged());
    let outcome = manager.report_fault(unplugged());
    assert!(
        matches!(outcome, RebuildOutcome::Failed { .. }),
        "the rebuild could not finish: {outcome:?}"
    );

    {
        let mut chain = handle.lock().expect("chain");
        assert!(chain.is_suspended(), "silenced while the device is gone");
        for _ in 0..5 {
            assert_eq!(chain.tick(&mut tick), 0, "silence, not a stutter");
            assert!(
                tick.iter().all(|sample| *sample == 0.0),
                "not one sample leaked to the absent device"
            );
        }
        assert_eq!(
            chain.buffered_ms(),
            depth_before,
            "the sentence is still there — a device blink must not eat the user's words"
        );
    }

    // The device is back. Playback resumes exactly where it stopped: the
    // samples on the far side of the outage continue the same waveform, so
    // there is no step discontinuity for the speaker to render as a click.
    factory.repair_device("BuiltInMic:0");
    clock.advance(10_000);
    let outcome = manager.report_fault(unplugged());
    assert!(matches!(outcome, RebuildOutcome::Rebuilt { .. }));

    let mut chain = handle.lock().expect("chain");
    assert!(!chain.is_suspended(), "the gate was released");
    assert_eq!(chain.tick(&mut tick), block, "and it plays again");
    let resumed_at = spoken.len() - (depth_before as usize * GRAPH_RATE_HZ as usize / 1_000);
    assert!(
        (tick[0] - spoken[resumed_at]).abs() < 1e-6,
        "sample-continuous across the rebuild: {} vs {}",
        tick[0],
        spoken[resumed_at]
    );

    let mut drained = 0usize;
    while chain.tick(&mut tick) > 0 {
        drained += 1;
        assert!(drained < 100, "the depth recovers and drains");
    }
    assert_eq!(chain.buffered_ms(), 0, "and the sentence finishes");
}

// ---------------------------------------------------------------------------
// T5.4 Test 6 — no hot-plug notification to wait for
// ---------------------------------------------------------------------------

#[test]
fn a_device_change_is_not_a_reason_to_rebuild() {
    // cpal's own documentation for `ErrorKind::DeviceChanged`: the active route
    // changed and the stream was rerouted — "the stream remains active and no
    // rebuild is required". Closing and reopening here would be the defect.
    let factory = Arc::new(ScriptedFactory::new());
    let clock = Arc::new(TestClock::new());
    let mut manager = manager(Arc::clone(&factory), Arc::clone(&clock));
    manager.start_session().expect("the session opens");
    let before = factory.log().opened.len();

    let outcome = manager.report_fault(DeviceFault::Changed {
        detail: "输出路由切到了扬声器".to_string(),
    });
    assert!(
        matches!(outcome, RebuildOutcome::Ignored { .. }),
        "a reroute is not a rebuild: {outcome:?}"
    );
    assert_eq!(factory.log().opened.len(), before, "nothing was reopened");
    assert_eq!(manager.status().rebuilds, 0);
}

#[test]
fn there_is_no_hotplug_notification_api_to_wait_for() {
    // The source-check form of the same contract: cpal 0.18 has no device
    // add/remove callback, so nothing in this layer may wait for one. The only
    // signal a device vanished is an error delivered to the stream callbacks —
    // which is why every fault here is `ErrorKind`-shaped.
    let source = include_str!("../src/audio/device.rs");
    for forbidden in [
        "hotplug",
        "hot_plug",
        "device_change_callback",
        "on_device_change(",
        "add_device_listener",
        "register_device",
    ] {
        assert!(
            !source.contains(forbidden),
            "no hot-plug notification may be assumed: found {forbidden:?}"
        );
    }
    assert!(
        source.contains("DeviceChanged"),
        "the one signal that *does* exist — routed through the error callback — is handled"
    );
    assert!(
        source.contains("ErrorKind"),
        "every fault is shaped by the backend's error kinds, not by an invented event"
    );
    assert!(
        source.contains("StreamInvalidated"),
        "the kind that means 'this stream is over' is what actually drives the rebuild"
    );
}

// ---------------------------------------------------------------------------
// T5.4 Test 7 — the session boundary
// ---------------------------------------------------------------------------

#[test]
fn stopping_the_session_stops_the_rebuilding_and_restarting_re_enumerates() {
    let factory = Arc::new(ScriptedFactory::new());
    let clock = Arc::new(TestClock::new());
    let mut manager = manager(Arc::clone(&factory), Arc::clone(&clock));
    manager.start_session().expect("the session opens");
    let opened = factory.log().opened.len();
    let enumerated = factory.log().enumerations;

    manager.stop_session();
    let status = manager.status();
    assert!(!status.in_session);
    assert!(status.input.is_none() && status.output.is_none());
    assert_eq!(
        factory.log().closed.len(),
        2,
        "both streams were closed with the session"
    );

    // A fault that lands after 停止 is a late error from a stream nobody is
    // listening to. It must not reopen anything.
    let outcome = manager.report_fault(unplugged());
    assert!(
        matches!(outcome, RebuildOutcome::Ignored { .. }),
        "no session, no rebuild: {outcome:?}"
    );
    assert_eq!(factory.log().opened.len(), opened, "nothing was reopened");
    assert_eq!(manager.status().rebuilds, 0);

    // Restarting re-reads the device list: the machine may have changed while
    // the app sat idle.
    manager.start_session().expect("the session restarts");
    assert_eq!(
        factory.log().enumerations,
        enumerated + 1,
        "a restart enumerates again — the device list is per session"
    );
    assert_eq!(factory.log().opened.len(), opened + 2);
}

// ---------------------------------------------------------------------------
// T5.4 — the real devices (manual; unplug something)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs real hardware and a human to unplug it; run with --ignored"]
fn a_real_device_unplug_is_survived() {
    // Run this by hand once per release candidate, with the app on a call:
    //   1. unplug the headset (or switch it off) mid-sentence
    //   2. the session must stay alive and the console must show the fault code
    //   3. plug it back in; the next sentence must play through the new device
    // Record what you saw in the phase SUMMARY — this is the one thing a
    // scripted backend can never prove.
    let factory = nextalk_desktop_lib::audio::device::CpalStreamFactory::shared()
        .expect("a default audio host is available");
    let clock = Arc::new(TestClock::new());
    let mut manager = DeviceManager::with_policy(factory, RebuildPolicy::default(), clock);
    let status = manager.start_session().expect("the machine has audio devices");
    assert!(
        status.input.is_some(),
        "this machine reports an input device: {status:?}"
    );
    println!("real-device probe: {status:?}");
}
