// NexTalk desktop — Tauri application entry.
//
// Phase 1 (01-02 walking skeleton): two fixed frameless transparent windows
// (console 340x680 visible, dual 860x680 hidden) per tauri.conf.json; a LAN
// server (pairing-as-auth WebSocket + static H5 bundle, no-store) that the
// phone teleprompter joins by scanning the QR; and a deterministic SimSource
// that plays one simulated round over both transports (Tauri `session` emit
// + WS broadcast) so desktop and H5 observe the same timeline.

use serde::Serialize;
use tauri::{Emitter, Manager};

// Public so `tests/session_integration.rs` (a separate crate) can drive the
// real state + LAN server the way the demo does.
pub mod audio;
pub mod enroll;
pub mod lan;
pub mod pipeline;
pub mod sim;
pub mod state;
pub mod trace;

use enroll::capture::{
    finish_capture, CaptureBackend, CaptureError, CaptureGuard, CaptureSession, CpalCapture,
};
use enroll::register::{
    next_speaker_id, train_voice_clone as run_voice_clone_training, CloneTrainError, CloneTrainer,
};
use enroll::voice_store::{preset_from_lookup, VoiceProfile, VoiceStore, VoiceStoreError};
use pipeline::stages::VoiceRef;
use state::SessionState;
use trace::jsonl::UsageSummary;
use trace::{CostReport, MONTHLY_QUOTA_MINUTES};

/// LAN port the phone H5 teleprompter connects to (fixed for the skeleton).
const LAN_PORT: u16 = 8787;

/// Pairing payload for the console QR (get_pairing_info).
#[derive(Serialize)]
struct PairingInfo {
    url: String,
    port: u16,
}

/// `get_pairing_info` — QR code source: the pairing URL with the 128-bit
/// session token (T-01-01). A new token is drawn per process start, so a
/// stale QR can never pair against a later run.
#[tauri::command]
fn get_pairing_info(state: tauri::State<'_, SessionState>) -> PairingInfo {
    PairingInfo {
        url: state.pairing_url(),
        port: state.port(),
    }
}

/// `start_session` — resets the timeline, marks the session listening and
/// spawns the SimSource scheduler against the real clock. Re-entrant calls
/// while a script is playing are rejected; the returned epoch is the
/// scheduler's cancellation ticket (stop/restart bumps it).
#[tauri::command]
fn start_session(state: tauri::State<'_, SessionState>) -> Result<(), String> {
    let epoch = state.start_session()?;
    sim::source::spawn_scheduler(state.inner().clone(), epoch, sim::source::RealClock::new());
    Ok(())
}

/// `stop_session` — ends the session (status -> ended, scheduler cancelled)
/// while keeping the timeline for review.
#[tauri::command]
fn stop_session(state: tauri::State<'_, SessionState>) -> Result<(), String> {
    state.stop_session();
    Ok(())
}

/// The month's staged usage and cost for the diagnostics panel (T3.7/D-13).
/// A pure read of the local JSONL traces (D-05): the frontend renders these
/// numbers and never recomputes them.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageReport {
    usage: UsageSummary,
    cost: CostReport,
    quota_minutes: u64,
    used_minutes: u64,
}

/// `usage_summary` — current-month aggregation for DiagnosticsPage.
#[tauri::command]
fn usage_summary(state: tauri::State<'_, SessionState>) -> UsageReport {
    let usage = state.usage_summary();
    let cost = CostReport::from_usage(&usage);
    UsageReport {
        used_minutes: usage.used_minutes().round() as u64,
        quota_minutes: MONTHLY_QUOTA_MINUTES,
        usage,
        cost,
    }
}

// ---------------------------------------------------------------------------
// Voice enrollment (02-04 T4.1): capture a take into <app data>/enroll/
// ---------------------------------------------------------------------------

/// The enrollment recorder is its own managed state — deliberately separate
/// from [`SessionState`]. The sample is biometric data (T-02-16): it must not
/// ride along the session/JSONL path, and stopping a session must never touch
/// a take that is being recorded for the voice profile.
#[derive(Default)]
struct EnrollmentState {
    active: std::sync::Mutex<Option<ActiveEnrollment>>,
}

struct ActiveEnrollment {
    backend: Box<dyn CaptureBackend>,
    session: CaptureSession,
}

/// The structured error the frontend renders: stable code + locked Chinese copy.
#[derive(Serialize)]
struct CaptureErrorDto {
    code: String,
    message: String,
}

impl From<CaptureError> for CaptureErrorDto {
    fn from(error: CaptureError) -> Self {
        Self {
            code: error.code().to_string(),
            message: error.message(),
        }
    }
}

fn poisoned_state() -> CaptureErrorDto {
    CaptureErrorDto {
        code: "internal".to_string(),
        message: "录音状态异常，请重启应用后重试".to_string(),
    }
}

/// `start_enrollment_recording` — open the default input device and begin a take.
#[tauri::command]
fn start_enrollment_recording(
    state: tauri::State<'_, EnrollmentState>,
) -> Result<(), CaptureErrorDto> {
    let mut active = state.active.lock().map_err(|_| poisoned_state())?;
    if active.is_some() {
        return Err(CaptureError::AlreadyRecording.into());
    }
    let mut backend = CpalCapture::new()?;
    let session = CaptureSession::begin(&mut backend)?;
    *active = Some(ActiveEnrollment {
        backend: Box::new(backend),
        session,
    });
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureResultDto {
    path: String,
    duration_s: f64,
    silence_ratio: f32,
    bytes: u64,
}

/// `enrollment_level` — the newest input peak of the running take (0.0–1.0),
/// polled by the wizard's level meter. `None` means no take is active.
#[tauri::command]
fn enrollment_level(
    state: tauri::State<'_, EnrollmentState>,
) -> Result<Option<f32>, CaptureErrorDto> {
    let active = state.active.lock().map_err(|_| poisoned_state())?;
    Ok(active
        .as_ref()
        .map(|enrollment| enrollment.session.level().latest()))
}

/// `stop_enrollment_recording` — stop the take, run the guard checks, and write
/// `<app data>/enroll/<sessionId>.wav` (16 kHz mono PCM16, owner-only).
#[tauri::command]
fn stop_enrollment_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, EnrollmentState>,
) -> Result<CaptureResultDto, CaptureErrorDto> {
    let mut active = state
        .active
        .lock()
        .map_err(|_| poisoned_state())?
        .take()
        .ok_or(CaptureError::NoActiveTake)?;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| CaptureError::Io(error.to_string()))?;
    let session_id = format!(
        "take-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_millis())
            .unwrap_or(0)
    );

    let result = finish_capture(
        active.session,
        active.backend.as_mut(),
        &CaptureGuard::default(),
        &root,
        &session_id,
    )?;

    Ok(CaptureResultDto {
        path: result.path.display().to_string(),
        duration_s: result.duration_s,
        silence_ratio: result.silence_ratio,
        bytes: result.bytes,
    })
}

// ---------------------------------------------------------------------------
// Voice enrollment (02-04 T4.2/T4.3): training, the profile, the preview state
// ---------------------------------------------------------------------------

/// Stable error shape for the voice commands (mirrors [`CaptureErrorDto`]).
#[derive(Serialize)]
struct VoiceCommandErrorDto {
    code: String,
    message: String,
}

impl From<CloneTrainError> for VoiceCommandErrorDto {
    fn from(error: CloneTrainError) -> Self {
        Self {
            code: error.code().to_string(),
            message: error.message(),
        }
    }
}

impl From<VoiceStoreError> for VoiceCommandErrorDto {
    fn from(error: VoiceStoreError) -> Self {
        Self {
            code: error.code().to_string(),
            message: error.message(),
        }
    }
}

impl VoiceCommandErrorDto {
    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal".to_string(),
            message: message.into(),
        }
    }

    fn busy() -> Self {
        Self {
            code: "busy".to_string(),
            message: "正在录音，请先停止录音再删除音色档案".to_string(),
        }
    }
}

/// The resolved voice for the badge: 我的克隆 vs 预置.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VoiceRefDto {
    kind: String,
    name: String,
}

impl VoiceRefDto {
    fn of(voice: &VoiceRef) -> Self {
        match voice {
            VoiceRef::Clone(speaker) => Self {
                kind: "clone".to_string(),
                name: speaker.as_str().to_string(),
            },
            VoiceRef::Preset(name) => Self {
                kind: "preset".to_string(),
                name: name.clone(),
            },
        }
    }
}

/// What the enrollment wizard and the badge read (T4.3): the profile as saved
/// (never the audio, never a credential), the voice that will actually speak,
/// and a warning when the profile is unusable.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VoiceStatusDto {
    profile: Option<VoiceProfile>,
    voice: VoiceRefDto,
    warning: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteVoiceDto {
    profile_removed: bool,
    samples: Vec<String>,
}

fn app_root(app: &tauri::AppHandle) -> Result<std::path::PathBuf, VoiceCommandErrorDto> {
    app.path()
        .app_data_dir()
        .map_err(|error| VoiceCommandErrorDto::internal(format!("应用数据目录不可用：{error}")))
}

/// The current status: stored profile + resolved voice. Never fails — a
/// corrupt profile is a warning plus the preset voice, so the app can always
/// render.
fn voice_status(store: &VoiceStore) -> VoiceStatusDto {
    let preset = preset_from_lookup(|name| std::env::var(name).ok());
    let profile = store.load().ok().flatten();
    let resolution = store.resolve(&preset);
    VoiceStatusDto {
        profile,
        voice: VoiceRefDto::of(&resolution.voice),
        warning: resolution.warning,
    }
}

/// `train_voice_clone` — upload one recorded take and store the resulting
/// voice profile. Emits `enrollment_train` progress (`training` → `ready` /
/// `failed`) on the desktop window only.
#[tauri::command]
async fn train_voice_clone(
    app: tauri::AppHandle,
    sample_path: String,
    transcript: String,
) -> Result<VoiceStatusDto, VoiceCommandErrorDto> {
    let root = app_root(&app)?;
    let store = VoiceStore::new(&root);
    let lookup = |name: &str| std::env::var(name).ok();
    let trainer = CloneTrainer::from_lookup(lookup);
    let speaker_id = next_speaker_id(lookup);

    let _ = app.emit(
        "enrollment_train",
        serde_json::json!({ "stage": "training", "speakerId": speaker_id }),
    );

    match run_voice_clone_training(
        &trainer,
        &store,
        std::path::Path::new(&sample_path),
        &transcript,
        &speaker_id,
    )
    .await
    {
        Ok(_profile) => {
            let status = voice_status(&store);
            let _ = app.emit(
                "enrollment_train",
                serde_json::json!({ "stage": "ready", "speakerId": speaker_id }),
            );
            Ok(status)
        }
        Err(error) => {
            let dto: VoiceCommandErrorDto = error.into();
            let _ = app.emit(
                "enrollment_train",
                serde_json::json!({
                    "stage": "failed",
                    "code": dto.code,
                    "message": dto.message,
                }),
            );
            Err(dto)
        }
    }
}

/// `get_voice_profile` — the stored profile, the resolved voice and any
/// warning, for the wizard and the 「当前音色」 badge.
#[tauri::command]
fn get_voice_profile(app: tauri::AppHandle) -> Result<VoiceStatusDto, VoiceCommandErrorDto> {
    let root = app_root(&app)?;
    Ok(voice_status(&VoiceStore::new(&root)))
}

/// `delete_voice_profile` — wipe the profile and every enrollment take
/// (T-02-16). Refused while a take is being recorded: the wipe must not race
/// the capture that is still writing.
#[tauri::command]
fn delete_voice_profile(
    app: tauri::AppHandle,
    state: tauri::State<'_, EnrollmentState>,
) -> Result<DeleteVoiceDto, VoiceCommandErrorDto> {
    let active = state
        .active
        .lock()
        .map_err(|_| VoiceCommandErrorDto::internal("录音状态异常，请重启应用后重试"))?;
    if active.is_some() {
        return Err(VoiceCommandErrorDto::busy());
    }
    drop(active);

    let root = app_root(&app)?;
    let outcome = VoiceStore::new(&root)
        .delete()
        .map_err(VoiceCommandErrorDto::from)?;
    Ok(DeleteVoiceDto {
        profile_removed: outcome.profile_removed,
        samples: outcome
            .samples
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
    })
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_pairing_info,
            start_session,
            stop_session,
            usage_summary,
            start_enrollment_recording,
            enrollment_level,
            stop_enrollment_recording,
            train_voice_clone,
            get_voice_profile,
            delete_voice_profile
        ])
        .setup(|app| {
            let state = SessionState::new(LAN_PORT);
            // The state carries the handle the event mirror needs (phone_count
            // and session emits); cargo tests never call this, which is why
            // every emit path is a no-op without it.
            state.set_app_handle(app.handle().clone());
            // Durable traces live under the app data dir (D-05/D-06): JSONL
            // records under `traces/<date>/` — nothing here leaves the
            // machine. A resolution failure only turns tracing off.
            match app.path().app_data_dir() {
                Ok(dir) => state.set_trace_dir(dir.join("traces")),
                Err(err) => eprintln!("[trace] app data dir unavailable, tracing off: {err}"),
            }
            app.manage(state.clone());
            app.manage(EnrollmentState::default());

            // LAN server (pairing WS + static H5, no-store). A bind failure
            // must not take the desktop app down — log and continue. The app
            // handle lets a phone-initiated 开始模拟会话 reveal the dual window.
            let server_state = state.clone();
            let teleprompter_dist = lan::server::teleprompter_dist_path();
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match tokio::net::TcpListener::bind(("0.0.0.0", LAN_PORT)).await {
                    Ok(listener) => {
                        let router =
                            lan::server::router(server_state, teleprompter_dist, Some(app_handle));
                        if let Err(err) = axum::serve(listener, router).await {
                            eprintln!("[lan] server error: {err}");
                        }
                    }
                    Err(err) => eprintln!("[lan] failed to bind port {LAN_PORT}: {err}"),
                }
            });

            // Initial session status so webviews start in the idle state.
            let _ = app.emit("session_status", serde_json::json!({ "session": "idle" }));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
