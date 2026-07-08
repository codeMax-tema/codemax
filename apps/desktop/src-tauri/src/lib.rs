mod commands;
mod events;

pub mod agent;
pub mod core;
pub mod exec;
pub mod git;
pub mod privacy;
pub mod safety;
pub mod secrets;
pub mod storage;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let managed_storage = storage::ManagedStorage::initialize(app_data_dir)?;
            let agent_app_data_dir = managed_storage.roots.app_data_dir.clone();
            app.manage(managed_storage);
            app.manage(exec::CommandRunRegistry::default());
            app.manage(agent::AgentService::with_app_data_dir(agent_app_data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::agent::start_agent_service,
            commands::agent::stop_agent_service,
            commands::agent::get_agent_service_status,
            commands::agent::check_agent_health,
            commands::agent::create_agent_task,
            commands::agent::get_agent_task_state,
            commands::agent::advance_agent_task,
            commands::agent::submit_agent_validation_result,
            commands::agent::run_agent_validation_cycle,
            commands::app::health,
            commands::app::ping,
            commands::app::get_storage_roots,
            commands::app::get_app_setting,
            commands::app::set_app_setting,
            commands::app::get_startup_health,
            commands::app::get_storage_usage,
            commands::app::cleanup_storage,
            commands::app::emit_app_ready,
            commands::approvals::list_pending_approvals,
            commands::approvals::list_task_approvals,
            commands::approvals::decide_approval,
            commands::delivery::generate_task_delivery,
            commands::diff::generate_task_diff,
            commands::exec::execute_task_command,
            commands::exec::cancel_task_command,
            commands::exec::read_task_command_log,
            commands::exec::summarize_task_command_log,
            commands::exec::cleanup_expired_task_logs,
            commands::merge::prepare_task_merge,
            commands::merge::merge_task,
            commands::models::get_model_config,
            commands::models::save_model_config,
            commands::models::test_model_connection,
            commands::privacy::active_profile,
            commands::privacy::privacy_ledger_summary,
            commands::privacy::run_contract,
            commands::privacy::token_budget_summary,
            commands::repository::select_repository_path,
            commands::repository::validate_repository_path,
            commands::repository::get_repository_current_branch,
            commands::repository::get_repository_dirty_status,
            commands::s12_evidence::generate_task_proof_pack,
            commands::s12_evidence::get_delivery_review_state,
            commands::s12_evidence::record_quality_gate_result,
            commands::s12_evidence::override_quality_gate,
            commands::tasks::create_task_record,
            commands::tasks::list_tasks,
            commands::tasks::get_task_record,
            commands::tasks::get_task_detail,
            commands::worktree::create_task_branch,
            commands::worktree::create_task_worktree,
            commands::worktree::get_task_worktree_status,
            commands::worktree::cleanup_task_worktree
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Codemax desktop application");
}
