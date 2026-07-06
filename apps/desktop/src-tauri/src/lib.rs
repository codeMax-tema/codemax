mod commands;
mod events;

pub mod agent;
pub mod core;
pub mod exec;
pub mod git;
pub mod safety;
pub mod storage;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let managed_storage = storage::ManagedStorage::initialize(app_data_dir)?;
            app.manage(managed_storage);
            app.manage(exec::CommandRunRegistry::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::health,
            commands::app::ping,
            commands::app::emit_app_ready,
            commands::exec::execute_task_command,
            commands::exec::cancel_task_command,
            commands::exec::read_task_command_log,
            commands::exec::summarize_task_command_log,
            commands::exec::cleanup_expired_task_logs,
            commands::repository::select_repository_path,
            commands::repository::validate_repository_path,
            commands::repository::get_repository_current_branch,
            commands::repository::get_repository_dirty_status,
            commands::worktree::create_task_branch,
            commands::worktree::create_task_worktree,
            commands::worktree::get_task_worktree_status,
            commands::worktree::cleanup_task_worktree
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Codemax desktop application");
}
