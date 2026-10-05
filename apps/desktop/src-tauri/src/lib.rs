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

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_pairing_info,
            start_session,
            stop_session,
            usage_summary,
            start_enrollment_recording,
            stop_enrollment_recording
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
