// NexTalk desktop — Tauri application entry.
//
// Phase 1 (01-02 walking skeleton): two fixed frameless transparent windows
// (console 340x680 visible, dual 860x680 hidden) per tauri.conf.json. Commands
// get_pairing_info / start_session / stop_session are wired to the real
// SessionState + LAN server + SimSource in 01-02 Task 2 (lan/, sim/, state.rs).

use serde::Serialize;
use tauri::Emitter;

/// Placeholder pairing info returned by the Task-1 stub; the real value comes
/// from SessionState::pairing_url() once state.rs lands (Task 2).
#[derive(Serialize)]
struct PairingInfo {
    url: String,
    port: u16,
}

/// Stub — real implementation (LAN IP + session token) lands in Task 2.
#[tauri::command]
fn get_pairing_info() -> PairingInfo {
    PairingInfo {
        url: "http://127.0.0.1:8787/?token=pending".to_string(),
        port: 8787,
    }
}

/// Stub — Task 2 wires this to SessionState + the SimSource scheduler.
#[tauri::command]
async fn start_session() -> Result<(), String> {
    Ok(())
}

/// Stub — Task 2 wires this to SessionState (status -> ended, timeline kept).
#[tauri::command]
async fn stop_session() -> Result<(), String> {
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
            // Initial session status so webviews start in the idle state
            // (event name "session_status", payload { session: 'idle' }).
            let _ = app.emit("session_status", serde_json::json!({ "session": "idle" }));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
