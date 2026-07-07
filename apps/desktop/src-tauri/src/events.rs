use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::exec::{CommandExecutionResult, CommandFinishedEvent, CommandOutputEvent};

pub const APP_READY_EVENT: &str = "codemax://app-ready";
pub const COMMAND_OUTPUT_EVENT: &str = "codemax://command-output";
pub const COMMAND_FINISHED_EVENT: &str = "codemax://command-finished";

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

pub fn emit_command_output(app: &AppHandle, payload: CommandOutputEvent) -> tauri::Result<()> {
    app.emit(COMMAND_OUTPUT_EVENT, payload)
}

pub fn emit_command_finished(app: &AppHandle, result: CommandExecutionResult) -> tauri::Result<()> {
    app.emit(COMMAND_FINISHED_EVENT, CommandFinishedEvent { result })
}
