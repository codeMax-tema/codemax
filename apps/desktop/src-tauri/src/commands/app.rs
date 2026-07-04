use serde::Serialize;

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

