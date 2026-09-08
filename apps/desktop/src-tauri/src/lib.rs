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

mod lan;
mod sim;
mod state;

use lan::server::SessionStatus;
use state::SessionState;

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
/// spawns the SimSource scheduler. Re-entrant calls while a script is playing
/// are rejected.
#[tauri::command]
fn start_session(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionState>,
) -> Result<(), String> {
    let current = state.session_status();
    if current == SessionStatus::Listening || current == SessionStatus::Generating {
        return Err("a simulated session is already running".into());
    }
    state.reset_timeline();
    let _ = state.set_session_status(SessionStatus::Listening);
    let _ = app.emit(
        "session_status",
        serde_json::json!({ "session": "listening" }),
    );
    sim::source::spawn_scheduler(state.inner().clone(), app);
    Ok(())
}

/// `stop_session` — ends the session (status -> ended) while keeping the
/// timeline for review.
#[tauri::command]
fn stop_session(
    app: tauri::AppHandle,
    state: tauri::State<'_, SessionState>,
) -> Result<(), String> {
    let _ = state.set_session_status(SessionStatus::Ended);
    let _ = app.emit("session_status", serde_json::json!({ "session": "ended" }));
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_pairing_info,
            start_session,
            stop_session
        ])
        .setup(|app| {
            let state = SessionState::new(LAN_PORT);
            app.manage(state.clone());

            // LAN server (pairing WS + static H5, no-store). A bind failure
            // must not take the desktop app down — log and continue.
            let server_state = state.clone();
            let teleprompter_dist = lan::server::teleprompter_dist_path();
            tauri::async_runtime::spawn(async move {
                match tokio::net::TcpListener::bind(("0.0.0.0", LAN_PORT)).await {
                    Ok(listener) => {
                        let router = lan::server::router(server_state, teleprompter_dist);
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
