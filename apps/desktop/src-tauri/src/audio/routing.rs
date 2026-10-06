//! Stream roles and the routing profile (02-05 T5.5).
//!
//! An interview needs three audio paths at once, and they are not
//! interchangeable:
//!
//! | role | which device | where its audio goes |
//! |------|--------------|----------------------|
//! | [`StreamRole::UserMic`] | the user's own microphone | AEC → 16 kHz → the STT line that feeds the clone |
//! | [`StreamRole::Loopback`] | a virtual output device, if any | the interviewer's STT sub-line (their English) — **off by default** |
//! | [`StreamRole::Output`] | the speakers/headset | the cloned voice, and the AEC's far-end reference |
//!
//! Two rules are load-bearing here.
//!
//! **The loopback is opt-in.** It captures whatever the machine plays, which
//! during an interview includes the other side of the call — someone else's
//! voice, arriving without their knowledge (T-02-24). So the default profile
//! names no loopback device, and the loopback path does not exist until a
//! device name is configured. Its audio is treated exactly like the user's own:
//! it goes to the STT sub-line and nowhere else — never to disk, never into the
//! JSONL trace, never retained.
//!
//! **This layer only reads.** It enumerates devices and matches them by name or
//! falls back to the system default. It does not join devices into a composite
//! virtual device, it does not change the system's audio defaults, and it does
//! not guess at what a device can do from its capability flags — macOS virtual
//! drivers report both directions and happily claim to be a microphone.
//! Selection is by role and by name, and a name that is not there is a readable
//! error rather than a silent substitution. Composite devices and system audio
//! settings are Phase 3's surface, behind its own consent flow; the 只读
//! boundary is marked here so that hand knows where it starts.

use crate::audio::device::{
    DeviceFault, DeviceHandle, OpenRequest, OpenStream, RebuildPolicy, StreamDirection,
    StreamFactory,
};

/// The virtual device Phase 3 resolves the loopback to. Named here so the
/// settings page and the installer guidance cannot drift apart.
pub const LOOPBACK_DEVICE_NAME: &str = "BlackHole 2ch";

/// Where the profile lives, under the app data dir.
pub const ROUTING_CONFIG_FILE: &str = "routing.json";

// ---------------------------------------------------------------------------
// roles
// ---------------------------------------------------------------------------

/// What a stream is *for*. Distinct from [`StreamDirection`], which is only
/// what the hardware does: the user's mic and the loopback are both inputs, and
/// confusing them is the bug this type exists to make unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StreamRole {
    /// The user's own microphone — the voice that gets cloned.
    UserMic,
    /// The system's playback, captured back — the interviewer's voice.
    Loopback,
    /// The speakers or headset the cloned voice comes out of.
    Output,
}

impl StreamRole {
    /// Every role, in the order the settings page shows them.
    pub const ALL: [Self; 3] = [Self::UserMic, Self::Loopback, Self::Output];

    /// A stable machine-readable code. Never localized.
    pub fn code(self) -> &'static str {
        match self {
            Self::UserMic => "user_mic",
            Self::Loopback => "loopback",
            Self::Output => "output",
        }
    }

    /// The label the settings page shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::UserMic => "用户麦克风",
            Self::Loopback => "回采",
            Self::Output => "输出",
        }
    }

    pub fn direction(self) -> StreamDirection {
        match self {
            Self::UserMic | Self::Loopback => StreamDirection::Input,
            Self::Output => StreamDirection::Output,
        }
    }

    /// Does this role feed a capture chain?
    pub fn is_capture(self) -> bool {
        self.direction() == StreamDirection::Input
    }
}

impl std::fmt::Display for StreamRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.label())
    }
}

// ---------------------------------------------------------------------------
// the profile
// ---------------------------------------------------------------------------

/// The user's device choices, as they will be persisted.
///
/// All three empty is the default and it is a complete, working configuration:
/// the mic and the output follow the system defaults, and there is no loopback
/// path at all. Naming a device is how a role stops following the system.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RoutingProfile {
    pub mic_device: Option<String>,
    pub loopback_device: Option<String>,
    pub output_device: Option<String>,
}

impl RoutingProfile {
    /// Everything on the system defaults, with 回采 off (T-02-24).
    pub fn all_default() -> Self {
        Self::default()
    }

    pub fn with_mic(mut self, name: impl Into<String>) -> Self {
        self.mic_device = Some(name.into());
        self
    }

    /// Opt the loopback in, by name. There is no other way to enable it.
    pub fn with_loopback(mut self, name: impl Into<String>) -> Self {
        self.loopback_device = Some(name.into());
        self
    }

    pub fn with_output(mut self, name: impl Into<String>) -> Self {
        self.output_device = Some(name.into());
        self
    }

    /// The name configured for a role, when one was.
    pub fn device_name(&self, role: StreamRole) -> Option<&str> {
        match role {
            StreamRole::UserMic => self.mic_device.as_deref(),
            StreamRole::Loopback => self.loopback_device.as_deref(),
            StreamRole::Output => self.output_device.as_deref(),
        }
    }

    /// Is the loopback opted in?
    ///
    /// The answer is "a device name was configured", and nothing else. An
    /// implicit default — "it is probably BlackHole" — would turn a privacy
    /// choice into a guess.
    pub fn loopback_enabled(&self) -> bool {
        self.loopback_device.is_some()
    }

    /// Has the user configured anything at all?
    pub fn is_all_default(&self) -> bool {
        self.mic_device.is_none() && self.loopback_device.is_none() && self.output_device.is_none()
    }

    /// Read the profile from the local config file.
    ///
    /// **Not `std::fs`-free, and not meant to be.** This file is the app's own
    /// settings, not the system's audio state — the 只读 boundary above is about
    /// CoreAudio, and nothing here touches that.
    ///
    /// **Never fails.** A missing file is the default profile, and a file that
    /// will not parse is the default profile too — and the default leaves 回采
    /// off, which is the safe direction to be wrong in. Refusing to start
    /// because a settings file is corrupt would turn a display problem into a
    /// dead app.
    pub fn load_from(path: &std::path::Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    /// Write the profile to the local config file. Stays on this machine: the
    /// profile names devices the user has, which is nobody else's business.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(path, json).map_err(|error| error.to_string())
    }
}

// ---------------------------------------------------------------------------
// the resolved plan
// ---------------------------------------------------------------------------

/// One role bound to one device.
#[derive(Debug, Clone)]
pub struct ResolvedStream {
    role: StreamRole,
    device: DeviceHandle,
}

impl ResolvedStream {
    pub fn role(&self) -> StreamRole {
        self.role
    }

    pub fn device(&self) -> &DeviceHandle {
        &self.device
    }

    /// The name the host reports. Untrusted display text (T-02-22).
    pub fn device_name(&self) -> &str {
        self.device.name()
    }

    pub fn direction(&self) -> StreamDirection {
        self.role.direction()
    }
}

/// What each role resolved to, before anything is opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleStatus {
    pub role: StreamRole,
    /// The device name, when the role has one.
    pub device: Option<String>,
    /// False for a role that is deliberately not in play (回采, by default).
    pub enabled: bool,
}

/// The profile, resolved against the machine's devices.
#[derive(Debug, Clone)]
pub struct RoutingPlan {
    profile: RoutingProfile,
    streams: Vec<ResolvedStream>,
}

impl RoutingPlan {
    /// Resolve every role once, in [`StreamRole::ALL`] order.
    ///
    /// One enumeration for all three roles, so the plan cannot say the mic is
    /// one device and the output another when the list changed in between.
    /// Nothing is opened here: resolving is a read, and a plan that opened
    /// streams would make "show me the settings" cost three devices.
    pub fn resolve(
        profile: &RoutingProfile,
        factory: &dyn StreamFactory,
    ) -> Result<Self, RoutingError> {
        let devices = factory
            .enumerate()
            .map_err(|fault| RoutingError::Enumeration { fault })?;

        let mut streams = Vec::new();
        for role in StreamRole::ALL {
            if let Some(stream) = resolve_role(profile, factory, &devices, role)? {
                streams.push(stream);
            }
        }
        Ok(Self {
            profile: profile.clone(),
            streams,
        })
    }

    pub fn profile(&self) -> &RoutingProfile {
        &self.profile
    }

    pub fn stream(&self, role: StreamRole) -> Option<&ResolvedStream> {
        self.streams.iter().find(|stream| stream.role == role)
    }

    pub fn device(&self, role: StreamRole) -> Option<&DeviceHandle> {
        self.stream(role).map(ResolvedStream::device)
    }

    /// The roles that capture — the mic always, the loopback only when it is
    /// opted in. Each is its own chain: two inputs into one chain would mix the
    /// user's voice with the interviewer's and send both to the clone.
    pub fn capture_roles(&self) -> Vec<StreamRole> {
        self.streams
            .iter()
            .filter(|stream| stream.role.is_capture())
            .map(|stream| stream.role)
            .collect()
    }

    pub fn loopback_enabled(&self) -> bool {
        self.device(StreamRole::Loopback).is_some()
    }

    /// The role → device mapping the settings page shows.
    pub fn status(&self) -> Vec<RoleStatus> {
        StreamRole::ALL
            .iter()
            .map(|role| {
                let device = self.device(*role);
                RoleStatus {
                    role: *role,
                    device: device.map(|handle| handle.name().to_string()),
                    enabled: device.is_some(),
                }
            })
            .collect()
    }

    /// Open one role's stream.
    ///
    /// The build carries the same bounded timeout the rebuild path uses: a
    /// device that never answers must surface as an error, not as a hung start.
    pub fn open(
        &self,
        role: StreamRole,
        factory: &dyn StreamFactory,
    ) -> Result<Box<dyn OpenStream>, RoutingError> {
        let device = self.device(role).ok_or_else(|| {
            if role == StreamRole::Loopback {
                RoutingError::RoleDisabled { role }
            } else {
                RoutingError::NoDefaultDevice { role }
            }
        })?;
        factory
            .open(&OpenRequest {
                direction: role.direction(),
                device: Some(device),
                timeout: RebuildPolicy::default().open_timeout,
            })
            .map_err(|fault| RoutingError::Open { role, fault })
    }
}

/// One role, resolved. `None` when the role is deliberately out of play.
fn resolve_role(
    profile: &RoutingProfile,
    factory: &dyn StreamFactory,
    devices: &[DeviceHandle],
    role: StreamRole,
) -> Result<Option<ResolvedStream>, RoutingError> {
    let Some(configured) = profile.device_name(role) else {
        // 回采默认不启用: no name, no loopback path. This is the privacy
        // default, and it is the only role without a fallback — the other two
        // follow the system, which is what the user hears anyway.
        if role == StreamRole::Loopback {
            return Ok(None);
        }
        let device = factory
            .default_device(role.direction())
            .ok_or(RoutingError::NoDefaultDevice { role })?;
        return Ok(Some(ResolvedStream { role, device }));
    };

    // By name, because the name is what the user typed and what the host
    // reports. Not by capability flag: a virtual driver answers yes to
    // everything, so the flags cannot tell a loopback from a microphone.
    let wanted = configured.trim();
    let found = devices
        .iter()
        .find(|device| device.name() == wanted)
        .or_else(|| {
            devices
                .iter()
                .find(|device| device.name().eq_ignore_ascii_case(wanted))
        })
        .cloned()
        .ok_or_else(|| RoutingError::DeviceNotFound {
            role,
            name: wanted.to_string(),
        })?;
    Ok(Some(ResolvedStream {
        role,
        device: found,
    }))
}

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Why a role could not be resolved or opened.
///
/// Every variant is a sentence the settings page can show. None of them is a
/// downgrade: a role that cannot have the device it was given does not quietly
/// get another one — substituting the microphone for a missing loopback would
/// hand the clone the user's own voice while the UI said "interviewer".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingError {
    /// A device was named and the host does not report it.
    DeviceNotFound { role: StreamRole, name: String },
    /// A role follows the system default and the system has none.
    NoDefaultDevice { role: StreamRole },
    /// The role is not in play (回采 without a configured device).
    RoleDisabled { role: StreamRole },
    /// The device list itself could not be read.
    Enumeration { fault: DeviceFault },
    /// The device is there and refused to open.
    Open {
        role: StreamRole,
        fault: DeviceFault,
    },
}

impl RoutingError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DeviceNotFound { .. } => "device_not_found",
            Self::NoDefaultDevice { .. } => "no_default_device",
            Self::RoleDisabled { .. } => "role_disabled",
            Self::Enumeration { .. } => "device_enumeration_failed",
            Self::Open { .. } => "device_open_failed",
        }
    }

    /// The role this is about, when it is about one.
    pub fn role(&self) -> Option<StreamRole> {
        match self {
            Self::DeviceNotFound { role, .. }
            | Self::NoDefaultDevice { role }
            | Self::RoleDisabled { role }
            | Self::Open { role, .. } => Some(*role),
            Self::Enumeration { .. } => None,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::DeviceNotFound { role, name } => format!(
                "找不到{}设备「{name}」，请在系统音频设置中确认它已连接，或改用默认设备",
                role.label()
            ),
            Self::NoDefaultDevice { role } => {
                format!("系统没有可用的{}设备，请连接设备后重试", role.label())
            }
            Self::RoleDisabled { role } => {
                format!("{}未启用：在设置中指定设备后才会启用", role.label())
            }
            Self::Enumeration { fault } => format!("无法读取音频设备列表：{}", fault.message()),
            Self::Open { role, fault } => {
                format!("无法打开{}设备：{}", role.label(), fault.message())
            }
        }
    }
}

impl std::fmt::Display for RoutingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} [{}]", self.message(), self.code())
    }
}

impl std::error::Error for RoutingError {}

// ---------------------------------------------------------------------------
// unit tests — `cargo test routing::`
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::device::DeviceRef;

    /// A backend with two inputs and one output, and no capability flags to
    /// consult even if someone wanted to.
    struct Rig {
        handles: Vec<DeviceHandle>,
    }

    impl Rig {
        fn new() -> Self {
            Self {
                handles: vec![
                    DeviceRef::shared("BuiltInMic:0", "MacBook Pro 麦克风"),
                    DeviceRef::shared("BlackHole:3", LOOPBACK_DEVICE_NAME),
                    DeviceRef::shared("BuiltInOut:1", "MacBook Pro 扬声器"),
                ],
            }
        }
    }

    impl StreamFactory for Rig {
        fn enumerate(&self) -> Result<Vec<DeviceHandle>, DeviceFault> {
            Ok(self.handles.clone())
        }

        fn default_device(&self, direction: StreamDirection) -> Option<DeviceHandle> {
            match direction {
                StreamDirection::Input => self.handles.first().cloned(),
                StreamDirection::Output => self.handles.last().cloned(),
            }
        }

        fn open(&self, request: &OpenRequest<'_>) -> Result<Box<dyn OpenStream>, DeviceFault> {
            Ok(Box::new(Unused {
                device: request.device.cloned(),
            }))
        }
    }

    struct Unused {
        device: Option<DeviceHandle>,
    }

    impl OpenStream for Unused {
        fn device(&self) -> Option<&DeviceHandle> {
            self.device.as_ref()
        }
    }

    fn plan(profile: &RoutingProfile, rig: &Rig) -> RoutingPlan {
        RoutingPlan::resolve(profile, rig).expect("the rig has every device")
    }

    #[test]
    fn the_three_roles_are_distinct_and_their_codes_are_stable() {
        assert_eq!(StreamRole::ALL.len(), 3);
        assert_eq!(StreamRole::UserMic.direction(), StreamDirection::Input);
        assert_eq!(StreamRole::Loopback.direction(), StreamDirection::Input);
        assert_eq!(StreamRole::Output.direction(), StreamDirection::Output);
        assert!(StreamRole::UserMic.is_capture());
        assert!(StreamRole::Loopback.is_capture());
        assert!(!StreamRole::Output.is_capture());

        // The mic and the loopback share a direction and are still not the same
        // role: that distinction is the whole type.
        assert_ne!(StreamRole::UserMic, StreamRole::Loopback);
        let codes: Vec<&str> = StreamRole::ALL.iter().map(|role| role.code()).collect();
        assert_eq!(codes, vec!["user_mic", "loopback", "output"]);
    }

    #[test]
    fn an_unnamed_mic_and_output_follow_the_system_and_an_unnamed_loopback_does_not_exist() {
        let rig = Rig::new();
        let resolved = plan(&RoutingProfile::all_default(), &rig);
        assert!(resolved.profile().is_all_default());
        assert!(!resolved.loopback_enabled());
        assert_eq!(
            resolved.device(StreamRole::UserMic).map(|d| d.id()),
            Some("BuiltInMic:0")
        );
        assert_eq!(
            resolved.device(StreamRole::Output).map(|d| d.id()),
            Some("BuiltInOut:1")
        );
        assert!(resolved.device(StreamRole::Loopback).is_none());
        assert_eq!(resolved.capture_roles(), vec![StreamRole::UserMic]);

        // And opening a role that is not in play says so instead of opening
        // something else.
        let Err(error) = resolved.open(StreamRole::Loopback, &rig) else {
            panic!("no loopback device, no loopback stream");
        };
        assert_eq!(error.code(), "role_disabled");
        assert_eq!(error.role(), Some(StreamRole::Loopback));
    }

    #[test]
    fn a_configured_loopback_becomes_its_own_capture_path() {
        let rig = Rig::new();
        let resolved = plan(
            &RoutingProfile::all_default().with_loopback(LOOPBACK_DEVICE_NAME),
            &rig,
        );
        assert!(resolved.loopback_enabled());
        assert_eq!(
            resolved.capture_roles(),
            vec![StreamRole::UserMic, StreamRole::Loopback],
            "two capture paths, and the mic is not one of them twice"
        );
        let mic = resolved.device(StreamRole::UserMic).expect("mic");
        let loopback = resolved.device(StreamRole::Loopback).expect("loopback");
        assert!(!std::sync::Arc::ptr_eq(mic, loopback));
    }

    #[test]
    fn a_name_that_is_not_on_the_machine_is_an_error_that_names_it() {
        struct NoBlackHole(Rig);
        impl StreamFactory for NoBlackHole {
            fn enumerate(&self) -> Result<Vec<DeviceHandle>, DeviceFault> {
                Ok(self
                    .0
                    .enumerate()?
                    .into_iter()
                    .filter(|device| device.id() != "BlackHole:3")
                    .collect())
            }

            fn default_device(&self, direction: StreamDirection) -> Option<DeviceHandle> {
                self.0.default_device(direction)
            }

            fn open(&self, request: &OpenRequest<'_>) -> Result<Box<dyn OpenStream>, DeviceFault> {
                self.0.open(request)
            }
        }

        let rig = NoBlackHole(Rig::new());
        let error = RoutingPlan::resolve(
            &RoutingProfile::all_default().with_loopback(LOOPBACK_DEVICE_NAME),
            &rig,
        )
        .expect_err("the machine has never heard of it");
        assert_eq!(error.code(), "device_not_found");
        assert_eq!(error.role(), Some(StreamRole::Loopback));
        assert!(error.message().contains(LOOPBACK_DEVICE_NAME));
        assert!(error.message().contains("回采"));
        // Readable and actionable, and never a substitution.
        assert!(error.message().contains("默认设备"));
    }

    #[test]
    fn the_status_maps_every_role_including_the_disabled_one() {
        let rig = Rig::new();
        let resolved = plan(&RoutingProfile::all_default(), &rig);
        let status = resolved.status();
        assert_eq!(status.len(), 3, "every role is reported, even the off one");
        assert_eq!(status[0].role, StreamRole::UserMic);
        assert!(status[0].enabled);
        assert_eq!(status[0].device.as_deref(), Some("MacBook Pro 麦克风"));
        assert_eq!(status[1].role, StreamRole::Loopback);
        assert!(!status[1].enabled, "回采 is off and the page says so");
        assert_eq!(status[1].device, None);
        assert_eq!(status[2].role, StreamRole::Output);
        assert!(status[2].enabled);
    }

    #[test]
    fn an_enumeration_failure_is_reported_as_itself_and_is_not_a_role_error() {
        struct DeadRig;
        impl StreamFactory for DeadRig {
            fn enumerate(&self) -> Result<Vec<DeviceHandle>, DeviceFault> {
                Err(DeviceFault::HostGone {
                    detail: "音频服务没有响应".to_string(),
                })
            }

            fn default_device(&self, _direction: StreamDirection) -> Option<DeviceHandle> {
                None
            }

            fn open(&self, _request: &OpenRequest<'_>) -> Result<Box<dyn OpenStream>, DeviceFault> {
                unreachable!("resolve never opens")
            }
        }

        let error = RoutingPlan::resolve(&RoutingProfile::all_default(), &DeadRig)
            .expect_err("no host, no device list");
        assert_eq!(error.code(), "device_enumeration_failed");
        assert_eq!(error.role(), None, "the failure is not about one role");
        assert!(error.message().contains("音频服务没有响应"));
    }

    #[test]
    fn a_profile_round_trips_and_a_broken_one_falls_back_to_the_safe_default() {
        let dir = std::env::temp_dir().join("nextalk-routing-profile-test");
        let path = dir.join(ROUTING_CONFIG_FILE);
        let _ = std::fs::remove_file(&path);

        // Missing file: the default, which is a working configuration.
        let loaded = RoutingProfile::load_from(&path);
        assert!(loaded.is_all_default() && !loaded.loopback_enabled());

        let profile = RoutingProfile::all_default()
            .with_mic("MacBook Pro 麦克风")
            .with_loopback(LOOPBACK_DEVICE_NAME);
        profile.save_to(&path).expect("the temp dir is writable");
        assert_eq!(RoutingProfile::load_from(&path), profile);

        // Corrupt file: still the default, and still loopback-off. A settings
        // file that will not parse must not cost the user their microphone.
        std::fs::write(&path, b"{ not json").expect("write");
        let broken = RoutingProfile::load_from(&path);
        assert!(!broken.loopback_enabled());

        // A partial file (an older version's shape) keeps what it names.
        std::fs::write(&path, br#"{"loopback_device":"BlackHole 2ch"}"#).expect("write");
        let partial = RoutingProfile::load_from(&path);
        assert!(partial.loopback_enabled());
        assert_eq!(partial.mic_device, None);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
