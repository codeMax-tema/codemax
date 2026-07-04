use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const APP_READY_EVENT: &str = "codemax://app-ready";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppReadyPayload {
    pub service: &'static str,
    pub version: &'static str,
}

pub fn emit_app_ready(app: &AppHandle) -> tauri::Result<()> {
    app.emit(
        APP_READY_EVENT,
        AppReadyPayload {
            service: "codemax-desktop",
            version: env!("CARGO_PKG_VERSION"),
        },
    )
}
