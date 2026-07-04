mod commands;

pub mod agent;
pub mod core;
pub mod exec;
pub mod git;
pub mod safety;
pub mod storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::app::health])
        .run(tauri::generate_context!())
        .expect("failed to run Codemax desktop application");
}

