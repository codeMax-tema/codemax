use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;
use uuid::Uuid;

use crate::{
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
};

const DEFAULT_TASK_TYPE: &str = "custom";
const DEFAULT_TASK_STATUS: &str = "queued";
const DEFAULT_TASK_LIST_LIMIT: usize = 50;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRecordRequest {
    pub repository_path: String,
    pub description: String,
    pub title: Option<String>,
    pub task_type: Option<String>,
    pub model_id: Option<String>,
    pub validation_command: Option<String>,
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

pub(crate) fn create_task_record_inner(
    storage: &ManagedStorage,
    request: CreateTaskRecordRequest,
) -> AppResult<TaskSummaryView> {
    let repository =
        git::validate_repository(request.repository_path.trim()).map_err(task_git_error)?;
    let repository_id = repository_id(&repository.path);
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
    let task_id = format!("task-{}", Uuid::new_v4());
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(&task_id)
        .map_err(storage_error)?;
    let worktree = match git::create_task_worktree(
        &repository.path,
        storage.roots.worktree_root.clone(),
        &task_id,
    ) {
        Ok(worktree) => worktree,
        Err(error) => {
            let _ = fs::remove_dir_all(&paths.root);
            return Err(task_git_error(error));
        }
    };
    let session_id = format!("agent-session-{}", Uuid::new_v4());
    let session_path = paths.artifacts_dir.join("agent-session.json");
    let session_payload = json!({
        "session_id": session_id,
        "task_id": task_id,
        "repository_id": repository_id,
        "repository_path": repository.path,
        "worktree_path": worktree.worktree_path,
        "task_branch": worktree.branch_name,
        "target_branch": repository.branch,
        "model_id": model_id,
        "validation_command": validation_command,
        "status": "created",
        "stage": DEFAULT_TASK_STATUS,
    });

    if let Err(error) = write_json_file(&session_path, &session_payload) {
        rollback_created_task_files(&repository.path, &worktree, &paths.root);
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
        &sanitized_description,
        &sanitized_title,
        sanitized_validation_command.as_ref(),
        &repository,
        &repository_id,
        &worktree,
        &session_id,
        &session_path,
    );

    if let Err(error) = result {
        rollback_created_task_files(&repository.path, &worktree, &paths.root);
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
    sanitized_description: &SanitizedContent,
    sanitized_title: &SanitizedContent,
    sanitized_validation_command: Option<&SanitizedContent>,
    repository: &git::RepositoryInfo,
    repository_id: &str,
    worktree: &git::TaskWorktree,
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
            repository_path: &repository.path,
            worktree_path: Some(&worktree.worktree_path),
            branch_name: Some(&worktree.branch_name),
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
    let allowed_paths = vec![worktree.worktree_path.clone()];
    let allowed_commands = validation_command
        .map(|command| vec![command.to_string()])
        .unwrap_or_default();
    let allowed_paths_json = serde_json::to_string(&allowed_paths).map_err(json_error)?;
    let allowed_commands_json = serde_json::to_string(&allowed_commands).map_err(json_error)?;
    let contract_payload = json!({
        "taskId": task_id,
        "profileId": profile.id,
        "mode": profile.mode,
        "modelId": effective_model_id,
        "reasoningEffort": profile.reasoning_effort,
        "permissionLevel": profile.permission_level,
        "networkPolicy": profile.network_policy,
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
            mode: &profile.mode,
            model_id: effective_model_id,
            reasoning_effort: &profile.reasoning_effort,
            permission_level: &profile.permission_level,
            network_policy: &profile.network_policy,
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
    record_event(
        &events,
        task_id,
        "task.created",
        DEFAULT_TASK_STATUS,
        "Task created with isolated branch, worktree, and local Agent session.",
        json!({
            "repository_id": repository_id,
            "repository_path": repository.path,
            "target_branch": repository.branch,
            "task_branch": worktree.branch_name,
            "worktree_path": worktree.worktree_path,
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
    let target_branch = git::current_branch(&record.repository_path).unwrap_or_default();
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
    repository_path: &str,
    worktree: &git::TaskWorktree,
    artifact_root: &Path,
) {
    let _ = git::remove_task_worktree(repository_path, &worktree.worktree_path);
    let _ = fs::remove_dir_all(artifact_root);
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
}
