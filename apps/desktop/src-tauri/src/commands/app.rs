use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{events, storage::ManagedStorage};

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageRootsResponse {
    pub app_data_dir: String,
    pub artifact_root: String,
    pub worktree_root: String,
    pub database_path: String,
}

#[tauri::command]
pub fn get_storage_roots(storage: State<'_, ManagedStorage>) -> StorageRootsResponse {
    StorageRootsResponse {
        app_data_dir: storage.roots.app_data_dir.to_string_lossy().to_string(),
        artifact_root: storage.roots.artifact_root.to_string_lossy().to_string(),
        worktree_root: storage.roots.worktree_root.to_string_lossy().to_string(),
        database_path: storage.roots.database_path().to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub fn emit_app_ready(app: AppHandle) -> Result<(), String> {
    events::emit_app_ready(&app).map_err(|error| error.to_string())
}
