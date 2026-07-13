use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;
use uuid::Uuid;

use crate::{
    commands::s12_evidence::DeliveryReviewState,
    core::error::{AppResult, CommandError},
    git::{self, GitError},
    privacy::{
        record_context_observation, record_token_budget_observation, sanitize_for_model_context,
        ContextObservation, SanitizedContent, TokenBudgetObservation,
    },
    storage::{
        AgentEventRecord, AgentEventRepository, AgentSessionRecord, AgentSessionRepository,
        ApprovalRecord, ApprovalRepository, ArtifactFileRecord, ArtifactRecord, ArtifactRepository,
        CommandRunRecord, CommandRunRepository, ManagedStorage, MergeRecord, MergeRecordRepository,
        NewAgentEvent, NewAgentSession, NewArtifactFile, NewRunContract, NewTask, NewTodo,
        PersonalProfileRepository, RunContractRepository, StorageError, TaskRecord, TaskRepository,
        TodoRecord, TodoRepository, ValidationRoundRecord, ValidationRoundRepository,
    },
    workspace,
};

const DEFAULT_TASK_TYPE: &str = "custom";
const DEFAULT_TASK_STATUS: &str = "queued";
const DEFAULT_TASK_LIST_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy)]
struct RunContractOverrides<'a> {
    mode: Option<&'a str>,
    reasoning_effort: Option<&'a str>,
    permission_level: Option<&'a str>,
    network_policy: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedWorkspaceStrategy {
    GitWorktree,
    InitializeGit,
    IsolatedCopy,
    DirectOriginal,
}

#[derive(Debug)]
struct PreparedTaskWorkspace {
    path: String,
    task_branch: Option<String>,
    target_branch: String,
    kind: &'static str,
    source_path: String,
    original_write_authorized: bool,
    estimated_bytes: i64,
    initialized_git: bool,
}

fn resolve_workspace_strategy(
    is_git_repository: bool,
    work_mode: Option<&str>,
    requested_strategy: Option<&str>,
    original_write_authorized: bool,
) -> AppResult<ResolvedWorkspaceStrategy> {
    let work_mode = work_mode.unwrap_or("coding").trim();
    if !matches!(work_mode, "daily" | "coding") {
        return Err(CommandError::new(
            "workspace.invalidWorkMode",
            "Work mode must be daily or coding.",
        ));
    }

    if is_git_repository {
        return Ok(ResolvedWorkspaceStrategy::GitWorktree);
    }

    let requested_strategy = requested_strategy
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (work_mode, requested_strategy) {
        ("daily", Some("initialize_git")) => Ok(ResolvedWorkspaceStrategy::InitializeGit),
        ("daily", Some("isolated_copy")) => Ok(ResolvedWorkspaceStrategy::IsolatedCopy),
        ("daily", None) => Err(CommandError::new(
            "workspace.strategyRequired",
            "Daily mode requires choosing Git initialization or an isolated copy.",
        )),
        ("coding", None | Some("isolated_copy")) => Ok(ResolvedWorkspaceStrategy::IsolatedCopy),
        ("coding", Some("direct_original")) if original_write_authorized => {
            Ok(ResolvedWorkspaceStrategy::DirectOriginal)
        }
        (_, Some("direct_original")) => Err(CommandError::new(
            "workspace.directOriginalNotAuthorized",
            "Direct edits require coding mode and explicit authorization.",
        )),
        (_, Some(_)) => Err(CommandError::new(
            "workspace.invalidStrategy",
            "The selected workspace strategy is not available for this work mode.",
        )),
        (_, None) => Err(CommandError::new(
            "workspace.invalidWorkMode",
            "Work mode must be daily or coding.",
        )),
    }
}

fn prepare_task_workspace(
    storage: &ManagedStorage,
    project: &git::ProjectInfo,
    task_id: &str,
    strategy: ResolvedWorkspaceStrategy,
    workspace_exclusions: &[String],
) -> AppResult<PreparedTaskWorkspace> {
    match strategy {
        ResolvedWorkspaceStrategy::GitWorktree => {
            let worktree = git::create_task_worktree(
                &project.path,
                storage.roots.worktree_root.clone(),
                task_id,
            )
            .map_err(task_git_error)?;
            Ok(PreparedTaskWorkspace {
                path: worktree.worktree_path,
                task_branch: Some(worktree.branch_name),
                target_branch: project.branch.clone().unwrap_or_default(),
                kind: "git_worktree",
                source_path: project.path.clone(),
                original_write_authorized: false,
                estimated_bytes: 0,
                initialized_git: false,
            })
        }
        ResolvedWorkspaceStrategy::IsolatedCopy => {
            let isolated = workspace::prepare_isolated_copy_with_exclusions(
                &project.path,
                &storage.roots.worktree_root,
                task_id,
                workspace_exclusions,
            )
            .map_err(workspace_io_error)?;
            Ok(PreparedTaskWorkspace {
                path: isolated.workspace_path.to_string_lossy().to_string(),
                task_branch: None,
                target_branch: String::new(),
                kind: "isolated_copy",
                source_path: isolated.source_path.to_string_lossy().to_string(),
                original_write_authorized: false,
                estimated_bytes: i64::try_from(isolated.estimated_bytes).unwrap_or(i64::MAX),
                initialized_git: false,
            })
        }
        ResolvedWorkspaceStrategy::DirectOriginal => Ok(PreparedTaskWorkspace {
            path: project.path.clone(),
            task_branch: None,
            target_branch: String::new(),
            kind: "direct_original",
            source_path: project.path.clone(),
            original_write_authorized: true,
            estimated_bytes: 0,
            initialized_git: false,
        }),
        ResolvedWorkspaceStrategy::InitializeGit => {
            let estimate =
                workspace::estimate_isolated_copy(&project.path).map_err(workspace_io_error)?;
            let initialized =
                git::initialize_repository_with_baseline(&project.path).map_err(task_git_error)?;
            let worktree = match git::create_task_worktree(
                &initialized.path,
                storage.roots.worktree_root.clone(),
                task_id,
            ) {
                Ok(worktree) => worktree,
                Err(error) => {
                    let _ = git::remove_initialized_repository_metadata(&initialized.path);
                    return Err(task_git_error(error));
                }
            };
            Ok(PreparedTaskWorkspace {
                path: worktree.worktree_path,
                task_branch: Some(worktree.branch_name),
                target_branch: initialized.branch.unwrap_or_default(),
                kind: "git_initialized_worktree",
                source_path: initialized.path,
                original_write_authorized: false,
                estimated_bytes: i64::try_from(estimate.estimated_bytes).unwrap_or(i64::MAX),
                initialized_git: true,
            })
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRecordRequest {
    pub repository_path: String,
    pub description: String,
    pub title: Option<String>,
    pub task_type: Option<String>,
    pub model_id: Option<String>,
    pub validation_command: Option<String>,
    pub mode: Option<String>,
    pub reasoning_effort: Option<String>,
    pub permission_level: Option<String>,
    pub network_policy: Option<String>,
    pub work_mode: Option<String>,
    pub workspace_strategy: Option<String>,
    #[serde(default)]
    pub original_write_authorized: bool,
    #[serde(default)]
    pub workspace_exclusions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateTaskWorkspaceRequest {
    pub repository_path: String,
    pub workspace_strategy: Option<String>,
    #[serde(default)]
    pub workspace_exclusions: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskWorkspaceEstimateView {
    pub source_path: String,
    pub destination_root: String,
    pub workspace_kind: String,
    pub estimated_bytes: u64,
    pub estimated_files: u64,
    pub available_bytes: u64,
    pub sufficient_space: bool,
    pub cleanup_policy: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTasksRequest {
    pub repository_path: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummaryView {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(rename = "type")]
    pub task_type: String,
    pub status: String,
    pub task_status: String,
    pub repository_id: String,
    pub repository_path: String,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    pub task_branch: Option<String>,
    pub target_branch: String,
    pub workspace_kind: String,
    pub source_path: String,
    pub original_write_authorized: bool,
    pub workspace_estimated_bytes: i64,
    pub agent_stage: String,
    pub latest_validation_status: String,
    pub latest_diff_summary: String,
    pub merge_preview: MergePreviewView,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePreviewView {
    pub target_branch: String,
    pub source_branch: Option<String>,
    pub status: String,
    pub can_merge: bool,
    pub blockers: Vec<String>,
    pub record_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetailView {
    pub task: TaskSummaryView,
    pub todos: Vec<TodoView>,
    pub command_runs: Vec<CommandRunView>,
    pub approvals: Vec<ApprovalView>,
    pub artifacts: Vec<ArtifactView>,
    pub artifact_files: Vec<ArtifactFileView>,
    pub agent_session: Option<AgentSessionView>,
    pub timeline: Vec<AgentEventView>,
    pub validation_rounds: Vec<ValidationRoundView>,
    pub merge_records: Vec<MergeRecordView>,
    pub delivery_review_state: DeliveryReviewState,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoView {
    pub id: String,
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRunView {
    pub run_id: String,
    pub task_id: String,
    pub purpose: String,
    pub command: String,
    pub cwd: String,
    pub status: String,
    pub stdout_path: Option<String>,
    pub stderr_path: Option<String>,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalView {
    pub id: String,
    pub task_id: String,
    pub approval_type: String,
    pub risk_level: String,
    pub content: String,
    pub reason: String,
    pub decision: Option<String>,
    pub comment: Option<String>,
    pub created_at: String,
    pub decided_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactView {
    pub id: String,
    pub task_id: String,
    pub changed_files: String,
    pub diff_path: Option<String>,
    pub test_report_path: Option<String>,
    pub screenshots: String,
    pub summary: String,
    pub commit_message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactFileView {
    pub id: String,
    pub task_id: String,
    pub artifact_id: Option<String>,
    pub file_type: String,
    pub path: String,
    pub size_bytes: i64,
    pub compressed: bool,
    pub retention_class: String,
    pub created_at: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionView {
    pub id: String,
    pub task_id: String,
    pub status: String,
    pub stage: String,
    pub checkpoint_id: Option<String>,
    pub iterations: i64,
    pub repair_round: i64,
    pub max_repair_rounds: i64,
    pub validation_request: Value,
    pub validation_round: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEventView {
    pub event_id: String,
    pub task_id: String,
    pub event_type: String,
    pub stage: String,
    pub message: String,
    pub created_at: String,
    pub payload: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationRoundView {
    pub id: String,
    pub task_id: String,
    pub round_index: i64,
    pub repair_round: i64,
    pub status: String,
    pub command_run_id: Option<String>,
    pub analysis: String,
    pub repair_summary: String,
    pub validation_summary: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRecordView {
    pub id: String,
    pub task_id: String,
    pub status: String,
    pub target_branch: String,
    pub source_branch: String,
    pub commit_sha: String,
    pub commit_message: String,
    pub conflict_files: Vec<String>,
    pub error_reason: Option<String>,
    pub record_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Default)]
struct RuntimeSummary {
    agent_stage: Option<String>,
    latest_validation_status: String,
    latest_diff_summary: String,
    merge_preview: Option<MergePreviewView>,
}

#[tauri::command]
pub fn create_task_record(
    storage: State<'_, ManagedStorage>,
    request: CreateTaskRecordRequest,
) -> AppResult<TaskSummaryView> {
    create_task_record_inner(&storage, request)
}

#[tauri::command]
pub fn estimate_task_workspace(
    storage: State<'_, ManagedStorage>,
    request: EstimateTaskWorkspaceRequest,
) -> AppResult<TaskWorkspaceEstimateView> {
    estimate_task_workspace_inner(&storage, request)
}

#[tauri::command]
pub fn delete_task_record(
    storage: State<'_, ManagedStorage>,
    task_id: String,
    rollback_initialization: Option<bool>,
) -> AppResult<()> {
    let task_id = require_non_empty(task_id.trim(), "task.taskIdRequired")?;
    delete_task_record_with_options(&storage, task_id, rollback_initialization.unwrap_or(false))
}

fn estimate_task_workspace_inner(
    storage: &ManagedStorage,
    request: EstimateTaskWorkspaceRequest,
) -> AppResult<TaskWorkspaceEstimateView> {
    let project = git::inspect_project(request.repository_path.trim()).map_err(task_git_error)?;
    let requested_strategy = request.workspace_strategy.as_deref().map(str::trim);
    let (workspace_kind, destination_root, estimate, cleanup_policy) = if project.is_git_repository
    {
        (
            "git_worktree",
            storage.roots.worktree_root.to_string_lossy().to_string(),
            workspace::estimate_isolated_copy(&project.path).map_err(|error| {
                workspace_operation_error(error, "estimate source", Path::new(&project.path))
            })?,
            "remove_workspace_keep_source",
        )
    } else {
        match requested_strategy {
            Some("direct_original") => (
                "direct_original",
                project.path.clone(),
                workspace::IsolatedCopyEstimate::default(),
                "keep_original",
            ),
            Some("initialize_git") => (
                "git_initialized_worktree",
                storage.roots.worktree_root.to_string_lossy().to_string(),
                workspace::estimate_isolated_copy(&project.path).map_err(|error| {
                    workspace_operation_error(error, "estimate source", Path::new(&project.path))
                })?,
                "remove_worktree_keep_repository",
            ),
            None | Some("isolated_copy") => (
                "isolated_copy",
                storage.roots.worktree_root.to_string_lossy().to_string(),
                workspace::estimate_isolated_copy_with_exclusions(
                    &project.path,
                    &request.workspace_exclusions,
                )
                .map_err(|error| {
                    workspace_operation_error(error, "estimate source", Path::new(&project.path))
                })?,
                "remove_workspace_keep_source",
            ),
            Some(_) => {
                return Err(CommandError::new(
                    "workspace.invalidStrategy",
                    "The selected workspace strategy cannot be estimated.",
                ))
            }
        }
    };
    let available_bytes =
        workspace_available_space(Path::new(&destination_root)).map_err(|error| {
            workspace_operation_error(error, "inspect target space", Path::new(&destination_root))
        })?;
    Ok(TaskWorkspaceEstimateView {
        source_path: project.path,
        destination_root,
        workspace_kind: workspace_kind.to_string(),
        estimated_bytes: estimate.estimated_bytes,
        estimated_files: estimate.estimated_files,
        available_bytes,
        sufficient_space: available_bytes >= estimate.estimated_bytes,
        cleanup_policy: cleanup_policy.to_string(),
    })
}

pub(crate) fn create_task_record_inner(
    storage: &ManagedStorage,
    request: CreateTaskRecordRequest,
) -> AppResult<TaskSummaryView> {
    let project = git::inspect_project(request.repository_path.trim()).map_err(task_git_error)?;
    let workspace_strategy = resolve_workspace_strategy(
        project.is_git_repository,
        request.work_mode.as_deref(),
        request.workspace_strategy.as_deref(),
        request.original_write_authorized,
    )?;
    let repository_id = repository_id(&project.path);
    let raw_description =
        require_non_empty(request.description.trim(), "task.descriptionRequired")?;
    let sanitized_description = sanitize_for_model_context(raw_description, "task.description");
    let description = sanitized_description.content.clone();
    let raw_title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| derive_title(&description));
    let sanitized_title = sanitize_for_model_context(&raw_title, "task.title");
    let title = sanitized_title.content.clone();
    let task_type = normalize_task_type(request.task_type.as_deref())?;
    let model_id = request
        .model_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let raw_validation_command = request
        .validation_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let sanitized_validation_command = raw_validation_command
        .as_deref()
        .map(|command| sanitize_for_model_context(command, "task.validationCommand"));
    let validation_command = sanitized_validation_command
        .as_ref()
        .map(|command| command.content.as_str())
        .filter(|command| !command.trim().is_empty());
    let contract_overrides = RunContractOverrides {
        mode: clean_optional(request.mode.as_deref()),
        reasoning_effort: clean_optional(request.reasoning_effort.as_deref()),
        permission_level: clean_optional(request.permission_level.as_deref()),
        network_policy: clean_optional(request.network_policy.as_deref()),
    };
    let task_id = format!("task-{}", Uuid::new_v4());
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(&task_id)
        .map_err(storage_error)?;
    let workspace = match prepare_task_workspace(
        storage,
        &project,
        &task_id,
        workspace_strategy,
        &request.workspace_exclusions,
    ) {
        Ok(workspace) => workspace,
        Err(error) => {
            let _ = fs::remove_dir_all(&paths.root);
            return Err(error);
        }
    };
    let session_id = format!("agent-session-{task_id}");
    let session_path = paths.artifacts_dir.join("agent-session.json");
    let session_payload = json!({
        "session_id": session_id,
        "task_id": task_id,
        "repository_id": repository_id,
        "repository_path": project.path,
        "worktree_path": workspace.path,
        "task_branch": workspace.task_branch,
        "target_branch": workspace.target_branch,
        "workspace_kind": workspace.kind,
        "source_path": workspace.source_path,
        "original_write_authorized": workspace.original_write_authorized,
        "workspace_estimated_bytes": workspace.estimated_bytes,
        "model_id": model_id,
        "validation_command": validation_command,
        "status": "created",
        "stage": DEFAULT_TASK_STATUS,
    });

    if let Err(error) = write_json_file(&session_path, &session_payload) {
        if let Err(cleanup_error) = rollback_created_task_files(
            &task_id,
            &project.path,
            Some(&workspace.path),
            workspace.task_branch.as_deref(),
            workspace.kind,
            workspace.initialized_git,
            &paths.root,
        ) {
            return Err(CommandError::new(
                "storage.rollbackFailed",
                format!(
                    "Task creation failed and rollback left residual files: {error}; cleanup error: {cleanup_error}"
                ),
            ));
        }
        return Err(storage_error(error));
    }

    let result = persist_created_task(
        storage,
        &task_id,
        &title,
        &description,
        &task_type,
        model_id,
        validation_command,
        contract_overrides,
        &sanitized_description,
        &sanitized_title,
        sanitized_validation_command.as_ref(),
        &project,
        &repository_id,
        &workspace.path,
        workspace.task_branch.as_deref(),
        &workspace.target_branch,
        workspace.kind,
        &workspace.source_path,
        workspace.original_write_authorized,
        workspace.estimated_bytes,
        &session_id,
        &session_path,
    );

    if let Err(error) = result {
        if let Err(cleanup_error) = rollback_created_task_files(
            &task_id,
            &project.path,
            Some(&workspace.path),
            workspace.task_branch.as_deref(),
            workspace.kind,
            workspace.initialized_git,
            &paths.root,
        ) {
            return Err(CommandError::new(
                "storage.rollbackFailed",
                format!(
                    "{} Cleanup after the failed task creation also failed: {cleanup_error}",
                    error.message
                ),
            ));
        }
        return Err(error);
    }

    let record = load_task(storage, &task_id)?;
    summarize_task(storage, record)
}

#[tauri::command]
pub fn list_tasks(
    storage: State<'_, ManagedStorage>,
    request: Option<ListTasksRequest>,
) -> AppResult<Vec<TaskSummaryView>> {
    let request = request.unwrap_or(ListTasksRequest {
        repository_path: None,
        status: None,
        limit: None,
    });
    let repository_path = request
        .repository_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let status = request
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let limit = request.limit.unwrap_or(DEFAULT_TASK_LIST_LIMIT);
    let records = {
        let store = storage.store.lock().map_err(|_| storage_lock_error())?;
        TaskRepository::new(store.connection())
            .list_recent(repository_path, status, limit)
            .map_err(storage_error)?
    };

    records
        .into_iter()
        .map(|record| summarize_task(&storage, record))
        .collect()
}

#[tauri::command]
pub fn get_task_record(
    storage: State<'_, ManagedStorage>,
    task_id: String,
) -> AppResult<TaskSummaryView> {
    let task_id = require_non_empty(task_id.trim(), "task.taskIdRequired")?;
    let record = load_task(&storage, task_id)?;

    summarize_task(&storage, record)
}

#[cfg(test)]
fn delete_task_record_inner(storage: &ManagedStorage, task_id: &str) -> AppResult<()> {
    delete_task_record_with_options(storage, task_id, false)
}

fn delete_task_record_with_options(
    storage: &ManagedStorage,
    task_id: &str,
    rollback_initialization: bool,
) -> AppResult<()> {
    let task = load_task(storage, task_id)?;
    let task_paths = storage.roots.task_artifact_paths(task_id);
    {
        let store = storage.store.lock().map_err(|_| storage_lock_error())?;
        TaskRepository::new(store.connection())
            .delete(task_id)
            .map_err(storage_error)?;
    }

    rollback_created_task_files(
        &task.id,
        &task.repository_path,
        task.worktree_path.as_deref(),
        task.branch_name.as_deref(),
        &task.workspace_kind,
        rollback_initialization && task.workspace_kind == "git_initialized_worktree",
        &task_paths.root,
    )
    .map_err(storage_error)
}

#[tauri::command]
pub fn get_task_detail(
    storage: State<'_, ManagedStorage>,
    task_id: String,
) -> AppResult<TaskDetailView> {
    let task_id = require_non_empty(task_id.trim(), "task.taskIdRequired")?;
    let record = load_task(&storage, task_id)?;
    let task = summarize_task(&storage, record)?;

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    let todos = TodoRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?
        .into_iter()
        .map(TodoView::from)
        .collect();
    let command_runs = CommandRunRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?
        .into_iter()
        .map(CommandRunView::from)
        .collect();
    let approvals = ApprovalRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?
        .into_iter()
        .map(ApprovalView::from)
        .collect();
    let artifacts = ArtifactRepository::new(connection)
        .artifacts_for_task(task_id)
        .map_err(storage_error)?
        .into_iter()
        .map(ArtifactView::from)
        .collect();
    let artifact_files = ArtifactRepository::new(connection)
        .files_for_task(task_id)
        .map_err(storage_error)?
        .into_iter()
        .map(ArtifactFileView::from)
        .collect();
    let agent_session = AgentSessionRepository::new(connection)
        .get_for_task(task_id)
        .map_err(storage_error)?
        .map(AgentSessionView::from);
    let timeline = AgentEventRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?
        .into_iter()
        .map(AgentEventView::from)
        .collect();
    let validation_rounds = ValidationRoundRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?
        .into_iter()
        .map(ValidationRoundView::from)
        .collect();
    let merge_records = MergeRecordRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?
        .into_iter()
        .map(MergeRecordView::from)
        .collect();
    drop(store);
    let delivery_review_state =
        crate::commands::s12_evidence::delivery_review_state_for_task(&storage, task_id)?;

    Ok(TaskDetailView {
        task,
        todos,
        command_runs,
        approvals,
        artifacts,
        artifact_files,
        agent_session,
        timeline,
        validation_rounds,
        merge_records,
        delivery_review_state,
    })
}

#[allow(clippy::too_many_arguments)]
fn persist_created_task(
    storage: &ManagedStorage,
    task_id: &str,
    title: &str,
    description: &str,
    task_type: &str,
    model_id: Option<&str>,
    validation_command: Option<&str>,
    contract_overrides: RunContractOverrides<'_>,
    sanitized_description: &SanitizedContent,
    sanitized_title: &SanitizedContent,
    sanitized_validation_command: Option<&SanitizedContent>,
    project: &git::ProjectInfo,
    repository_id: &str,
    worktree_path: &str,
    task_branch: Option<&str>,
    target_branch: &str,
    workspace_kind: &str,
    source_path: &str,
    original_write_authorized: bool,
    workspace_estimated_bytes: i64,
    session_id: &str,
    session_path: &Path,
) -> AppResult<()> {
    let session_path_text = session_path.to_string_lossy().to_string();
    let session_size = file_size(session_path).map_err(storage_error)?;
    let mut store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let transaction = store
        .connection_mut()
        .transaction()
        .map_err(StorageError::from)
        .map_err(storage_error)?;

    TaskRepository::new(&transaction)
        .create(NewTask {
            id: task_id,
            title,
            description,
            task_type,
            status: DEFAULT_TASK_STATUS,
            repository_path: &project.path,
            worktree_path: Some(worktree_path),
            branch_name: task_branch,
            target_branch,
            workspace_kind,
            source_path,
            original_write_authorized,
            workspace_estimated_bytes,
            model_id,
        })
        .map_err(storage_error)?;

    AgentSessionRepository::new(&transaction)
        .create(NewAgentSession {
            id: session_id,
            task_id,
            status: "created",
            stage: DEFAULT_TASK_STATUS,
            checkpoint_id: None,
            iterations: 0,
            repair_round: 0,
            max_repair_rounds: 0,
            validation_request_json: "{}",
            validation_round: 0,
        })
        .map_err(storage_error)?;

    ArtifactRepository::new(&transaction)
        .record_file(NewArtifactFile {
            id: &format!("file-{session_id}"),
            task_id,
            artifact_id: None,
            file_type: "agent_session",
            path: &session_path_text,
            size_bytes: session_size as i64,
            compressed: false,
            retention_class: "permanent",
            expires_at: None,
        })
        .map_err(storage_error)?;

    let profile = PersonalProfileRepository::new(&transaction)
        .active_profile()
        .map_err(storage_error)?;
    let effective_model_id = model_id.or(profile.model_id.as_deref());
    let mode = contract_overrides.mode.unwrap_or(&profile.mode);
    let reasoning_effort = contract_overrides
        .reasoning_effort
        .unwrap_or(&profile.reasoning_effort);
    let permission_level = contract_overrides
        .permission_level
        .unwrap_or(&profile.permission_level);
    let network_policy = contract_overrides
        .network_policy
        .unwrap_or(&profile.network_policy);
    let allowed_paths = vec![worktree_path.to_string()];
    let allowed_commands = validation_command
        .map(|command| vec![command.to_string()])
        .unwrap_or_default();
    let allowed_paths_json = serde_json::to_string(&allowed_paths).map_err(json_error)?;
    let allowed_commands_json = serde_json::to_string(&allowed_commands).map_err(json_error)?;
    let contract_payload = json!({
        "taskId": task_id,
        "profileId": profile.id,
        "mode": mode,
        "modelId": effective_model_id,
        "reasoningEffort": reasoning_effort,
        "permissionLevel": permission_level,
        "networkPolicy": network_policy,
        "allowedPaths": allowed_paths,
        "allowedCommands": allowed_commands,
        "validationCommand": validation_command,
        "tokenBudgetTotal": profile.token_budget_total,
        "tokenBudgetPerCall": profile.token_budget_per_call,
        "outputLanguage": profile.output_language,
        "memoryScope": profile.memory_scope,
        "budgetOverflowPolicy": "pause_for_approval",
        "source": "active_profile"
    });
    let contract_json = serde_json::to_string(&contract_payload).map_err(json_error)?;
    RunContractRepository::new(&transaction)
        .upsert(NewRunContract {
            id: &format!("run-contract-{task_id}"),
            task_id,
            profile_id: Some(&profile.id),
            mode,
            model_id: effective_model_id,
            reasoning_effort,
            permission_level,
            network_policy,
            allowed_paths_json: &allowed_paths_json,
            allowed_commands_json: &allowed_commands_json,
            validation_command,
            token_budget_total: profile.token_budget_total,
            token_budget_per_call: profile.token_budget_per_call,
            output_language: &profile.output_language,
            memory_scope: &profile.memory_scope,
            budget_overflow_policy: "pause_for_approval",
            contract_json: &contract_json,
        })
        .map_err(storage_error)?;

    record_context_observation(
        &transaction,
        ContextObservation {
            task_id,
            run_id: Some("task-create"),
            event_type: "task_created",
            data_kind: "task_description",
            source_type: "user_input",
            source_ref: "task.description",
            destination: "local_task_record",
            provider: Some("local-desktop"),
            model_id: effective_model_id,
            layer: "recent_user_request",
        },
        sanitized_description,
    )
    .map_err(storage_error)?;
    record_context_observation(
        &transaction,
        ContextObservation {
            task_id,
            run_id: Some("task-create"),
            event_type: "task_created",
            data_kind: "task_title",
            source_type: "user_input",
            source_ref: "task.title",
            destination: "local_task_record",
            provider: Some("local-desktop"),
            model_id: effective_model_id,
            layer: "recent_user_request",
        },
        sanitized_title,
    )
    .map_err(storage_error)?;
    if let Some(sanitized_validation_command) = sanitized_validation_command {
        record_context_observation(
            &transaction,
            ContextObservation {
                task_id,
                run_id: Some("task-create"),
                event_type: "task_created",
                data_kind: "validation_command",
                source_type: "user_input",
                source_ref: "task.validationCommand",
                destination: "local_task_record",
                provider: Some("local-desktop"),
                model_id: effective_model_id,
                layer: "validation_policy",
            },
            sanitized_validation_command,
        )
        .map_err(storage_error)?;
    }
    let initial_input_tokens = sanitized_description.tokens_estimate
        + sanitized_title.tokens_estimate
        + sanitized_validation_command
            .map(|content| content.tokens_estimate)
            .unwrap_or(0);
    record_token_budget_observation(
        &transaction,
        TokenBudgetObservation {
            task_id,
            run_id: Some("task-create"),
            call_type: "task_create",
            provider: Some("local-desktop"),
            model_id: effective_model_id,
            phase: DEFAULT_TASK_STATUS,
            input_tokens_estimate: initial_input_tokens,
            output_tokens_estimate: 0,
            budget_limit: profile.token_budget_total,
            overflow_policy: "pause_for_approval",
            quality_fallback: "",
        },
    )
    .map_err(storage_error)?;

    let events = AgentEventRepository::new(&transaction);
    let created_message = if project.is_git_repository {
        "Task created with isolated branch, worktree, and local Agent session."
    } else {
        "Task created from a local project directory and local Agent session."
    };
    record_event(
        &events,
        task_id,
        "task.created",
        DEFAULT_TASK_STATUS,
        created_message,
        json!({
            "repository_id": repository_id,
            "repository_path": project.path,
            "target_branch": target_branch,
            "task_branch": task_branch,
            "worktree_path": worktree_path,
            "workspace_kind": workspace_kind,
            "source_path": source_path,
            "original_write_authorized": original_write_authorized,
            "workspace_estimated_bytes": workspace_estimated_bytes,
            "is_git_repository": project.is_git_repository,
            "agent_session_id": session_id,
            "validation_command": validation_command,
            "run_contract_id": format!("run-contract-{task_id}"),
            "active_profile_id": profile.id,
        }),
    )?;

    let todos = TodoRepository::new(&transaction);
    for (index, title, body) in initial_todos(description) {
        let todo_id = format!("todo-{task_id}-{index:02}");
        todos
            .create(NewTodo {
                id: &todo_id,
                task_id,
                title,
                description: &body,
                status: "pending",
            })
            .map_err(storage_error)?;
        record_event(
            &events,
            task_id,
            "todo.created",
            DEFAULT_TASK_STATUS,
            &format!("Todo created: {title}"),
            json!({
                "todo_id": todo_id,
                "status": "pending",
            }),
        )?;
    }

    transaction
        .commit()
        .map_err(StorageError::from)
        .map_err(storage_error)?;

    Ok(())
}

fn summarize_task(storage: &ManagedStorage, record: TaskRecord) -> AppResult<TaskSummaryView> {
    let repository_id = repository_id(&record.repository_path);
    let target_branch = if record.target_branch.trim().is_empty() {
        git::inspect_project(&record.repository_path)
            .ok()
            .and_then(|project| project.branch)
            .unwrap_or_default()
    } else {
        record.target_branch.clone()
    };
    let runtime = runtime_summary(storage, &record, &target_branch);
    let agent_stage = runtime
        .agent_stage
        .unwrap_or_else(|| stage_from_task_status(&record.status));
    let merge_preview = runtime.merge_preview.unwrap_or_else(|| {
        default_merge_preview(
            &target_branch,
            record.branch_name.as_deref(),
            record.worktree_path.as_deref(),
        )
    });

    Ok(TaskSummaryView {
        id: record.id,
        title: record.title,
        description: record.description,
        task_type: record.task_type,
        task_status: record.status.clone(),
        status: record.status,
        repository_id,
        repository_path: record.repository_path,
        worktree_path: record.worktree_path,
        task_branch: record.branch_name.clone(),
        branch_name: record.branch_name,
        target_branch,
        workspace_kind: record.workspace_kind,
        source_path: record.source_path,
        original_write_authorized: record.original_write_authorized,
        workspace_estimated_bytes: record.workspace_estimated_bytes,
        agent_stage,
        latest_validation_status: runtime.latest_validation_status,
        latest_diff_summary: runtime.latest_diff_summary,
        merge_preview,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

fn runtime_summary(
    storage: &ManagedStorage,
    record: &TaskRecord,
    target_branch: &str,
) -> RuntimeSummary {
    let mut summary = RuntimeSummary {
        latest_validation_status: "notRun".to_string(),
        ..RuntimeSummary::default()
    };

    let Ok(store) = storage.store.lock() else {
        return summary;
    };
    let connection = store.connection();

    if let Ok(Some(session)) = AgentSessionRepository::new(connection).get_for_task(&record.id) {
        summary.agent_stage = Some(session.stage);
    }
    if let Ok(runs) = CommandRunRepository::new(connection).list_for_task(&record.id) {
        summary.latest_validation_status = latest_validation_status(&runs).to_string();
    }
    if let Ok(artifacts) = ArtifactRepository::new(connection).artifacts_for_task(&record.id) {
        summary.latest_diff_summary = latest_diff_summary(&artifacts);
    }
    if let Ok(records) = MergeRecordRepository::new(connection).list_for_task(&record.id) {
        summary.merge_preview = latest_merge_preview(
            target_branch,
            record.branch_name.as_deref(),
            record.worktree_path.as_deref(),
            &records,
        );
    }

    summary
}

fn latest_validation_status(runs: &[CommandRunRecord]) -> &'static str {
    let Some(run) = runs.iter().rev().find(|run| is_validation_command_run(run)) else {
        return "notRun";
    };

    if run.status == "passed" && run.exit_code.unwrap_or(0) == 0 {
        "passed"
    } else if run.status == "cancelled" {
        "cancelled"
    } else if run.status == "timedOut" {
        "timedOut"
    } else {
        "failed"
    }
}

fn is_validation_command_run(run: &CommandRunRecord) -> bool {
    run.purpose == "validation"
}

fn latest_diff_summary(artifacts: &[ArtifactRecord]) -> String {
    let Some(artifact) = artifacts
        .iter()
        .rev()
        .find(|artifact| artifact.diff_path.is_some())
    else {
        return String::new();
    };

    let file_count = changed_file_count(&artifact.changed_files);
    if file_count == 0 {
        artifact.summary.clone()
    } else {
        format!("{file_count} changed file(s)")
    }
}

fn changed_file_count(changed_files: &str) -> usize {
    serde_json::from_str::<Value>(changed_files)
        .ok()
        .and_then(|value| value.as_array().map(Vec::len))
        .unwrap_or(0)
}

fn latest_merge_preview(
    target_branch: &str,
    task_branch: Option<&str>,
    worktree_path: Option<&str>,
    records: &[MergeRecord],
) -> Option<MergePreviewView> {
    records
        .last()
        .map(|record| MergePreviewView {
            target_branch: record.target_branch.clone(),
            source_branch: Some(record.source_branch.clone()),
            status: record.status.clone(),
            can_merge: false,
            blockers: vec![format!("latest merge status: {}", record.status)],
            record_path: record.record_path.clone(),
        })
        .or_else(|| {
            Some(default_merge_preview(
                target_branch,
                task_branch,
                worktree_path,
            ))
        })
}

fn default_merge_preview(
    target_branch: &str,
    task_branch: Option<&str>,
    worktree_path: Option<&str>,
) -> MergePreviewView {
    let mut blockers = Vec::new();
    if worktree_path.is_none() {
        blockers.push("worktree missing".to_string());
    }
    if task_branch.is_none() {
        blockers.push("task branch missing".to_string());
    }

    MergePreviewView {
        target_branch: target_branch.to_string(),
        source_branch: task_branch.map(ToOwned::to_owned),
        status: if blockers.is_empty() {
            "available".to_string()
        } else {
            "blocked".to_string()
        },
        can_merge: false,
        blockers,
        record_path: None,
    }
}

fn record_event(
    events: &AgentEventRepository<'_>,
    task_id: &str,
    event_type: &str,
    stage: &str,
    message: &str,
    payload: Value,
) -> AppResult<()> {
    let event_id = format!("event-{}", Uuid::new_v4());
    let payload = serde_json::to_string(&payload).map_err(json_error)?;
    events
        .create(NewAgentEvent {
            event_id: &event_id,
            task_id,
            event_type,
            stage,
            message,
            payload: &payload,
        })
        .map_err(storage_error)?;
    Ok(())
}

fn initial_todos(description: &str) -> Vec<(usize, &'static str, String)> {
    vec![
        (
            1,
            "Plan from real task request",
            format!("Turn the user request into an auditable plan: {description}"),
        ),
        (
            2,
            "Edit only inside task worktree",
            "Apply code changes within the isolated task worktree.".to_string(),
        ),
        (
            3,
            "Run validation and capture logs",
            "Execute configured validation commands with stdout/stderr persisted to disk."
                .to_string(),
        ),
        (
            4,
            "Prepare review, diff, and merge evidence",
            "Generate diff, delivery evidence, and merge preview for this task id.".to_string(),
        ),
    ]
}

fn stage_from_task_status(status: &str) -> String {
    match status {
        "queued" => "queued".to_string(),
        "planning" => "planning".to_string(),
        "editing" => "editing".to_string(),
        "validating" => "validating".to_string(),
        "repairing" => "repairing".to_string(),
        "awaitingApproval" => "awaitingApproval".to_string(),
        "awaitingReview" => "awaitingReview".to_string(),
        "readyToMerge" => "readyToMerge".to_string(),
        "merged" => "merged".to_string(),
        "needsIntervention" => "needsIntervention".to_string(),
        "failed" => "failed".to_string(),
        "cancelled" => "cancelled".to_string(),
        _ => status.to_string(),
    }
}

fn repository_id(path: &str) -> String {
    format!("repo-{:08x}", stable_hash(path))
}

fn stable_hash(value: &str) -> u32 {
    let mut hash = 0x811c9dc5_u32;
    for byte in value.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

fn load_task(storage: &ManagedStorage, task_id: &str) -> AppResult<TaskRecord> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    TaskRepository::new(store.connection())
        .get_required(task_id)
        .map_err(storage_error)
}

fn write_json_file(path: &Path, payload: &Value) -> Result<(), StorageError> {
    let json = serde_json::to_string_pretty(payload).map_err(|error| {
        StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    })?;
    fs::write(path, json)?;
    Ok(())
}

fn file_size(path: &Path) -> std::io::Result<u64> {
    Ok(fs::metadata(path)?.len())
}

fn rollback_created_task_files(
    task_id: &str,
    repository_path: &str,
    worktree_path: Option<&str>,
    task_branch: Option<&str>,
    workspace_kind: &str,
    remove_initialized_git: bool,
    artifact_root: &Path,
) -> Result<(), StorageError> {
    let mut cleanup_failures = Vec::new();

    if let Some(worktree_path) = worktree_path {
        let workspace_root = Path::new(worktree_path);
        if workspace_root.exists()
            && matches!(workspace_kind, "git_worktree" | "git_initialized_worktree")
        {
            if let Err(error) = git::remove_task_worktree(repository_path, workspace_root) {
                cleanup_failures.push(format!(
                    "failed to remove worktree {worktree_path}: {error}"
                ));
            }
        } else if workspace_root.exists() && workspace_kind == "isolated_copy" {
            if let Err(error) = fs::remove_dir_all(workspace_root) {
                cleanup_failures.push(format!(
                    "failed to remove isolated workspace {worktree_path}: {error}"
                ));
            }
        }
    }

    if matches!(workspace_kind, "git_worktree" | "git_initialized_worktree") {
        if let Some(task_branch) = task_branch {
            if let Err(error) = git::delete_task_branch(repository_path, task_id, task_branch) {
                cleanup_failures.push(format!(
                    "failed to delete task branch {task_branch}: {error}"
                ));
            }
        }
    }

    if remove_initialized_git {
        if let Err(error) = git::remove_initialized_repository_metadata(repository_path) {
            cleanup_failures.push(format!(
                "failed to remove initialized Git metadata: {error}"
            ));
        }
    }

    if artifact_root.exists() {
        if let Err(error) = fs::remove_dir_all(artifact_root) {
            cleanup_failures.push(format!(
                "failed to remove artifact directory {}: {error}",
                artifact_root.display()
            ));
        }
    }

    if cleanup_failures.is_empty() {
        Ok(())
    } else {
        Err(StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            cleanup_failures.join("; "),
        )))
    }
}

impl From<TodoRecord> for TodoView {
    fn from(record: TodoRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            title: record.title,
            description: record.description,
            status: record.status,
            started_at: record.started_at,
            completed_at: record.completed_at,
            error_message: record.error_message,
        }
    }
}

impl From<CommandRunRecord> for CommandRunView {
    fn from(record: CommandRunRecord) -> Self {
        Self {
            run_id: record.id,
            task_id: record.task_id,
            purpose: record.purpose,
            command: record.command,
            cwd: record.cwd,
            status: record.status,
            stdout_path: record.stdout_path,
            stderr_path: record.stderr_path,
            exit_code: record.exit_code,
            duration_ms: record.duration_ms,
            created_at: record.created_at,
        }
    }
}

impl From<ApprovalRecord> for ApprovalView {
    fn from(record: ApprovalRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            approval_type: record.approval_type,
            risk_level: record.risk_level,
            content: record.content,
            reason: record.reason,
            decision: record.decision,
            comment: record.comment,
            created_at: record.created_at,
            decided_at: record.decided_at,
        }
    }
}

impl From<ArtifactRecord> for ArtifactView {
    fn from(record: ArtifactRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            changed_files: record.changed_files,
            diff_path: record.diff_path,
            test_report_path: record.test_report_path,
            screenshots: record.screenshots,
            summary: record.summary,
            commit_message: record.commit_message,
        }
    }
}

impl From<ArtifactFileRecord> for ArtifactFileView {
    fn from(record: ArtifactFileRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            artifact_id: record.artifact_id,
            file_type: record.file_type,
            path: record.path,
            size_bytes: record.size_bytes,
            compressed: record.compressed,
            retention_class: record.retention_class,
            created_at: record.created_at,
            expires_at: record.expires_at,
        }
    }
}

impl From<AgentSessionRecord> for AgentSessionView {
    fn from(record: AgentSessionRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            status: record.status,
            stage: record.stage,
            checkpoint_id: record.checkpoint_id,
            iterations: record.iterations,
            repair_round: record.repair_round,
            max_repair_rounds: record.max_repair_rounds,
            validation_request: serde_json::from_str(&record.validation_request_json)
                .unwrap_or_else(|_| json!({})),
            validation_round: record.validation_round,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

impl From<AgentEventRecord> for AgentEventView {
    fn from(record: AgentEventRecord) -> Self {
        Self {
            event_id: record.event_id,
            task_id: record.task_id,
            event_type: record.event_type,
            stage: record.stage,
            message: record.message,
            created_at: record.created_at,
            payload: serde_json::from_str(&record.payload).unwrap_or_else(|_| json!({})),
        }
    }
}

impl From<ValidationRoundRecord> for ValidationRoundView {
    fn from(record: ValidationRoundRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            round_index: record.round_index,
            repair_round: record.repair_round,
            status: record.status,
            command_run_id: record.command_run_id,
            analysis: record.analysis,
            repair_summary: record.repair_summary,
            validation_summary: record.validation_summary,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

impl From<MergeRecord> for MergeRecordView {
    fn from(record: MergeRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            status: record.status,
            target_branch: record.target_branch,
            source_branch: record.source_branch,
            commit_sha: record.commit_sha,
            commit_message: record.commit_message,
            conflict_files: serde_json::from_str(&record.conflict_files).unwrap_or_default(),
            error_reason: record.error_reason,
            record_path: record.record_path,
            created_at: record.created_at,
        }
    }
}

#[cfg(test)]
mod a_line_tests {
    use super::*;
    use crate::storage::{SqliteStore, StorageRoots};
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        sync::Mutex,
    };

    #[test]
    fn a_line_create_task_record_inner_persists_real_task_chain() {
        let repository = committed_repository("a-line-create-task");
        let storage = test_storage("a-line-storage");

        let summary = create_task_record_inner(
            &storage,
            CreateTaskRecordRequest {
                repository_path: repository.to_string_lossy().to_string(),
                description: "Wire a real A-line task through DB and filesystem.".to_string(),
                title: Some("A-line smoke task".to_string()),
                task_type: Some("custom".to_string()),
                model_id: Some("model-default".to_string()),
                validation_command: Some("npm run check".to_string()),
                mode: None,
                reasoning_effort: None,
                permission_level: None,
                network_policy: None,
                work_mode: Some("coding".to_string()),
                workspace_strategy: None,
                original_write_authorized: false,
                workspace_exclusions: Vec::new(),
            },
        )
        .expect("create task through real command inner");

        assert_eq!(summary.status, DEFAULT_TASK_STATUS);
        assert_eq!(summary.task_status, DEFAULT_TASK_STATUS);
        assert_eq!(
            Path::new(&summary.repository_path)
                .canonicalize()
                .expect("canonical summary path"),
            repository
                .canonicalize()
                .expect("canonical repository path")
        );
        assert!(summary
            .worktree_path
            .as_deref()
            .is_some_and(|path| Path::new(path).is_dir()));
        assert!(summary
            .task_branch
            .as_deref()
            .is_some_and(|branch| !branch.trim().is_empty()));

        let store = storage.store.lock().expect("storage lock");
        let connection = store.connection();
        let task = TaskRepository::new(connection)
            .get_required(&summary.id)
            .expect("task record exists");
        assert_eq!(task.title, "A-line smoke task");
        assert!(AgentSessionRepository::new(connection)
            .get_for_task(&summary.id)
            .expect("query agent session")
            .is_some());
        assert_eq!(
            TodoRepository::new(connection)
                .list_for_task(&summary.id)
                .expect("list todos")
                .len(),
            4
        );
        assert!(AgentEventRepository::new(connection)
            .list_for_task(&summary.id)
            .expect("list events")
            .iter()
            .any(|event| event.event_type == "task.created"));
        assert!(ArtifactRepository::new(connection)
            .files_for_task(&summary.id)
            .expect("list artifact files")
            .iter()
            .any(|file| file.file_type == "agent_session" && Path::new(&file.path).is_file()));
        let task_branch = summary.task_branch.clone().expect("task branch");
        let worktree_path = PathBuf::from(summary.worktree_path.as_deref().expect("worktree path"));
        drop(store);

        delete_task_record_inner(&storage, &summary.id).expect("delete Git task");
        assert!(!worktree_path.exists());
        let branches = Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(["branch", "--list", &task_branch])
            .output()
            .expect("list task branch");
        assert!(branches.status.success());
        assert!(String::from_utf8_lossy(&branches.stdout).trim().is_empty());

        fs::remove_dir_all(repository).expect("clean repository");
    }

    #[test]
    fn create_task_record_inner_persists_contract_overrides_from_task_dialog() {
        let repository = committed_repository("a-line-contract-override");
        let storage = test_storage("a-line-contract-storage");

        let summary = create_task_record_inner(
            &storage,
            CreateTaskRecordRequest {
                repository_path: repository.to_string_lossy().to_string(),
                description: "Persist chosen mode and permissions into the task contract."
                    .to_string(),
                title: Some("Contract override task".to_string()),
                task_type: Some("custom".to_string()),
                model_id: Some("gpt-5-codex".to_string()),
                validation_command: Some("npm run check".to_string()),
                mode: Some("review".to_string()),
                reasoning_effort: Some("max".to_string()),
                permission_level: Some("read_only".to_string()),
                network_policy: Some("enabled".to_string()),
                work_mode: Some("coding".to_string()),
                workspace_strategy: None,
                original_write_authorized: false,
                workspace_exclusions: Vec::new(),
            },
        )
        .expect("create task with contract overrides");

        let store = storage.store.lock().expect("storage lock");
        let contract = RunContractRepository::new(store.connection())
            .get_for_task(&summary.id)
            .expect("load run contract")
            .expect("run contract exists");

        assert_eq!(contract.mode, "review");
        assert_eq!(contract.reasoning_effort, "max");
        assert_eq!(contract.permission_level, "read_only");
        assert_eq!(contract.network_policy, "enabled");
    }

    #[test]
    fn create_task_record_inner_accepts_non_git_project_directory() {
        let project = non_git_temp_path("a-line-project");
        fs::create_dir_all(&project).expect("create project");
        fs::write(project.join("README.md"), "# Plain project\n").expect("write readme");
        fs::create_dir_all(project.join("coverage")).expect("create custom excluded directory");
        fs::write(project.join("coverage/report.json"), "{}\n").expect("write excluded report");
        let storage = test_storage("a-line-non-git-storage");

        let summary = create_task_record_inner(
            &storage,
            CreateTaskRecordRequest {
                repository_path: project.to_string_lossy().to_string(),
                description: "Run Agent inside a plain local project directory.".to_string(),
                title: Some("Plain project task".to_string()),
                task_type: Some("custom".to_string()),
                model_id: Some("model-default".to_string()),
                validation_command: Some("python -m unittest".to_string()),
                mode: None,
                reasoning_effort: None,
                permission_level: None,
                network_policy: None,
                work_mode: Some("coding".to_string()),
                workspace_strategy: None,
                original_write_authorized: false,
                workspace_exclusions: vec!["coverage".to_string()],
            },
        )
        .expect("create task for non-git project");

        assert_eq!(summary.status, DEFAULT_TASK_STATUS);
        assert_eq!(
            Path::new(&summary.repository_path)
                .canonicalize()
                .expect("canonical summary path"),
            project.canonicalize().expect("canonical project path")
        );
        let workspace_path = summary
            .worktree_path
            .as_deref()
            .map(PathBuf::from)
            .expect("isolated workspace path");
        assert_ne!(
            workspace_path.canonicalize().expect("canonical workspace"),
            project.canonicalize().expect("canonical project")
        );
        assert_eq!(
            fs::read_to_string(workspace_path.join("README.md")).expect("read copied file"),
            "# Plain project\n"
        );
        assert!(!workspace_path.join("coverage").exists());
        assert_eq!(summary.task_branch, None);
        assert_eq!(summary.target_branch, "");
        assert!(summary.workspace_estimated_bytes > 0);

        let store = storage.store.lock().expect("storage lock");
        let connection = store.connection();
        let task = TaskRepository::new(connection)
            .get_required(&summary.id)
            .expect("task record exists");
        assert_eq!(task.workspace_kind, "isolated_copy");
        assert_eq!(
            Path::new(&task.source_path)
                .canonicalize()
                .expect("canonical source path"),
            project.canonicalize().expect("canonical project source")
        );
        assert!(!task.original_write_authorized);
        assert_eq!(
            task.workspace_estimated_bytes,
            summary.workspace_estimated_bytes
        );
        assert_eq!(task.branch_name, None);
        drop(store);

        delete_task_record_inner(&storage, &summary.id).expect("delete isolated-copy task");
        assert!(!workspace_path.exists());
        assert!(project.join("README.md").is_file());

        fs::remove_dir_all(project).expect("clean project");
    }

    #[test]
    fn estimates_non_git_workspace_with_custom_exclusions_before_creation() {
        let project = non_git_temp_path("workspace-estimate-project");
        fs::create_dir_all(project.join("coverage")).expect("create project directories");
        fs::write(project.join("README.md"), "estimate me\n").expect("write source file");
        fs::write(project.join("coverage/report.json"), "{}\n").expect("write excluded file");
        let storage = test_storage("workspace-estimate-storage");

        let estimate = estimate_task_workspace_inner(
            &storage,
            EstimateTaskWorkspaceRequest {
                repository_path: project.to_string_lossy().to_string(),
                workspace_strategy: Some("isolated_copy".to_string()),
                workspace_exclusions: vec!["coverage".to_string()],
            },
        )
        .expect("estimate isolated workspace");

        assert_eq!(estimate.workspace_kind, "isolated_copy");
        assert_eq!(estimate.estimated_files, 1);
        assert!(estimate.estimated_bytes > 0);
        assert!(estimate.available_bytes >= estimate.estimated_bytes);
        assert!(estimate.sufficient_space);
        assert_eq!(
            PathBuf::from(&estimate.destination_root)
                .canonicalize()
                .expect("canonical destination root"),
            storage
                .roots
                .worktree_root
                .canonicalize()
                .expect("canonical storage worktree root")
        );

        fs::remove_dir_all(project).expect("clean project");
    }

    #[test]
    fn daily_mode_can_initialize_git_and_create_an_isolated_worktree() {
        let project = non_git_temp_path("daily-init-git-project");
        fs::create_dir_all(&project).expect("create project");
        fs::write(project.join("README.md"), "# Daily project\n").expect("write readme");
        let storage = test_storage("daily-init-git-storage");

        let summary = create_task_record_inner(
            &storage,
            CreateTaskRecordRequest {
                repository_path: project.to_string_lossy().to_string(),
                description: "Initialize Git before starting the daily task.".to_string(),
                title: Some("Daily initialized task".to_string()),
                task_type: Some("custom".to_string()),
                model_id: Some("model-default".to_string()),
                validation_command: None,
                mode: None,
                reasoning_effort: None,
                permission_level: None,
                network_policy: None,
                work_mode: Some("daily".to_string()),
                workspace_strategy: Some("initialize_git".to_string()),
                original_write_authorized: false,
                workspace_exclusions: Vec::new(),
            },
        )
        .expect("initialize Git and create task worktree");

        assert!(project.join(".git").is_dir());
        assert!(summary.task_branch.is_some());
        assert!(!summary.target_branch.is_empty());
        assert_eq!(summary.workspace_kind, "git_initialized_worktree");
        let workspace_path =
            PathBuf::from(summary.worktree_path.as_deref().expect("worktree path"));
        assert_ne!(
            workspace_path.canonicalize().expect("canonical worktree"),
            project.canonicalize().expect("canonical source")
        );
        assert_eq!(
            fs::read_to_string(workspace_path.join("README.md"))
                .expect("read worktree file")
                .replace("\r\n", "\n"),
            "# Daily project\n"
        );

        delete_task_record_inner(&storage, &summary.id).expect("delete initialized Git task");
        assert!(!workspace_path.exists());
        assert!(project.join(".git").is_dir());

        fs::remove_dir_all(project).expect("clean project");
    }

    #[test]
    fn task_creation_rollback_removes_new_git_metadata() {
        let project = non_git_temp_path("rollback-init-git-project");
        fs::create_dir_all(&project).expect("create project");
        fs::write(project.join("README.md"), "# Rollback project\n").expect("write readme");
        let storage = test_storage("rollback-init-git-storage");

        let summary = create_task_record_inner(
            &storage,
            CreateTaskRecordRequest {
                repository_path: project.to_string_lossy().to_string(),
                description: "Rollback Git initialization when Agent startup fails.".to_string(),
                title: Some("Rollback initialized task".to_string()),
                task_type: Some("custom".to_string()),
                model_id: None,
                validation_command: None,
                mode: None,
                reasoning_effort: None,
                permission_level: None,
                network_policy: None,
                work_mode: Some("daily".to_string()),
                workspace_strategy: Some("initialize_git".to_string()),
                original_write_authorized: false,
                workspace_exclusions: Vec::new(),
            },
        )
        .expect("create initialized task");

        delete_task_record_with_options(&storage, &summary.id, true)
            .expect("rollback task creation");

        assert!(!project.join(".git").exists());
        assert!(project.join("README.md").is_file());
        fs::remove_dir_all(project).expect("clean project");
    }

    #[test]
    fn a_line_latest_validation_status_ignores_diagnostic_commands() {
        let diagnostic = command_run("diagnostic-ok", "diagnostic", "passed", Some(0));
        let failed_validation = command_run("validation-failed", "validation", "failed", Some(1));
        let passed_validation = command_run("validation-passed", "validation", "passed", Some(0));

        assert_eq!(latest_validation_status(&[diagnostic.clone()]), "notRun");
        assert_eq!(
            latest_validation_status(&[diagnostic, failed_validation.clone()]),
            "failed"
        );
        assert_eq!(
            latest_validation_status(&[failed_validation, passed_validation]),
            "passed"
        );
    }

    fn command_run(
        id: &str,
        purpose: &str,
        status: &str,
        exit_code: Option<i64>,
    ) -> CommandRunRecord {
        CommandRunRecord {
            id: id.to_string(),
            task_id: "task-a-line".to_string(),
            purpose: purpose.to_string(),
            command: "npm run check".to_string(),
            cwd: "E:/codemax".to_string(),
            status: status.to_string(),
            stdout_path: None,
            stderr_path: None,
            exit_code,
            duration_ms: Some(1000),
            created_at: "2026-07-08T00:00:00Z".to_string(),
        }
    }

    fn test_storage(label: &str) -> ManagedStorage {
        let roots = StorageRoots::from_app_data_dir(temp_path(label));
        roots.ensure_base_dirs().expect("create storage roots");
        let store = SqliteStore::open_in_memory().expect("open sqlite");
        store.migrate().expect("run migrations");

        ManagedStorage {
            roots,
            store: Mutex::new(store),
        }
    }

    fn committed_repository(label: &str) -> PathBuf {
        let path = temp_path(label);
        fs::create_dir_all(&path).expect("create repository");
        run_git(&path, &["init"]);
        run_git(&path, &["config", "user.email", "codemax@example.test"]);
        run_git(&path, &["config", "user.name", "Codemax Test"]);
        fs::write(path.join("README.md"), "# CodeMax A-line smoke\n").expect("write readme");
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "initial commit"]);
        path
    }

    fn run_git(path: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("codemax-{label}-{}", Uuid::new_v4()))
    }

    fn non_git_temp_path(label: &str) -> PathBuf {
        std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("CodeMaxTests")
            .join(format!("codemax-{label}-{}", Uuid::new_v4()))
    }
}

fn normalize_task_type(task_type: Option<&str>) -> AppResult<String> {
    let task_type = task_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_TASK_TYPE);

    if matches!(
        task_type,
        "bugfix" | "test" | "refactor" | "explain" | "custom"
    ) {
        return Ok(task_type.to_string());
    }

    Err(CommandError::new(
        "task.invalidType",
        format!("Unsupported task type: {task_type}"),
    ))
}

fn clean_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn require_non_empty<'a>(value: &'a str, code: &'static str) -> AppResult<&'a str> {
    if value.is_empty() {
        return Err(CommandError::new(code, "Task field is required."));
    }

    Ok(value)
}

fn derive_title(description: &str) -> String {
    let first_line = description
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Agent task");
    let mut title = String::new();

    for character in first_line.chars().take(64) {
        title.push(character);
    }

    title
}

fn storage_lock_error() -> CommandError {
    CommandError::new(
        "storage.lockUnavailable",
        "Local storage is temporarily unavailable.",
    )
}

fn storage_error(error: impl Into<StorageError>) -> CommandError {
    match error.into() {
        StorageError::NotFound(message) => CommandError::new("task.notFound", message),
        StorageError::UnsafeCleanup { task_id, reasons } => CommandError::new(
            "storage.unsafeCleanup",
            format!(
                "Task {task_id} is not safe to clean: {}",
                reasons.join("; ")
            ),
        ),
        StorageError::Sqlite(error) => CommandError::new(
            "storage.sqliteError",
            format!("Local database error: {error}"),
        ),
        StorageError::Io(error) => CommandError::new(
            "storage.filesystemError",
            format!("Filesystem error: {error}"),
        ),
    }
}

fn workspace_available_space(path: &Path) -> std::io::Result<u64> {
    let mut existing = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "workspace root has no existing parent",
                )
            })?
            .to_path_buf();
    }
    match fs2::available_space(&existing) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            fs2::free_space(existing)
        }
        Err(error) => Err(error),
    }
}

fn workspace_operation_error(error: std::io::Error, operation: &str, path: &Path) -> CommandError {
    let code = match error.kind() {
        std::io::ErrorKind::AlreadyExists => "workspace.alreadyExists",
        std::io::ErrorKind::InvalidInput => "workspace.invalidPath",
        std::io::ErrorKind::PermissionDenied => "workspace.permissionDenied",
        std::io::ErrorKind::StorageFull => "workspace.insufficientSpace",
        _ => "workspace.copyFailed",
    };
    CommandError::new(
        code,
        format!("Unable to {operation} at {}: {error}", path.display()),
    )
}

fn workspace_io_error(error: std::io::Error) -> CommandError {
    let code = match error.kind() {
        std::io::ErrorKind::AlreadyExists => "workspace.alreadyExists",
        std::io::ErrorKind::InvalidInput => "workspace.invalidPath",
        std::io::ErrorKind::PermissionDenied => "workspace.permissionDenied",
        _ => "workspace.copyFailed",
    };
    CommandError::new(
        code,
        format!("Unable to create isolated workspace: {error}"),
    )
}

fn task_git_error(error: GitError) -> CommandError {
    match error {
        GitError::PathNotFound(path) => CommandError::new(
            "repository.pathNotFound",
            format!("Repository path does not exist: {}", path.to_string_lossy()),
        ),
        GitError::PathNotDirectory(path) => CommandError::new(
            "repository.pathNotDirectory",
            format!(
                "Repository path is not a directory: {}",
                path.to_string_lossy()
            ),
        ),
        GitError::Io(error) => CommandError::new(
            "repository.filesystemError",
            format!("Filesystem error while reading repository path: {error}"),
        ),
        GitError::GitUnavailable(message) => CommandError::new(
            "repository.gitUnavailable",
            format!("Git is not available on this machine: {message}"),
        ),
        GitError::NotRepository { path, stderr } => CommandError::new(
            "repository.notGitRepository",
            format!(
                "Selected path is not a Git repository: {}{}",
                path.to_string_lossy(),
                format_stderr(&stderr)
            ),
        ),
        GitError::CommandFailed { path, args, stderr } => CommandError::new(
            "repository.gitCommandFailed",
            format!(
                "Git command failed in {}: git {}{}",
                path.to_string_lossy(),
                args,
                format_stderr(&stderr)
            ),
        ),
        GitError::InvalidTaskId(task_id) => CommandError::new(
            "repository.invalidTaskId",
            format!("Task id cannot produce a valid Git branch or worktree path: {task_id}"),
        ),
        GitError::WorktreePathExists(path) => CommandError::new(
            "repository.worktreePathExists",
            format!("Worktree path already exists: {}", path.to_string_lossy()),
        ),
        GitError::EmptyCommitMessage => CommandError::new(
            "repository.mergeCommitMessageRequired",
            "Merge commit message is required.",
        ),
        GitError::DirtyTarget(path) => CommandError::new(
            "repository.mergeTargetDirty",
            format!(
                "Target repository has uncommitted changes: {}",
                path.to_string_lossy()
            ),
        ),
        GitError::TargetBranchChanged {
            path,
            expected,
            actual,
        } => CommandError::new(
            "repository.mergeTargetBranchChanged",
            format!(
                "Target branch changed in {}: expected {}, got {}",
                path.to_string_lossy(),
                expected,
                actual
            ),
        ),
    }
}

fn format_stderr(stderr: &str) -> String {
    let stderr = stderr.trim();
    if stderr.is_empty() {
        String::new()
    } else {
        format!(" ({stderr})")
    }
}

fn json_error(error: serde_json::Error) -> CommandError {
    CommandError::new(
        "task.invalidJson",
        format!("Unable to encode task metadata: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_title_uses_first_non_empty_line_and_truncates() {
        let title = derive_title(
            "   \nFix the payment precision issue and add regression coverage for decimal values",
        );

        assert_eq!(title.chars().count(), 64);
        assert!(title.starts_with("Fix the payment precision issue"));
    }

    #[test]
    fn normalize_task_type_rejects_unknown_values() {
        let error = normalize_task_type(Some("preview")).expect_err("unknown task types fail");

        assert_eq!(error.code, "task.invalidType");
    }

    #[test]
    fn repository_id_is_stable_and_prefixed() {
        assert_eq!(repository_id("E:/codemax"), repository_id("E:/codemax"));
        assert!(repository_id("E:/codemax").starts_with("repo-"));
    }

    #[test]
    fn git_projects_always_use_isolated_worktrees() {
        assert_eq!(
            resolve_workspace_strategy(true, Some("daily"), None, false)
                .expect("resolve Git strategy"),
            ResolvedWorkspaceStrategy::GitWorktree
        );
    }

    #[test]
    fn non_git_modes_resolve_to_user_approved_safe_strategies() {
        assert_eq!(
            resolve_workspace_strategy(false, Some("daily"), Some("initialize_git"), false,)
                .expect("daily Git initialization"),
            ResolvedWorkspaceStrategy::InitializeGit
        );
        assert_eq!(
            resolve_workspace_strategy(false, Some("daily"), Some("isolated_copy"), false,)
                .expect("daily isolated copy"),
            ResolvedWorkspaceStrategy::IsolatedCopy
        );
        assert_eq!(
            resolve_workspace_strategy(false, Some("coding"), None, false)
                .expect("coding defaults to isolated copy"),
            ResolvedWorkspaceStrategy::IsolatedCopy
        );
    }

    #[test]
    fn direct_original_requires_coding_mode_and_explicit_authorization() {
        for (mode, authorized) in [("daily", true), ("coding", false)] {
            let error =
                resolve_workspace_strategy(false, Some(mode), Some("direct_original"), authorized)
                    .expect_err("unsafe direct original strategy must fail");
            assert_eq!(error.code, "workspace.directOriginalNotAuthorized");
        }

        assert_eq!(
            resolve_workspace_strategy(false, Some("coding"), Some("direct_original"), true,)
                .expect("authorized coding direct edit"),
            ResolvedWorkspaceStrategy::DirectOriginal
        );
    }
}
