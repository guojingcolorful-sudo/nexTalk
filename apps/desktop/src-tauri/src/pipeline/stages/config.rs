//! Credentials, endpoints and the vendor routing table (T2.1).
//!
//! Every credential comes from the process environment — the same variables
//! `tools/vendor-experiments/.env.example` documents — and never from a file we
//! ship or a literal in the source. Two rules make that safe to rely on:
//!
//! 1. [`Secret`] has no way to print itself: `Debug` and `Display` render
//!    `***`, so a credential can be held inside a struct that derives `Debug`
//!    without leaking it through a log line or a panic message.
//! 2. A missing **or blank** value is a configuration error that lists the
//!    variable *names* only. Blank matters: the shipped `.env` template has
//!    `XFYUN_API_KEY=""` for every unfilled key, and `set -a; source .env`
//!    exports those empty strings as if they were values.
//!
//! [`Endpoints`] exists so tests can point a client at a loopback mock without
//! a network round trip to the vendor (T2.6): each URL is overridable through
//! `NEXTALK_*_URL`, and the defaults below are the production endpoints.

use std::collections::BTreeSet;
use std::fmt;

use super::error::StageError;
use super::traits::{SpeakerId, VoiceRef};

/// Which vendor sits at each stage (02-RESEARCH routing table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StageRole {
    /// The user's Chinese speech — feeds the clone TTS.
    UserStt,
    /// The interviewer's English speech — subtitles and copilot only.
    InterviewerStt,
    /// Chinese fragment → English fragment.
    Translation,
    /// English text → the user's cloned voice.
    Tts,
}

/// Which half of the conversation a stage belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    /// Spoken by the user (Chinese in, English out).
    User,
    /// Spoken by the interviewer (English, never re-voiced).
    Interviewer,
}

impl StageRole {
    pub const ALL: [StageRole; 4] = [
        StageRole::UserStt,
        StageRole::InterviewerStt,
        StageRole::Translation,
        StageRole::Tts,
    ];

    /// The vendor this role is routed to (D-01).
    pub fn provider(self) -> &'static str {
        match self {
            StageRole::UserStt => "xfyun",
            StageRole::InterviewerStt => "deepgram",
            StageRole::Translation => "deepseek",
            StageRole::Tts => "volc",
        }
    }

    /// Default model/product identifier; a client replaces it with what the
    /// vendor actually reported when it can (Deepgram's `model_info`).
    pub fn model(self) -> &'static str {
        match self {
            StageRole::UserStt => "iat",
            StageRole::InterviewerStt => "nova-3",
            StageRole::Translation => "deepseek-chat",
            StageRole::Tts => "seed-icl-2.0",
        }
    }

    pub fn track(self) -> Track {
        match self {
            StageRole::InterviewerStt => Track::Interviewer,
            _ => Track::User,
        }
    }

    /// **The T2.3 invariant**: only the user's line may be re-voiced. The
    /// interviewer's English is heard by the interviewer already — sending it
    /// to TTS would speak over them.
    pub fn feeds_tts(self) -> bool {
        self.track() == Track::User
    }
}

/// A credential that cannot be printed.
///
/// `PartialEq` is deliberately absent: comparing secrets is how they end up in
/// a test assertion message.
#[derive(Clone, Default)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The only way to reach the value. Call sites are few by design: a
    /// handshake header, a signature input.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_blank(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// One env var, blank-tolerant.
fn read(lookup: &impl Fn(&str) -> Option<String>, name: &'static str) -> Option<String> {
    lookup(name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn require_complete(missing: &[&'static str]) -> Result<(), StageError> {
    if missing.is_empty() {
        Ok(())
    } else {
        Err(StageError::missing_config(missing))
    }
}

fn env_lookup(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

// ---------------------------------------------------------------------------
// 讯飞 iat
// ---------------------------------------------------------------------------

/// 讯飞 streaming dictation (`iat`): the credentials the HMAC-SHA256 handshake
/// needs, and nothing else.
#[derive(Debug, Clone)]
pub struct XfyunCredentials {
    pub app_id: String,
    pub api_key: Secret,
    pub api_secret: Secret,
}

impl XfyunCredentials {
    pub const APP_ID_VAR: &'static str = "XFYUN_APP_ID";
    pub const API_KEY_VAR: &'static str = "XFYUN_API_KEY";
    pub const API_SECRET_VAR: &'static str = "XFYUN_API_SECRET";

    pub fn from_env() -> Result<Self, StageError> {
        Self::from_lookup(env_lookup)
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, StageError> {
        let mut missing = Vec::new();
        let app_id = read(&lookup, Self::APP_ID_VAR).record_missing(&mut missing, Self::APP_ID_VAR);
        let api_key =
            read(&lookup, Self::API_KEY_VAR).record_missing(&mut missing, Self::API_KEY_VAR);
        let api_secret =
            read(&lookup, Self::API_SECRET_VAR).record_missing(&mut missing, Self::API_SECRET_VAR);
        require_complete(&missing)?;
        Ok(Self {
            app_id: app_id.unwrap_or_default(),
            api_key: Secret::new(api_key.unwrap_or_default()),
            api_secret: Secret::new(api_secret.unwrap_or_default()),
        })
    }
}

// ---------------------------------------------------------------------------
// Deepgram
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DeepgramCredentials {
    pub api_key: Secret,
}

impl DeepgramCredentials {
    pub const API_KEY_VAR: &'static str = "DEEPGRAM_API_KEY";

    pub fn from_env() -> Result<Self, StageError> {
        Self::from_lookup(env_lookup)
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, StageError> {
        let mut missing = Vec::new();
        let api_key =
            read(&lookup, Self::API_KEY_VAR).record_missing(&mut missing, Self::API_KEY_VAR);
        require_complete(&missing)?;
        Ok(Self {
            api_key: Secret::new(api_key.unwrap_or_default()),
        })
    }
}

// ---------------------------------------------------------------------------
// DeepSeek
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DeepseekCredentials {
    pub api_key: Secret,
}

impl DeepseekCredentials {
    pub const API_KEY_VAR: &'static str = "DEEPSEEK_API_KEY";

    pub fn from_env() -> Result<Self, StageError> {
        Self::from_lookup(env_lookup)
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, StageError> {
        let mut missing = Vec::new();
        let api_key =
            read(&lookup, Self::API_KEY_VAR).record_missing(&mut missing, Self::API_KEY_VAR);
        require_complete(&missing)?;
        Ok(Self {
            api_key: Secret::new(api_key.unwrap_or_default()),
        })
    }
}

// ---------------------------------------------------------------------------
// 火山 Seed-TTS 2.0 / ICL 2.0
// ---------------------------------------------------------------------------

/// 火山 TTS credentials. `app_id` identifies the account in the request body
/// (`user.uid`); the access token is the `X-Api-Key` handshake header.
#[derive(Debug, Clone)]
pub struct VolcCredentials {
    pub app_id: String,
    pub access_token: Secret,
    /// `X-Api-Resource-Id`: `seed-icl-2.0` clones, `seed-tts-2.0` presets.
    pub resource_id: String,
    /// Default preset voice (`VOLC_TTS_VOICE`) — the fallback without a clone.
    pub preset_voice: Option<String>,
    /// The cloned speaker (`VOLC_CLONE_SPEAKER_ID`), e.g. `S_xxxx`.
    pub clone_speaker: Option<SpeakerId>,
}

impl VolcCredentials {
    pub const APP_ID_VAR: &'static str = "VOLC_TTS_APP_ID";
    pub const ACCESS_TOKEN_VAR: &'static str = "VOLC_TTS_ACCESS_TOKEN";
    pub const RESOURCE_VAR: &'static str = "VOLC_TTS_RESOURCE";
    pub const VOICE_VAR: &'static str = "VOLC_TTS_VOICE";
    pub const SPEAKER_VAR: &'static str = "VOLC_CLONE_SPEAKER_ID";

    /// The clone resource: this phase's primary path is the user's own voice.
    pub const DEFAULT_RESOURCE_ID: &'static str = "seed-icl-2.0";

    pub fn from_env() -> Result<Self, StageError> {
        Self::from_lookup(env_lookup)
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, StageError> {
        let mut missing = Vec::new();
        let app_id = read(&lookup, Self::APP_ID_VAR).record_missing(&mut missing, Self::APP_ID_VAR);
        let access_token = read(&lookup, Self::ACCESS_TOKEN_VAR)
            .record_missing(&mut missing, Self::ACCESS_TOKEN_VAR);
        require_complete(&missing)?;
        Ok(Self {
            app_id: app_id.unwrap_or_default(),
            access_token: Secret::new(access_token.unwrap_or_default()),
            resource_id: read(&lookup, Self::RESOURCE_VAR)
                .unwrap_or_else(|| Self::DEFAULT_RESOURCE_ID.to_string()),
            preset_voice: read(&lookup, Self::VOICE_VAR),
            clone_speaker: read(&lookup, Self::SPEAKER_VAR).map(SpeakerId::new),
        })
    }

    /// The voice to synthesise with when the caller has no explicit choice:
    /// the clone when one is configured, otherwise the preset.
    pub fn default_voice(&self) -> Option<VoiceRef> {
        self.clone_speaker
            .clone()
            .map(VoiceRef::Clone)
            .or_else(|| self.preset_voice.clone().map(VoiceRef::Preset))
    }
}

/// Small helper that keeps "record the name of the missing var" out of the
/// `let ... = read(...)` lines above.
trait RecordMissing {
    fn record_missing(self, missing: &mut Vec<&'static str>, name: &'static str) -> Self;
}

impl RecordMissing for Option<String> {
    fn record_missing(self, missing: &mut Vec<&'static str>, name: &'static str) -> Self {
        if self.is_none() {
            missing.push(name);
        }
        self
    }
}

// ---------------------------------------------------------------------------
// endpoints
// ---------------------------------------------------------------------------

/// A vendor base URL. Overridable for tests and for regional routing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint(String);

impl Endpoint {
    pub fn new(url: impl Into<String>) -> Self {
        Self(url.into())
    }

    /// The env override when set and non-blank, else the production default.
    pub fn from_lookup(
        lookup: &impl Fn(&str) -> Option<String>,
        name: &'static str,
        default: &str,
    ) -> Self {
        Self(read(lookup, name).unwrap_or_else(|| default.to_string()))
    }

    pub fn url(&self) -> &str {
        &self.0
    }

    /// Append a path to the base URL, tolerating a trailing slash.
    pub fn join(&self, path: &str) -> String {
        format!("{}{}", self.0.trim_end_matches('/'), path)
    }
}

/// Every vendor endpoint in one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub xfyun_ws: Endpoint,
    pub deepgram_ws: Endpoint,
    pub deepseek_http: Endpoint,
    pub volc_ws: Endpoint,
}

impl Endpoints {
    pub const XFYUN_WS_DEFAULT: &'static str = "wss://iat-api.xfyun.cn/v2/iat";
    pub const DEEPGRAM_WS_DEFAULT: &'static str = "wss://api.deepgram.com/v1/listen";
    pub const DEEPSEEK_HTTP_DEFAULT: &'static str = "https://api.deepseek.com";
    pub const VOLC_WS_DEFAULT: &'static str =
        "wss://openspeech.bytedance.com/api/v3/tts/unidirectional/stream";

    pub const XFYUN_WS_VAR: &'static str = "NEXTALK_XFYUN_WS_URL";
    pub const DEEPGRAM_WS_VAR: &'static str = "NEXTALK_DEEPGRAM_WS_URL";
    pub const DEEPSEEK_HTTP_VAR: &'static str = "NEXTALK_DEEPSEEK_URL";
    pub const VOLC_WS_VAR: &'static str = "NEXTALK_VOLC_WS_URL";

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            xfyun_ws: Endpoint::from_lookup(&lookup, Self::XFYUN_WS_VAR, Self::XFYUN_WS_DEFAULT),
            deepgram_ws: Endpoint::from_lookup(
                &lookup,
                Self::DEEPGRAM_WS_VAR,
                Self::DEEPGRAM_WS_DEFAULT,
            ),
            deepseek_http: Endpoint::from_lookup(
                &lookup,
                Self::DEEPSEEK_HTTP_VAR,
                Self::DEEPSEEK_HTTP_DEFAULT,
            ),
            volc_ws: Endpoint::from_lookup(&lookup, Self::VOLC_WS_VAR, Self::VOLC_WS_DEFAULT),
        }
    }

    pub fn from_env() -> Self {
        Self::from_lookup(env_lookup)
    }

    /// Production defaults, no environment involved (tests and docs).
    pub fn defaults() -> Self {
        Self::from_lookup(|_| None)
    }

    /// Point the 讯飞 client at a loopback mock (T2.6 harnesses).
    pub fn with_xfyun_ws(mut self, url: impl Into<String>) -> Self {
        self.xfyun_ws = Endpoint::new(url);
        self
    }

    pub fn with_deepgram_ws(mut self, url: impl Into<String>) -> Self {
        self.deepgram_ws = Endpoint::new(url);
        self
    }

    pub fn with_deepseek_http(mut self, url: impl Into<String>) -> Self {
        self.deepseek_http = Endpoint::new(url);
        self
    }

    pub fn with_volc_ws(mut self, url: impl Into<String>) -> Self {
        self.volc_ws = Endpoint::new(url);
        self
    }
}

impl Default for Endpoints {
    fn default() -> Self {
        Self::defaults()
    }
}

// ---------------------------------------------------------------------------
// the routing table
// ---------------------------------------------------------------------------

/// Everything the four stages need to run, or one error naming what is absent.
#[derive(Debug, Clone)]
pub struct RoutingConfig {
    pub xfyun: XfyunCredentials,
    pub deepgram: DeepgramCredentials,
    pub deepseek: DeepseekCredentials,
    pub volc: VolcCredentials,
    pub endpoints: Endpoints,
}

impl RoutingConfig {
    pub fn from_env() -> Result<Self, StageError> {
        Self::from_lookup(env_lookup)
    }

    /// Missing variables are reported **together**, and only by name: the
    /// onboarding screen lists them all at once instead of one restarts-loop
    /// per key (GOV-18).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, StageError> {
        let mut missing: Vec<&'static str> = Vec::new();
        // Collect every stage's gaps in one pass: an aggregated list is the
        // whole point, so a failure here is not short-circuited per vendor.
        let xfyun = XfyunCredentials::from_lookup(&lookup);
        let deepgram = DeepgramCredentials::from_lookup(&lookup);
        let deepseek = DeepseekCredentials::from_lookup(&lookup);
        let volc = VolcCredentials::from_lookup(&lookup);
        for name in [
            XfyunCredentials::APP_ID_VAR,
            XfyunCredentials::API_KEY_VAR,
            XfyunCredentials::API_SECRET_VAR,
            DeepgramCredentials::API_KEY_VAR,
            DeepseekCredentials::API_KEY_VAR,
            VolcCredentials::APP_ID_VAR,
            VolcCredentials::ACCESS_TOKEN_VAR,
        ] {
            if read(&lookup, name).is_none() {
                missing.push(name);
            }
        }
        require_complete(&missing)?;
        Ok(Self {
            xfyun: xfyun?,
            deepgram: deepgram?,
            deepseek: deepseek?,
            volc: volc?,
            endpoints: Endpoints::from_lookup(&lookup),
        })
    }

    /// The endpoints alone, with no credentials — what the mock harnesses need.
    pub fn endpoints_only(endpoints: Endpoints) -> Self {
        Self {
            xfyun: XfyunCredentials {
                app_id: String::new(),
                api_key: Secret::default(),
                api_secret: Secret::default(),
            },
            deepgram: DeepgramCredentials {
                api_key: Secret::default(),
            },
            deepseek: DeepseekCredentials {
                api_key: Secret::default(),
            },
            volc: VolcCredentials {
                app_id: String::new(),
                access_token: Secret::default(),
                resource_id: VolcCredentials::DEFAULT_RESOURCE_ID.to_string(),
                preset_voice: None,
                clone_speaker: None,
            },
            endpoints,
        }
    }

    /// The roles this configuration can run right now (all of them, or it
    /// would not have constructed).
    pub fn roles(&self) -> BTreeSet<StageRole> {
        StageRole::ALL.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_lookup(name: &str) -> Option<String> {
        match name {
            "XFYUN_APP_ID" => Some("appid-test".to_string()),
            "XFYUN_API_KEY" => Some("key-test".to_string()),
            "XFYUN_API_SECRET" => Some("secret-test".to_string()),
            "DEEPGRAM_API_KEY" => Some("dg-test".to_string()),
            "DEEPSEEK_API_KEY" => Some("ds-test".to_string()),
            "VOLC_TTS_APP_ID" => Some("volc-appid".to_string()),
            "VOLC_TTS_ACCESS_TOKEN" => Some("volc-token".to_string()),
            _ => None,
        }
    }

    #[test]
    fn routing_table_matches_the_research_decisions() {
        assert_eq!(StageRole::UserStt.provider(), "xfyun");
        assert_eq!(StageRole::InterviewerStt.provider(), "deepgram");
        assert_eq!(StageRole::Translation.provider(), "deepseek");
        assert_eq!(StageRole::Tts.provider(), "volc");
        assert_eq!(StageRole::InterviewerStt.model(), "nova-3");
    }

    #[test]
    fn the_interviewer_line_never_reaches_tts() {
        assert!(StageRole::UserStt.feeds_tts());
        assert!(StageRole::Translation.feeds_tts());
        assert!(!StageRole::InterviewerStt.feeds_tts(), "T2.3 invariant");
        assert_eq!(StageRole::InterviewerStt.track(), Track::Interviewer);
    }

    #[test]
    fn a_complete_environment_builds_and_blanks_count_as_missing() {
        assert!(RoutingConfig::from_lookup(complete_lookup).is_ok());

        let blanked = |name: &str| {
            if name == "DEEPGRAM_API_KEY" {
                Some("   ".to_string()) // `XFYUN_API_KEY=""` in the template
            } else {
                complete_lookup(name)
            }
        };
        let error = RoutingConfig::from_lookup(blanked).expect_err("blank is not a value");
        assert!(error.to_string().contains("DEEPGRAM_API_KEY"));
    }

    #[test]
    fn every_missing_name_is_reported_at_once() {
        let error = RoutingConfig::from_lookup(|_| None).expect_err("nothing configured");
        let message = error.to_string();
        for name in [
            "XFYUN_APP_ID",
            "XFYUN_API_KEY",
            "XFYUN_API_SECRET",
            "DEEPGRAM_API_KEY",
            "DEEPSEEK_API_KEY",
            "VOLC_TTS_APP_ID",
            "VOLC_TTS_ACCESS_TOKEN",
        ] {
            assert!(message.contains(name), "{name} missing from {message}");
        }
        assert_eq!(error.kind, super::super::error::ErrorKind::Config);
    }

    #[test]
    fn credentials_never_render_their_value() {
        let credentials = XfyunCredentials::from_lookup(complete_lookup).expect("configured");
        let rendered = format!("{credentials:?}");
        assert!(!rendered.contains("secret-test"), "{rendered}");
        assert!(!rendered.contains("key-test"), "{rendered}");
        assert!(
            rendered.contains("appid-test"),
            "the app id is not a secret"
        );
        assert_eq!(credentials.api_secret.to_string(), "***");

        let config = RoutingConfig::from_lookup(complete_lookup).expect("configured");
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("volc-token"), "{rendered}");
        assert!(!rendered.contains("ds-test"), "{rendered}");
    }

    #[test]
    fn endpoints_default_and_override() {
        let defaults = Endpoints::defaults();
        assert_eq!(defaults.xfyun_ws.url(), "wss://iat-api.xfyun.cn/v2/iat");
        assert_eq!(
            defaults.deepseek_http.join("/chat/completions"),
            "https://api.deepseek.com/chat/completions"
        );

        let overridden = Endpoints::from_lookup(|name| match name {
            "NEXTALK_DEEPGRAM_WS_URL" => Some("ws://127.0.0.1:9/v1/listen".to_string()),
            "NEXTALK_VOLC_WS_URL" => Some("   ".to_string()), // blank → default
            _ => None,
        });
        assert_eq!(overridden.deepgram_ws.url(), "ws://127.0.0.1:9/v1/listen");
        assert_eq!(overridden.volc_ws.url(), Endpoints::VOLC_WS_DEFAULT);

        let joined = overridden.deepgram_ws.join("/extra");
        assert_eq!(joined, "ws://127.0.0.1:9/v1/listen/extra");
    }

    #[test]
    fn volc_defaults_to_the_clone_resource_and_reports_its_voice() {
        let credentials = VolcCredentials::from_lookup(complete_lookup).expect("configured");
        assert_eq!(credentials.resource_id, "seed-icl-2.0");
        assert_eq!(credentials.default_voice(), None, "no voice configured yet");

        let with_speaker = VolcCredentials::from_lookup(|name| match name {
            "VOLC_CLONE_SPEAKER_ID" => Some("S_abc123".to_string()),
            other => complete_lookup(other),
        })
        .expect("configured");
        assert_eq!(
            with_speaker.default_voice(),
            Some(VoiceRef::Clone(SpeakerId::new("S_abc123")))
        );

        let preset_only = VolcCredentials::from_lookup(|name| match name {
            "VOLC_TTS_VOICE" => Some("zh_female_vv_uranus_bigtts".to_string()),
            other => complete_lookup(other),
        })
        .expect("configured");
        assert_eq!(
            preset_only.default_voice(),
            Some(VoiceRef::Preset("zh_female_vv_uranus_bigtts".to_string()))
        );
    }

    #[test]
    fn single_vendor_credentials_build_independently() {
        // A client that needs one vendor must not be blocked by another's gap.
        let only_deepgram = |name: &str| match name {
            "DEEPGRAM_API_KEY" => Some("dg-test".to_string()),
            _ => None,
        };
        assert!(DeepgramCredentials::from_lookup(only_deepgram).is_ok());
        assert!(RoutingConfig::from_lookup(only_deepgram).is_err());
    }
}
