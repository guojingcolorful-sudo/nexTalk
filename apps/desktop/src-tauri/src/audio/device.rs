//! Device faults and error-driven stream rebuild (02-05 T5.4).
//!
//! During an interview the audio graph is taken away without warning: a headset
//! goes flat, a cable is nudged, another app grabs the device and will not let
//! go. The session has to survive that, and it has to survive it *without*
//! turning into a rebuild loop — the failure mode where the recovery is louder
//! than the fault (T-02-26).
//!
//! Three rules shape everything here:
//!
//! 1. **The rebuild is driven by errors, never by device notifications.**
//!    cpal 0.18 has no add/remove callback to subscribe to. The only signal that
//!    a device vanished is an error delivered to a stream's callback — and one
//!    of those kinds ([`cpal::ErrorKind::DeviceChanged`]) explicitly means *do
//!    not* rebuild. A layer that waited for a notification would wait forever;
//!    a layer that treated every error as a rebuild would interrupt audio that
//!    was working.
//! 2. **Callbacks only deliver.** [`FaultSender`] is what a stream's error
//!    callback touches: one channel send, no lock held across work, no
//!    enumeration, no device open. The rebuild itself runs inside
//!    [`DeviceManager::poll_faults`], on the session's own thread (T-02-23).
//! 3. **Bounded, always.** A short backoff, a consecutive-failure cap and a
//!    cooldown. Past the cap the session is told once ([`DeviceStatus::stalled`])
//!    and the storm stops — but the retry stays reachable after the cooldown, so
//!    a device that briefly slept does not end the interview for good.
//!
//! Device names are system-provided and untrusted (T-02-22): they are shown in
//! the settings page and matched by name, never used to decide what a device
//! *can* do (a virtual driver reports both directions and lies), and never
//! spliced into a shell command or a path.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::sim::source::{RealClock, TimeSource};

// ---------------------------------------------------------------------------
// devices
// ---------------------------------------------------------------------------

/// Which end of the graph a stream serves. Deliberately *not* the routing roles
/// (`UserMic` / `Loopback` / `Output` — see `audio::routing`, T5.5): this type
/// answers "does it capture or does it play", and the roles map onto it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StreamDirection {
    Input,
    Output,
}

impl std::fmt::Display for StreamDirection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input => write!(formatter, "输入"),
            Self::Output => write!(formatter, "输出"),
        }
    }
}

/// One audio device, resolved once.
///
/// Held behind an [`Arc`] on purpose: when the same hardware serves both
/// directions (a headset), both builds must receive the *same object*, not two
/// equal copies. macOS refuses a second open of one device, and it compares
/// identity, not equality — cpal 0.18's own `PartialEq for Device` is defined
/// on `audio_device_id`, which is the weaker check.
#[derive(Clone, Debug)]
pub struct DeviceRef {
    /// The backend's stable identifier (`"<host>:<device>"`), safe to persist
    /// and parse back. Never shown to the user.
    id: String,
    /// The backend's human-readable name: **untrusted display text**.
    name: String,
    /// The backend handle. `None` in scripted tests, where there is no backend.
    backend: Option<cpal::Device>,
}

impl DeviceRef {
    /// A device known only by name and id — the scripted-test and
    /// no-backend shape.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            backend: None,
        }
    }

    /// [`Self::new`] behind the shareable handle every manager method takes.
    pub fn shared(id: impl Into<String>, name: impl Into<String>) -> DeviceHandle {
        Arc::new(Self::new(id, name))
    }

    /// A device the backend can actually open.
    pub fn with_backend(
        id: impl Into<String>,
        name: impl Into<String>,
        backend: cpal::Device,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            backend: Some(backend),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// The name the settings page shows, verbatim. Nothing decides anything by
    /// its contents (T-02-22).
    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn backend(&self) -> Option<&cpal::Device> {
        self.backend.as_ref()
    }
}

/// Equality is the backend id: two handles are "the same device" when the host
/// says so. [`DeviceHandle`] identity (`Arc::ptr_eq`) is the stronger claim, and
/// the one the double-open guard needs.
impl PartialEq for DeviceRef {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for DeviceRef {}

/// The shared, identity-preserving device handle.
pub type DeviceHandle = Arc<DeviceRef>;

// ---------------------------------------------------------------------------
// faults
// ---------------------------------------------------------------------------

/// What went wrong with a device or a stream, in the vocabulary the rest of
/// the app reasons in.
///
/// One variant per *disposition*, not per backend constant: two kinds that call
/// for the same response share a variant, and each variant answers
/// [`Self::needs_rebuild`] the same way for every caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceFault {
    /// The device is gone — unplugged, switched off, or never there.
    NotAvailable { detail: String },
    /// The stream this fault arrived on is over; a new one has to be built.
    Invalidated { detail: String },
    /// The system rerouted the stream and it is **still running**. cpal's own
    /// words: "no rebuild is required".
    Changed { detail: String },
    /// Someone else holds the device. Retrying after the backoff may work.
    Busy { detail: String },
    /// No audio host at all — rebuilding cannot conjure one.
    HostGone { detail: String },
    /// The OS refused access (microphone privacy, realtime scheduling). Only
    /// the user can fix this; a rebuild would just fail again.
    Denied { detail: String },
    /// The backend cannot do what we asked. Our bug, not the device's.
    Unsupported { detail: String },
    /// A kind this layer does not classify. Carried verbatim so the console can
    /// still show it.
    Unexpected { kind: String, detail: String },
}

impl DeviceFault {
    /// The one mapping from the backend's kinds to ours.
    ///
    /// `ErrorKind` is `#[non_exhaustive]`, so the fallthrough is not defensive
    /// padding — a future cpal release adding a kind must land somewhere
    /// visible rather than be silently swallowed.
    pub fn from_kind(kind: cpal::ErrorKind, message: Option<&str>) -> Self {
        let detail = message.unwrap_or("").to_string();
        match kind {
            cpal::ErrorKind::DeviceNotAvailable => Self::NotAvailable { detail },
            cpal::ErrorKind::StreamInvalidated => Self::Invalidated { detail },
            cpal::ErrorKind::DeviceChanged => Self::Changed { detail },
            cpal::ErrorKind::DeviceBusy => Self::Busy { detail },
            cpal::ErrorKind::HostUnavailable => Self::HostGone { detail },
            cpal::ErrorKind::PermissionDenied | cpal::ErrorKind::RealtimeDenied => {
                Self::Denied { detail }
            }
            cpal::ErrorKind::UnsupportedConfig
            | cpal::ErrorKind::UnsupportedOperation
            | cpal::ErrorKind::InvalidInput => Self::Unsupported { detail },
            other => Self::Unexpected {
                kind: format!("{other:?}"),
                detail,
            },
        }
    }

    /// The mapping applied to a real backend error, as the callback receives it.
    pub fn from_error(error: &cpal::Error) -> Self {
        Self::from_kind(error.kind(), error.message())
    }

    /// Does this call for a new stream?
    ///
    /// The bar is "would reopening the device plausibly help". Everything that
    /// fails it is a deliberate refusal, not an omission: reopening a live,
    /// rerouted stream (`Changed`) interrupts working audio; a rebuild cannot
    /// create a missing host (`HostGone`) or grant a permission the user has
    /// withheld (`Denied`); and an unclassified kind is not a reason to tear
    /// down a stream that is demonstrably open. Only `NotAvailable`,
    /// `Invalidated` and `Busy` mean "this stream is over, build another".
    pub fn needs_rebuild(&self) -> bool {
        matches!(
            self,
            Self::NotAvailable { .. } | Self::Invalidated { .. } | Self::Busy { .. }
        )
    }

    /// A stable machine-readable code for the console and the failure-case
    /// library. Never localized — the UI copy is [`Self::message`].
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotAvailable { .. } => "device_not_available",
            Self::Invalidated { .. } => "stream_invalidated",
            Self::Changed { .. } => "device_changed",
            Self::Busy { .. } => "device_busy",
            Self::HostGone { .. } => "host_unavailable",
            Self::Denied { .. } => "permission_denied",
            Self::Unsupported { .. } => "unsupported_config",
            Self::Unexpected { .. } => "backend_error",
        }
    }

    /// The line the settings page shows. Chinese-locked, like the rest of the UI.
    pub fn message(&self) -> String {
        let head = match self {
            Self::NotAvailable { .. } => "音频设备已断开",
            Self::Invalidated { .. } => "音频流已失效，正在重建",
            Self::Changed { .. } => "音频输出已切换到其他设备",
            Self::Busy { .. } => "音频设备被其他程序占用",
            Self::HostGone { .. } => "系统音频服务不可用",
            Self::Denied { .. } => "未获得麦克风权限，请在系统设置中授权",
            Self::Unsupported { .. } => "设备不支持所需的音频格式",
            Self::Unexpected { .. } => "音频设备出现未知错误",
        };
        let detail = self.detail();
        if detail.is_empty() {
            head.to_string()
        } else {
            format!("{head}（{detail}）")
        }
    }

    /// The backend's own text, or the empty string when it did not give one.
    pub fn detail(&self) -> &str {
        match self {
            Self::NotAvailable { detail }
            | Self::Invalidated { detail }
            | Self::Changed { detail }
            | Self::Busy { detail }
            | Self::HostGone { detail }
            | Self::Denied { detail }
            | Self::Unsupported { detail }
            | Self::Unexpected { detail, .. } => detail,
        }
    }
}

impl std::fmt::Display for DeviceFault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} [{}]", self.message(), self.code())
    }
}

impl std::error::Error for DeviceFault {}

// ---------------------------------------------------------------------------
// the callback side
// ---------------------------------------------------------------------------

/// The only object a stream's error callback is allowed to touch.
///
/// `Sender::send` on an unbounded channel never blocks, which is the whole
/// reason this is a channel and not a shared `Vec` behind a mutex: a callback
/// that waited on a lock held by the thread doing the rebuild would drop audio
/// for exactly as long as the rebuild takes (T-02-23).
#[derive(Clone)]
pub struct FaultSender {
    sender: Sender<DeviceFault>,
}

impl FaultSender {
    /// Deliver one fault. The callback does this and returns.
    pub fn report(&self, fault: DeviceFault) {
        // A closed receiver means the session is gone; the fault has nowhere to
        // go and must not panic a realtime thread over it.
        let _ = self.sender.send(fault);
    }

    /// Map a backend error and deliver it — the shape a cpal `on_error`
    /// callback wants.
    pub fn report_backend(&self, error: &cpal::Error) {
        self.report(DeviceFault::from_error(error));
    }

    /// `impl FnMut(cpal::Error)` for `build_*_stream`'s error parameter.
    pub fn callback(&self) -> impl FnMut(cpal::Error) + Send + 'static {
        let sender = self.clone();
        move |error| sender.report_backend(&error)
    }
}

/// What is silenced while the device is being rebuilt.
///
/// The manager never learns what a playout chain is; it only knows that
/// something must stop feeding a device that is not there, and must start again
/// once a new one is. `PlayoutChain` implements this: suspending holds the
/// buffered sentence and outputs silence, so the user's words survive the
/// outage and resume sample-continuous (a step discontinuity is what a speaker
/// renders as a click).
pub trait RebuildGate: Send {
    fn suspend(&mut self);
    fn resume(&mut self);
}

// ---------------------------------------------------------------------------
// the rebuild policy
// ---------------------------------------------------------------------------

/// How hard the manager is allowed to try (T-02-26).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RebuildPolicy {
    /// Wait at least this long between attempts. A device that is busy because
    /// another app holds it needs time, and hammering it keeps it busy.
    pub backoff_ms: u64,
    /// Give up after this many failures *in a row* and report a session-level
    /// error. One success resets the tally.
    pub max_consecutive_failures: u32,
    /// After giving up, wait this long before a fault may try again. The cap
    /// stops the storm; this is what keeps the session recoverable, because a
    /// device can only ever be observed by trying it.
    pub cooldown_ms: u64,
    /// Passed to `build_*_stream`. cpal takes `Option<Duration>` and `None`
    /// means "wait indefinitely" — an interview must not hang on a device that
    /// will never answer.
    pub open_timeout: Duration,
}

impl Default for RebuildPolicy {
    fn default() -> Self {
        Self {
            backoff_ms: 250,
            max_consecutive_failures: 3,
            cooldown_ms: 5_000,
            open_timeout: Duration::from_secs(5),
        }
    }
}

// ---------------------------------------------------------------------------
// the backend seam
// ---------------------------------------------------------------------------

/// One stream build: which direction, which device, how long to wait.
#[derive(Debug)]
pub struct OpenRequest<'a> {
    pub direction: StreamDirection,
    /// The device to open, or `None` for the backend's own default.
    pub device: Option<&'a DeviceHandle>,
    pub timeout: Duration,
}

/// An open stream. Dropping it closes the device.
pub trait OpenStream: Send {
    /// The device this stream is bound to, when one was resolved.
    fn device(&self) -> Option<&DeviceHandle>;
}

/// Everything the manager needs from an audio backend.
///
/// Three methods, because the rebuild needs exactly three answers: what devices
/// exist, which one to use when the profile names none, and open it.
pub trait StreamFactory: Send + Sync {
    /// Re-read the device list. Never cached by the manager: the list changing
    /// is the reason a rebuild is happening.
    fn enumerate(&self) -> Result<Vec<DeviceHandle>, DeviceFault>;

    /// The backend's default device for a direction, resolved through the same
    /// identity-preserving cache [`Self::enumerate`] uses — that shared
    /// identity is what lets one headset serve both directions as one object.
    fn default_device(&self, direction: StreamDirection) -> Option<DeviceHandle>;

    /// Build one stream. `request.timeout` is the caller's bound on how long
    /// this may take.
    fn open(&self, request: &OpenRequest<'_>) -> Result<Box<dyn OpenStream>, DeviceFault>;
}

// ---------------------------------------------------------------------------
// what a report did
// ---------------------------------------------------------------------------

/// The manager's answer to a fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebuildOutcome {
    /// Nothing to do: no session, or a fault that does not call for a rebuild
    /// (a reroute, a refused permission, an unclassified kind).
    Ignored { fault: DeviceFault },
    /// The backoff has not elapsed.
    Deferred { retry_in_ms: u64 },
    /// A rebuild was attempted and did not finish. Every attempt produces one
    /// of these, including the one that reaches the cap.
    Failed { attempt: u32, fault: DeviceFault },
    /// The streams are new and live.
    Rebuilt { attempt: u32 },
    /// The cap is in force: this fault was refused **without** an attempt, and
    /// will be until the cooldown elapses. Counting these as attempts would
    /// make the cap unobservable — a storm of a thousand faults would look like
    /// a thousand rebuilds.
    Exhausted { attempts: u32, fault: DeviceFault },
}

/// What the settings page and the failure-case library read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceStatus {
    pub in_session: bool,
    pub input: Option<DeviceRef>,
    pub output: Option<DeviceRef>,
    /// Successful rebuilds this session.
    pub rebuilds: u64,
    /// Failures since the last success.
    pub consecutive_failures: u32,
    /// The session-level error, once the cap was reached. Cleared by a later
    /// successful rebuild.
    pub stalled: Option<DeviceFault>,
    /// Faults delivered by the callbacks, rebuildable or not.
    pub faults_seen: u64,
}

impl DeviceStatus {
    /// The names the settings page shows. Untrusted text, display only.
    pub fn input_name(&self) -> Option<&str> {
        self.input.as_ref().map(DeviceRef::name)
    }

    pub fn output_name(&self) -> Option<&str> {
        self.output.as_ref().map(DeviceRef::name)
    }
}

// ---------------------------------------------------------------------------
// the manager
// ---------------------------------------------------------------------------

#[derive(Default)]
struct SessionState {
    in_session: bool,
    input: Option<Box<dyn OpenStream>>,
    output: Option<Box<dyn OpenStream>>,
    input_device: Option<DeviceHandle>,
    output_device: Option<DeviceHandle>,
    rebuilds: u64,
    consecutive_failures: u32,
    next_attempt_at_ms: u64,
    retry_after_ms: u64,
    stalled: Option<DeviceFault>,
    faults_seen: u64,
}

/// Owns the session's streams and rebuilds them when the backend says so.
///
/// Every method takes `&mut self`: the manager is a session-scoped object owned
/// by the session task, which is what makes "the rebuild happens off the audio
/// callback" structural rather than a promise.
pub struct DeviceManager {
    factory: Arc<dyn StreamFactory>,
    policy: RebuildPolicy,
    clock: Arc<dyn TimeSource + Send + Sync>,
    gate: Option<Box<dyn RebuildGate>>,
    faults: Receiver<DeviceFault>,
    sender: FaultSender,
    session: SessionState,
}

impl DeviceManager {
    pub fn new(factory: Arc<dyn StreamFactory>) -> Self {
        Self::with_policy(
            factory,
            RebuildPolicy::default(),
            Arc::new(RealClock::new()),
        )
    }

    pub fn with_policy(
        factory: Arc<dyn StreamFactory>,
        policy: RebuildPolicy,
        clock: Arc<dyn TimeSource + Send + Sync>,
    ) -> Self {
        let (sender, faults) = channel();
        Self {
            factory,
            policy,
            clock,
            gate: None,
            faults,
            sender: FaultSender { sender },
            session: SessionState::default(),
        }
    }

    /// Install the object silenced during a rebuild. Callable at any time; the
    /// session wires it once, at session start.
    pub fn attach_gate(&mut self, gate: Box<dyn RebuildGate>) {
        self.gate = Some(gate);
    }

    /// A sender the stream callbacks can hold. Clone one per stream.
    ///
    /// This is the entire callback-side surface: everything else is reached
    /// only through [`Self::poll_faults`], on the session's thread.
    pub fn fault_sender(&self) -> FaultSender {
        self.sender.clone()
    }

    /// Open both streams and start the session.
    ///
    /// A failure here closes whatever opened, so a half-open session is not a
    /// state the rest of the app has to reason about.
    pub fn start_session(&mut self) -> Result<DeviceStatus, DeviceFault> {
        let (input_device, output_device) = self.resolve_devices()?;
        let input = self.open_stream(StreamDirection::Input, &input_device)?;
        let output = match self.open_stream(StreamDirection::Output, &output_device) {
            Ok(stream) => stream,
            Err(fault) => {
                drop(input);
                return Err(fault);
            }
        };

        self.session.in_session = true;
        self.session.input = Some(input);
        self.session.output = Some(output);
        self.session.input_device = input_device;
        self.session.output_device = output_device;
        self.session.consecutive_failures = 0;
        self.session.next_attempt_at_ms = 0;
        self.session.retry_after_ms = 0;
        self.session.stalled = None;
        Ok(self.status())
    }

    /// 停止: close both streams. A fault arriving afterwards is a late error
    /// from a stream nobody is listening to, and it reopens nothing.
    pub fn stop_session(&mut self) {
        self.session.in_session = false;
        self.session.input = None;
        self.session.output = None;
        self.session.input_device = None;
        self.session.output_device = None;
        self.session.consecutive_failures = 0;
        self.session.next_attempt_at_ms = 0;
        self.session.retry_after_ms = 0;
        self.session.stalled = None;
    }

    /// Drain everything the callbacks delivered and act on it.
    ///
    /// This is the method the session task calls — from its own thread, never
    /// from an audio callback (T-02-23).
    pub fn poll_faults(&mut self) -> Option<RebuildOutcome> {
        let mut last = None;
        loop {
            match self.faults.try_recv() {
                Ok(fault) => last = Some(self.report_fault(fault)),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        last
    }

    /// Act on one fault: ignore it, defer it, or rebuild for it.
    pub fn report_fault(&mut self, fault: DeviceFault) -> RebuildOutcome {
        self.session.faults_seen += 1;

        if !self.session.in_session {
            return RebuildOutcome::Ignored { fault };
        }
        if !fault.needs_rebuild() {
            return RebuildOutcome::Ignored { fault };
        }

        let now = self.clock.elapsed_ms();
        if self.session.stalled.is_some() {
            // Past the cap: one window of quiet, then the retry is allowed
            // again. The cap stops the storm, not the session.
            if now < self.session.retry_after_ms {
                return RebuildOutcome::Exhausted {
                    attempts: self.session.consecutive_failures,
                    fault,
                };
            }
            // A new window, and a new tally: the cap counts failures in the
            // window, not for the lifetime of the session.
            self.session.stalled = None;
            self.session.consecutive_failures = 0;
        }

        if now < self.session.next_attempt_at_ms {
            return RebuildOutcome::Deferred {
                retry_in_ms: self.session.next_attempt_at_ms - now,
            };
        }

        self.attempt_rebuild()
    }

    fn attempt_rebuild(&mut self) -> RebuildOutcome {
        // 1. Silence first. A device that is gone must not be fed, and the
        //    order matters: suspending after the enumeration would let a buffer
        //    drain into a device that is already offline.
        if let Some(gate) = self.gate.as_mut() {
            gate.suspend();
        }

        // 2. Re-read the device list, then resolve. The list is why we are
        //    here — a cached one would hand back the device that just left.
        let resolved = match self.resolve_devices() {
            Ok(devices) => devices,
            Err(error) => return self.record_failure(error),
        };

        // 3. Build both before swapping either: a half-rebuilt session is worse
        //    than the old streams, and a leaked second open is what makes macOS
        //    refuse the first.
        let input = match self.open_stream(StreamDirection::Input, &resolved.0) {
            Ok(stream) => stream,
            Err(error) => return self.record_failure(error),
        };
        let output = match self.open_stream(StreamDirection::Output, &resolved.1) {
            Ok(stream) => stream,
            Err(error) => {
                drop(input);
                return self.record_failure(error);
            }
        };

        self.session.input = Some(input);
        self.session.output = Some(output);
        self.session.input_device = resolved.0;
        self.session.output_device = resolved.1;
        self.session.rebuilds += 1;
        self.session.consecutive_failures = 0;
        self.session.next_attempt_at_ms = 0;
        self.session.stalled = None;
        let attempt = self.session.rebuilds as u32;

        // 4. And only now does sound resume.
        if let Some(gate) = self.gate.as_mut() {
            gate.resume();
        }
        RebuildOutcome::Rebuilt { attempt }
    }

    fn record_failure(&mut self, error: DeviceFault) -> RebuildOutcome {
        let now = self.clock.elapsed_ms();
        self.session.consecutive_failures += 1;
        let attempt = self.session.consecutive_failures;

        if attempt >= self.policy.max_consecutive_failures {
            // The cap takes force from here on: the session-level error is
            // raised, and every fault until the cooldown is refused without an
            // attempt. The gate stays suspended — there is no device to play
            // to, and a buffer draining into nothing is a buffer the user
            // loses.
            self.session.stalled = Some(error.clone());
            self.session.retry_after_ms = now + self.policy.cooldown_ms;
            self.session.next_attempt_at_ms = 0;
        } else {
            self.session.next_attempt_at_ms = now + self.policy.backoff_ms;
        }

        // Still a `Failed`: this attempt was made, and it did not finish.
        RebuildOutcome::Failed {
            attempt,
            fault: error,
        }
    }

    /// One enumeration, both directions — so the input and output lists cannot
    /// disagree, and so a device that serves both directions resolves to one
    /// object rather than two lookups that happen to be equal.
    fn resolve_devices(&self) -> Result<(Option<DeviceHandle>, Option<DeviceHandle>), DeviceFault> {
        self.factory.enumerate()?;
        let input = self.factory.default_device(StreamDirection::Input);
        let output = self.factory.default_device(StreamDirection::Output);
        if input.is_none() && output.is_none() {
            return Err(DeviceFault::NotAvailable {
                detail: "系统未报告任何音频设备".to_string(),
            });
        }
        Ok((input, output))
    }

    fn open_stream(
        &self,
        direction: StreamDirection,
        device: &Option<DeviceHandle>,
    ) -> Result<Box<dyn OpenStream>, DeviceFault> {
        self.factory.open(&OpenRequest {
            direction,
            device: device.as_ref(),
            timeout: self.policy.open_timeout,
        })
    }

    /// The device a direction is currently bound to, as the shared handle —
    /// identity comparison against this is meaningful, which is the point.
    pub fn current(&self, direction: StreamDirection) -> Option<DeviceHandle> {
        match direction {
            StreamDirection::Input => self.session.input_device.clone(),
            StreamDirection::Output => self.session.output_device.clone(),
        }
    }

    pub fn status(&self) -> DeviceStatus {
        DeviceStatus {
            in_session: self.session.in_session,
            input: self.session.input_device.as_ref().map(|d| (**d).clone()),
            output: self.session.output_device.as_ref().map(|d| (**d).clone()),
            rebuilds: self.session.rebuilds,
            consecutive_failures: self.session.consecutive_failures,
            stalled: self.session.stalled.clone(),
            faults_seen: self.session.faults_seen,
        }
    }
}

// ---------------------------------------------------------------------------
// the real backend
// ---------------------------------------------------------------------------

/// CoreAudio through cpal.
///
/// Holds the host and a cache keyed by the backend's stable device id, so the
/// *same* [`DeviceRef`] — and therefore the same `cpal::Device` — is handed back
/// on every enumeration. That cache is what makes a headset usable for both
/// directions at once on macOS.
pub struct CpalStreamFactory {
    host: cpal::Host,
    cache: Mutex<HashMap<String, DeviceHandle>>,
}

impl CpalStreamFactory {
    /// The default host. A machine with no audio host at all is a fault like
    /// any other, not a panic.
    pub fn shared() -> Result<Arc<Self>, DeviceFault> {
        Ok(Arc::new(Self {
            host: cpal::default_host(),
            cache: Mutex::new(HashMap::new()),
        }))
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, DeviceHandle>> {
        self.cache
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Resolve a backend device to a cached handle, creating it once.
    fn cached(&self, device: cpal::Device) -> Option<DeviceHandle> {
        let id = device.id().ok()?.to_string();
        let mut cache = self.lock();
        if let Some(handle) = cache.get(&id) {
            return Some(Arc::clone(handle));
        }
        // `description()` rather than `Device::to_string()`: cpal maps a
        // description failure to `fmt::Error`, and `ToString` turns that into a
        // panic — asking a half-disconnected device for its name could take the
        // process down.
        let name = device
            .description()
            .ok()
            .map(|description| description.name().to_string())
            .unwrap_or_else(|| id.clone());
        let handle = Arc::new(DeviceRef::with_backend(id.clone(), name, device));
        cache.insert(id, Arc::clone(&handle));
        Some(handle)
    }
}

impl StreamFactory for CpalStreamFactory {
    fn enumerate(&self) -> Result<Vec<DeviceHandle>, DeviceFault> {
        let mut handles = Vec::new();
        for device in self
            .host
            .devices()
            .map_err(|error| DeviceFault::from_error(&error))?
        {
            if let Some(handle) = self.cached(device) {
                handles.push(handle);
            }
        }
        Ok(handles)
    }

    fn default_device(&self, direction: StreamDirection) -> Option<DeviceHandle> {
        let device = match direction {
            StreamDirection::Input => self.host.default_input_device(),
            StreamDirection::Output => self.host.default_output_device(),
        }?;
        self.cached(device)
    }

    fn open(&self, request: &OpenRequest<'_>) -> Result<Box<dyn OpenStream>, DeviceFault> {
        let device = match request.device.and_then(|handle| handle.backend()) {
            Some(device) => device.clone(),
            None => match request.direction {
                StreamDirection::Input => self.host.default_input_device(),
                StreamDirection::Output => self.host.default_output_device(),
            }
            .ok_or_else(|| DeviceFault::NotAvailable {
                detail: format!("{}设备不存在", request.direction),
            })?,
        };

        // Every build carries the caller's timeout: cpal's `None` means "wait
        // forever", and a hung interview is the outcome this layer exists to
        // prevent.
        let stream = match request.direction {
            StreamDirection::Input => {
                let config: cpal::StreamConfig = device
                    .default_input_config()
                    .map_err(|error| DeviceFault::from_error(&error))?
                    .into();
                // A draining callback: T5.4 proves that a device can be opened,
                // timed out and rebuilt. T5.5 attaches the capture chain to the
                // same manager.
                device
                    .build_input_stream(
                        config,
                        |_data: &[f32], _| {},
                        |_error| {},
                        Some(request.timeout),
                    )
                    .map_err(|error| DeviceFault::from_error(&error))?
            }
            StreamDirection::Output => {
                let config: cpal::StreamConfig = device
                    .default_output_config()
                    .map_err(|error| DeviceFault::from_error(&error))?
                    .into();
                device
                    .build_output_stream(
                        config,
                        |data: &mut [f32], _| data.fill(0.0),
                        |_error| {},
                        Some(request.timeout),
                    )
                    .map_err(|error| DeviceFault::from_error(&error))?
            }
        };
        stream
            .play()
            .map_err(|error| DeviceFault::from_error(&error))?;
        Ok(Box::new(CpalStream {
            device: request.device.cloned(),
            _stream: stream,
        }))
    }
}

struct CpalStream {
    device: Option<DeviceHandle>,
    /// Held for its `Drop`: dropping a cpal stream is what closes the device.
    _stream: cpal::Stream,
}

impl OpenStream for CpalStream {
    fn device(&self) -> Option<&DeviceHandle> {
        self.device.as_ref()
    }
}

// ---------------------------------------------------------------------------
// unit tests — `cargo test device::`
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rebuildable_kinds_and_the_refusals_map_one_to_one() {
        let cases = [
            (cpal::ErrorKind::DeviceNotAvailable, "device_not_available"),
            (cpal::ErrorKind::StreamInvalidated, "stream_invalidated"),
            (cpal::ErrorKind::DeviceChanged, "device_changed"),
            (cpal::ErrorKind::DeviceBusy, "device_busy"),
            (cpal::ErrorKind::HostUnavailable, "host_unavailable"),
            (cpal::ErrorKind::PermissionDenied, "permission_denied"),
            (cpal::ErrorKind::RealtimeDenied, "permission_denied"),
            (cpal::ErrorKind::UnsupportedConfig, "unsupported_config"),
            (cpal::ErrorKind::UnsupportedOperation, "unsupported_config"),
        ];
        for (kind, code) in cases {
            let fault = DeviceFault::from_kind(kind, Some("后端消息"));
            assert_eq!(fault.code(), code, "{kind:?} mapped to {fault:?}");
            assert_eq!(fault.detail(), "后端消息", "the backend text is kept");
        }

        // The four kinds the plan names stay distinguishable from one another:
        // collapsing them would collapse their dispositions with them.
        let named: Vec<DeviceFault> = [
            cpal::ErrorKind::DeviceNotAvailable,
            cpal::ErrorKind::StreamInvalidated,
            cpal::ErrorKind::DeviceChanged,
            cpal::ErrorKind::DeviceBusy,
        ]
        .iter()
        .map(|kind| DeviceFault::from_kind(*kind, None))
        .collect();
        for (index, left) in named.iter().enumerate() {
            for right in named.iter().skip(index + 1) {
                assert_ne!(
                    left.code(),
                    right.code(),
                    "{left:?} and {right:?} must not be confused"
                );
            }
        }
    }

    #[test]
    fn only_the_three_kinds_that_mean_the_stream_is_over_ask_for_a_rebuild() {
        assert!(DeviceFault::from_kind(cpal::ErrorKind::DeviceNotAvailable, None).needs_rebuild());
        assert!(DeviceFault::from_kind(cpal::ErrorKind::StreamInvalidated, None).needs_rebuild());
        assert!(DeviceFault::from_kind(cpal::ErrorKind::DeviceBusy, None).needs_rebuild());

        // cpal's own documentation: "The stream remains active and no rebuild
        // is required." Reopening here would interrupt audio that works.
        assert!(!DeviceFault::from_kind(cpal::ErrorKind::DeviceChanged, None).needs_rebuild());
        // A rebuild cannot create a host or grant a permission.
        assert!(!DeviceFault::from_kind(cpal::ErrorKind::HostUnavailable, None).needs_rebuild());
        assert!(!DeviceFault::from_kind(cpal::ErrorKind::PermissionDenied, None).needs_rebuild());
        // An unclassified kind is not a reason to tear down an open stream.
        let unknown = DeviceFault::from_kind(cpal::ErrorKind::Xrun, None);
        assert!(!unknown.needs_rebuild());
        assert_eq!(unknown.code(), "backend_error");
        assert_eq!(
            unknown,
            DeviceFault::Unexpected {
                kind: "Xrun".to_string(),
                detail: String::new()
            },
            "the kind is carried verbatim, so the console can still show what happened"
        );
    }

    #[test]
    fn a_real_backend_error_maps_the_same_way_as_its_kind() {
        let error = cpal::Error::with_message(
            cpal::ErrorKind::DeviceNotAvailable,
            "The device is no longer available",
        );
        let fault = DeviceFault::from_error(&error);
        assert_eq!(fault.code(), "device_not_available");
        assert_eq!(fault.detail(), "The device is no longer available");
        assert!(fault.needs_rebuild());
        assert!(fault.message().contains("音频设备已断开"));
    }

    #[test]
    fn a_fault_without_a_backend_message_still_reads_as_a_sentence() {
        let fault = DeviceFault::from_kind(cpal::ErrorKind::PermissionDenied, None);
        assert_eq!(fault.message(), "未获得麦克风权限，请在系统设置中授权");
        assert_eq!(
            fault.detail(),
            "",
            "an absent message is empty, not the string \"None\""
        );
    }

    #[test]
    fn the_policy_is_bounded_in_every_direction() {
        let policy = RebuildPolicy::default();
        assert!(
            policy.backoff_ms > 0,
            "a zero backoff is a busy loop against a device that needs time"
        );
        assert!(policy.max_consecutive_failures >= 1, "at least one try");
        assert!(
            policy.cooldown_ms >= policy.backoff_ms,
            "the cooldown after giving up is longer than a retry, or it is not a cooldown"
        );
        assert!(
            policy.open_timeout > Duration::ZERO,
            "cpal's None means 'wait forever'; an interview must not"
        );
    }

    #[test]
    fn a_fault_sender_delivers_to_the_manager_channel_without_blocking() {
        let mut manager = DeviceManager::new(Arc::new(ProbeFactory));
        let sender = manager.fault_sender();
        let mut callback = sender.callback();

        // The callback's whole job: take the backend error and put it down.
        callback(cpal::Error::with_message(
            cpal::ErrorKind::StreamInvalidated,
            "stream was invalidated",
        ));
        callback(cpal::Error::with_message(
            cpal::ErrorKind::DeviceChanged,
            "route changed",
        ));
        assert_eq!(
            manager.faults.try_recv().expect("delivered").code(),
            "stream_invalidated",
            "the callback carried the backend's own kind across the boundary"
        );

        // No session is open, so acting on the second one decides nothing — but
        // it is delivered, seen, and drained exactly once. Delivery and action
        // are different counts on purpose: the callback may deliver a fault the
        // session has already stopped caring about.
        assert!(matches!(
            manager.poll_faults(),
            Some(RebuildOutcome::Ignored { .. })
        ));
        assert!(manager.faults.try_recv().is_err(), "the queue drained");
        assert_eq!(
            manager.status().faults_seen,
            1,
            "only what the session acted on was counted"
        );
    }

    #[test]
    fn a_closed_receiver_does_not_panic_the_callback() {
        let (sender, receiver) = channel();
        let faults = FaultSender { sender };
        drop(receiver);
        // A realtime thread must survive the session going away underneath it.
        faults.report(DeviceFault::from_kind(
            cpal::ErrorKind::DeviceNotAvailable,
            None,
        ));
    }

    /// A factory that reports one device and opens nothing — enough for the
    /// unit tests above, which never reach [`StreamFactory::open`].
    struct ProbeFactory;

    impl StreamFactory for ProbeFactory {
        fn enumerate(&self) -> Result<Vec<DeviceHandle>, DeviceFault> {
            Ok(vec![DeviceRef::shared("probe:0", "探针设备")])
        }

        fn default_device(&self, _direction: StreamDirection) -> Option<DeviceHandle> {
            Some(DeviceRef::shared("probe:0", "探针设备"))
        }

        fn open(&self, _request: &OpenRequest<'_>) -> Result<Box<dyn OpenStream>, DeviceFault> {
            Err(DeviceFault::NotAvailable {
                detail: "the unit tests never open a stream".to_string(),
            })
        }
    }
}
