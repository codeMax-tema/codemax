use serde::Serialize;
use tauri::AppHandle;

use crate::events;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub service: &'static str,
    pub status: &'static str,
    pub version: &'static str,
}

#[tauri::command]
pub fn health() -> HealthResponse {
    HealthResponse {
        service: "codemax-desktop",
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {
    pub message: &'static str,
}

#[tauri::command]
pub fn ping() -> PingResponse {
    PingResponse { message: "pong" }
}

#[tauri::command]
pub fn emit_app_ready(app: AppHandle) -> Result<(), String> {
    events::emit_app_ready(&app).map_err(|error| error.to_string())
}
