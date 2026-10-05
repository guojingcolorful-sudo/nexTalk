//! The local voice profile (02-04 T4.2).
//!
//! One JSON document under `<app data>/voice/profile.json` records which
//! cloned voice the TTS stage should use. It contains the speaker id and the
//! metadata needed to manage it — **never a credential** (T-02-17) and never
//! the audio itself (that stays under `enroll/`, T-02-16).
//!
//! Privacy posture for the whole file:
//!
//! - the profile is written owner-only (`0600`), like the samples;
//! - [`VoiceStore::delete`] is a *wipe*: the record, every take under
//!   `enroll/`, and therefore any orphan left by a failed retrain. Deletion
//!   must not depend on the record being readable — a corrupt profile is
//!   exactly when a user needs the escape hatch;
//! - [`VoiceStore::resolve`] never panics and never blocks the app: a missing
//!   profile is the normal first run, a corrupt one is a warning plus the
//!   preset voice, so a session can always start.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::pipeline::stages::{SpeakerId, VoiceRef};

use super::capture::enrollment_dir;

/// Where the profile lives inside the app data directory.
pub const VOICE_DIR: &str = "voice";
/// The profile document's file name.
pub const PROFILE_FILE_NAME: &str = "profile.json";
/// The 火山 resource a cloned voice is synthesised with (D-11).
pub const CLONE_RESOURCE_ID: &str = "seed-icl-2.0";
/// The preset voice used while no clone profile is ready (02-04 T4.3).
pub const DEFAULT_PRESET_VOICE: &str = "zh_female_vv_uranus_bigtts";
/// Preset override, mirroring `VolcCredentials::VOICE_VAR` (the TTS stage's
/// voice variable) so one env var moves both paths.
pub const PRESET_VOICE_VAR: &str = "VOLC_TTS_VOICE";
/// The only status a profile can carry and be used: training finished.
pub const PROFILE_STATUS_READY: &str = "ready";

/// One training generation, as preserved in the rollback history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceTake {
    pub speaker_id: String,
    pub resource_id: String,
    pub created_at: String,
    pub sample_path: String,
    pub duration_s: f64,
    pub status: String,
}

/// The current voice profile.
///
/// `previous` is newest-first: `previous[0]` is the voice a failed retrain
/// rolls back to (T4.4). History is kept so a retrain that trains a *worse*
/// clone can be undone without losing the sample ids.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceProfile {
    pub speaker_id: String,
    pub resource_id: String,
    pub created_at: String,
    pub sample_path: String,
    pub duration_s: f64,
    pub status: String,
    #[serde(default)]
    pub previous: Vec<VoiceTake>,
}

impl VoiceProfile {
    /// This profile as a history entry (what lands in the next generation's
    /// `previous` list).
    pub fn take(&self) -> VoiceTake {
        VoiceTake {
            speaker_id: self.speaker_id.clone(),
            resource_id: self.resource_id.clone(),
            created_at: self.created_at.clone(),
            sample_path: self.sample_path.clone(),
            duration_s: self.duration_s,
            status: self.status.clone(),
        }
    }

    /// The voice a failed retrain would roll back to.
    pub fn rollback_anchor(&self) -> Option<&VoiceTake> {
        self.previous.first()
    }

    pub fn is_ready(&self) -> bool {
        self.status == PROFILE_STATUS_READY
    }
}

/// What can go wrong reading or writing the profile.
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceStoreError {
    Io { path: String, detail: String },
    Invalid { path: String, detail: String },
}

impl VoiceStoreError {
    fn io(path: &Path, error: impl fmt::Display) -> Self {
        Self::Io {
            path: path.display().to_string(),
            detail: error.to_string(),
        }
    }

    fn invalid(path: &Path, error: impl fmt::Display) -> Self {
        Self::Invalid {
            path: path.display().to_string(),
            detail: error.to_string(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "io",
            Self::Invalid { .. } => "invalid",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Io { .. } => "音色档案读写失败".to_string(),
            Self::Invalid { .. } => "音色档案格式无效".to_string(),
        }
    }
}

impl fmt::Display for VoiceStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, detail } => write!(formatter, "voice store I/O at {path}: {detail}"),
            Self::Invalid { path, detail } => {
                write!(formatter, "invalid voice profile at {path}: {detail}")
            }
        }
    }
}

impl std::error::Error for VoiceStoreError {}

/// What [`VoiceStore::delete`] removed.
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteOutcome {
    /// Whether `profile.json` existed and was removed.
    pub profile_removed: bool,
    /// Every `*.wav` under `enroll/` that was removed.
    pub samples: Vec<PathBuf>,
}

/// Which voice to use, plus an optional warning for the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceResolution {
    pub voice: VoiceRef,
    pub warning: Option<String>,
}

/// The voice profile on disk, rooted at the app data directory.
#[derive(Debug, Clone)]
pub struct VoiceStore {
    root: PathBuf,
}

impl VoiceStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn profile_path(&self) -> PathBuf {
        self.root.join(VOICE_DIR).join(PROFILE_FILE_NAME)
    }

    /// Read the profile. `Ok(None)` is the first-run state, not an error.
    pub fn load(&self) -> Result<Option<VoiceProfile>, VoiceStoreError> {
        let path = self.profile_path();
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(VoiceStoreError::io(&path, error)),
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| VoiceStoreError::invalid(&path, error))
    }

    /// Write the profile owner-only (`0600`).
    pub fn save(&self, profile: &VoiceProfile) -> Result<(), VoiceStoreError> {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let path = self.profile_path();
        let dir = path.parent().expect("profile path has a parent");
        std::fs::create_dir_all(dir).map_err(|error| VoiceStoreError::io(dir, error))?;
        let body = serde_json::to_vec_pretty(profile)
            .map_err(|error| VoiceStoreError::invalid(&path, error))?;

        let mut file =
            std::fs::File::create(&path).map_err(|error| VoiceStoreError::io(&path, error))?;
        file.write_all(&body)
            .map_err(|error| VoiceStoreError::io(&path, error))?;
        file.sync_all()
            .map_err(|error| VoiceStoreError::io(&path, error))?;
        // The profile names the user's voice id — private data (T-02-16).
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| VoiceStoreError::io(&path, error))?;
        Ok(())
    }

    /// Remove the profile and every take under `enroll/`.
    ///
    /// Deliberately independent of the profile being parseable: a corrupt
    /// record must not trap the user's biometric data on disk. Idempotent.
    pub fn delete(&self) -> Result<DeleteOutcome, VoiceStoreError> {
        let profile_path = self.profile_path();
        let profile_removed = match std::fs::remove_file(&profile_path) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(VoiceStoreError::io(&profile_path, error)),
        };

        let enroll = enrollment_dir(&self.root);
        let entries = match std::fs::read_dir(&enroll) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(DeleteOutcome {
                    profile_removed,
                    samples: Vec::new(),
                })
            }
            Err(error) => return Err(VoiceStoreError::io(&enroll, error)),
        };

        let mut samples = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| VoiceStoreError::io(&enroll, error))?;
            let path = entry.path();
            let is_wav = path.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("wav");
            if !is_wav {
                continue;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => samples.push(path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(VoiceStoreError::io(&path, error)),
            }
        }
        samples.sort();
        Ok(DeleteOutcome {
            profile_removed,
            samples,
        })
    }

    /// The voice to synthesise with: the clone when a ready profile exists,
    /// the preset otherwise. `preset_voice` is the caller's resolved preset
    /// (see [`preset_from_lookup`]).
    pub fn resolve(&self, preset_voice: &str) -> VoiceResolution {
        let preset = || VoiceRef::Preset(preset_voice.to_string());
        match self.load() {
            Ok(Some(profile)) if profile.is_ready() => VoiceResolution {
                voice: VoiceRef::Clone(SpeakerId::new(profile.speaker_id)),
                warning: None,
            },
            // A profile exists but training never finished: usable state, but
            // the user should know why the clone is not speaking.
            Ok(Some(_)) => VoiceResolution {
                voice: preset(),
                warning: Some("音色档案尚未就绪，已使用预置音色".to_string()),
            },
            // First run: the preset is the expected voice, not a warning.
            Ok(None) => VoiceResolution {
                voice: preset(),
                warning: None,
            },
            Err(error) => {
                eprintln!("[enroll] voice profile unreadable, using preset: {error}");
                VoiceResolution {
                    voice: preset(),
                    warning: Some("音色档案已损坏，已回退到预置音色，可重新训练修复".to_string()),
                }
            }
        }
    }
}

/// The preset voice, honouring the `VOLC_TTS_VOICE` override.
pub fn preset_from_lookup(lookup: impl Fn(&str) -> Option<String>) -> String {
    lookup(PRESET_VOICE_VAR)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_PRESET_VOICE.to_string())
}

/// RFC3339 UTC (`2026-10-05T12:34:56Z`) from a `SystemTime`.
///
/// Hand-rolled rather than pulling in `chrono`/`time`: one format, one call
/// site, and a new dependency would need its own legitimacy gate.
pub fn rfc3339_utc(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0) as i64;
    let days = seconds.div_euclid(86_400);
    let remaining = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (remaining / 3_600, (remaining % 3_600) / 60, remaining % 60);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Days since 1970-01-01 → (year, month, day), Howard Hinnant's algorithm.
fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let shifted = days_since_epoch + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("nextalk-02-04-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp root");
        root
    }

    fn ready_profile(root: &Path) -> VoiceProfile {
        VoiceProfile {
            speaker_id: "S_unit".to_string(),
            resource_id: CLONE_RESOURCE_ID.to_string(),
            created_at: "2026-10-05T00:00:00Z".to_string(),
            sample_path: enrollment_dir(root)
                .join("take-1.wav")
                .display()
                .to_string(),
            duration_s: 90.0,
            status: PROFILE_STATUS_READY.to_string(),
            previous: Vec::new(),
        }
    }

    #[test]
    fn save_load_round_trips_and_the_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("round-trip");
        let store = VoiceStore::new(&root);
        assert_eq!(store.load().expect("fresh load"), None, "first run is None");

        let profile = ready_profile(&root);
        store.save(&profile).expect("save");
        assert_eq!(store.load().expect("load"), Some(profile));
        let mode = std::fs::metadata(store.profile_path())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn a_corrupt_profile_is_an_invalid_error_not_a_panic() {
        let root = temp_root("corrupt");
        let store = VoiceStore::new(&root);
        std::fs::create_dir_all(root.join(VOICE_DIR)).expect("voice dir");
        std::fs::write(store.profile_path(), "{ nope").expect("write");
        let error = store.load().expect_err("unparseable");
        assert_eq!(error.code(), "invalid");
    }

    #[test]
    fn delete_wipes_the_record_and_every_wav_under_enroll() {
        let root = temp_root("wipe");
        let store = VoiceStore::new(&root);
        store.save(&ready_profile(&root)).expect("save");
        let enroll = enrollment_dir(&root);
        std::fs::create_dir_all(&enroll).expect("enroll dir");
        let take = enroll.join("take-1.wav");
        std::fs::write(&take, b"not really audio").expect("write sample");
        // Anything that is not a take is left alone.
        std::fs::write(enroll.join("notes.txt"), b"keep me").expect("write notes");

        let outcome = store.delete().expect("delete");
        assert!(outcome.profile_removed);
        assert_eq!(outcome.samples, vec![take.clone()]);
        assert!(!take.exists() && !store.profile_path().exists());
        assert!(enroll.join("notes.txt").exists(), "only takes are wiped");

        let again = store.delete().expect("idempotent");
        assert!(!again.profile_removed && again.samples.is_empty());
    }

    #[test]
    fn resolve_prefers_a_ready_clone_and_warns_when_the_record_is_broken() {
        let root = temp_root("resolve");
        let store = VoiceStore::new(&root);

        let first_run = store.resolve(DEFAULT_PRESET_VOICE);
        assert_eq!(
            first_run.voice,
            VoiceRef::Preset(DEFAULT_PRESET_VOICE.to_string())
        );
        assert!(first_run.warning.is_none());

        let mut profile = ready_profile(&root);
        profile.status = "training".to_string();
        store.save(&profile).expect("save");
        let pending = store.resolve(DEFAULT_PRESET_VOICE);
        assert_eq!(
            pending.voice,
            VoiceRef::Preset(DEFAULT_PRESET_VOICE.to_string())
        );
        assert!(pending.warning.expect("warning").contains("预置"));

        std::fs::write(store.profile_path(), "broken").expect("corrupt");
        let broken = store.resolve(DEFAULT_PRESET_VOICE);
        assert!(broken.warning.expect("warning").contains("损坏"));
    }

    #[test]
    fn rfc3339_formats_known_epochs() {
        assert_eq!(
            rfc3339_utc(UNIX_EPOCH),
            "1970-01-01T00:00:00Z",
            "the epoch itself"
        );
        assert_eq!(
            rfc3339_utc(UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000)),
            "2023-11-14T22:13:20Z"
        );
    }

    #[test]
    fn preset_lookup_honours_the_override() {
        assert_eq!(
            preset_from_lookup(|_| None),
            DEFAULT_PRESET_VOICE.to_string()
        );
        assert_eq!(
            preset_from_lookup(|name| (name == PRESET_VOICE_VAR).then(|| "custom".to_string())),
            "custom".to_string()
        );
        assert_eq!(
            preset_from_lookup(|name| (name == PRESET_VOICE_VAR).then(String::new)),
            DEFAULT_PRESET_VOICE.to_string(),
            "a blank override falls back"
        );
    }
}
