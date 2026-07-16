#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;
use uuid::Uuid;

use crate::{
    exec::{CommandRequest, CommandRunRegistry},
    git,
    safe_fs::{self, SafeFileOperation},
    storage::{
        AgentToolCallRepository, ApprovalRepository, CompleteAgentToolCall, ManagedStorage,
        NewAgentToolCall, NewApproval, NewTodo, StorageError, TaskRecord, TaskRepository,
        TodoRepository,
    },
};

use super::{
    exec::run_task_command,
    files::{execute_transaction, ExecuteSafeFileOperationsRequest},
};

const MAX_LIST_ENTRIES: usize = 500;
const MAX_SEARCH_RESULTS: usize = 200;
const MAX_SEARCH_FILE_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCallRequest {
    pub task_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCallResult {
    pub task_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub status: String,
    pub output: Value,
    pub error: Option<AgentToolCallError>,
    pub replayed: bool,
    pub audit_status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCallError {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy)]
struct DispatchRuntime<'a> {
    app: &'a AppHandle,
    registry: &'a CommandRunRegistry,
}

#[derive(Debug)]
struct ToolFailure {
    code: &'static str,
    message: &'static str,
}

impl ToolFailure {
    fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }
}

/// Dispatches a tool call that does not need an application runtime. `run_command` is rejected
/// here; callers that can execute commands must use `dispatch_agent_tool_call_with_runtime`.
pub async fn dispatch_agent_tool_call(
    storage: &ManagedStorage,
    request: AgentToolCallRequest,
) -> AgentToolCallResult {
    dispatch(storage, None, request).await
}

/// Dispatches a tool call with the existing controlled command-execution boundary available.
pub async fn dispatch_agent_tool_call_with_runtime(
    app: &AppHandle,
    storage: &ManagedStorage,
    registry: &CommandRunRegistry,
    request: AgentToolCallRequest,
) -> AgentToolCallResult {
    dispatch(storage, Some(DispatchRuntime { app, registry }), request).await
}

async fn dispatch(
    storage: &ManagedStorage,
    runtime: Option<DispatchRuntime<'_>>,
    request: AgentToolCallRequest,
) -> AgentToolCallResult {
    let request = normalize_request(request);
    if request.task_id.is_empty() || request.call_id.is_empty() || request.tool_name.is_empty() {
        return unrecorded_failure(
            request,
            "tool.invalidRequest",
            "The tool request is invalid.",
        );
    }

    let started_at = Instant::now();
    let existing = match begin_audit(storage, &request) {
        Ok(record) => record,
        Err(_) => {
            return unrecorded_failure(
                request,
                "tool.auditUnavailable",
                "Tool auditing is unavailable.",
            )
        }
    };
    if existing.completed_at.is_some() {
        return AgentToolCallResult {
            task_id: request.task_id,
            call_id: request.call_id,
            tool_name: request.tool_name,
            status: existing.status.clone(),
            output: json!({"replayed": true}),
            error: (existing.status == "failed").then(|| AgentToolCallError {
                code: "tool.previousFailure".to_string(),
                message: "The prior tool call failed.".to_string(),
            }),
            replayed: true,
            audit_status: existing.status,
        };
    }

    let dispatched = dispatch_fresh(storage, runtime, &request).await;
    let (result, transaction_id, command_run_id) = match dispatched {
        Ok((output, transaction_id, command_run_id)) => (
            AgentToolCallResult {
                task_id: request.task_id.clone(),
                call_id: request.call_id.clone(),
                tool_name: request.tool_name.clone(),
                status: "succeeded".to_string(),
                output,
                error: None,
                replayed: false,
                audit_status: "succeeded".to_string(),
            },
            transaction_id,
            command_run_id,
        ),
        Err(error) => (
            AgentToolCallResult {
                task_id: request.task_id.clone(),
                call_id: request.call_id.clone(),
                tool_name: request.tool_name.clone(),
                status: "failed".to_string(),
                output: json!({}),
                error: Some(AgentToolCallError {
                    code: error.code.to_string(),
                    message: error.message.to_string(),
                }),
                replayed: false,
                audit_status: "failed".to_string(),
            },
            None,
            None,
        ),
    };

    complete_audit(
        storage,
        &request,
        &result,
        started_at.elapsed().as_millis() as i64,
        transaction_id.as_deref(),
        command_run_id.as_deref(),
    )
    .unwrap_or_else(|_| {
        unrecorded_failure(
            request,
            "tool.auditUnavailable",
            "Tool auditing is unavailable.",
        )
    })
}

fn normalize_request(mut request: AgentToolCallRequest) -> AgentToolCallRequest {
    request.task_id = request.task_id.trim().to_string();
    request.call_id = request.call_id.trim().to_string();
    request.tool_name = request.tool_name.trim().to_string();
    request
}

fn begin_audit(
    storage: &ManagedStorage,
    request: &AgentToolCallRequest,
) -> Result<crate::storage::AgentToolCallRecord, StorageError> {
    let store = storage
        .store
        .lock()
        .map_err(|_| StorageError::NotFound("tool audit lock".to_string()))?;
    AgentToolCallRepository::new(store.connection()).begin(NewAgentToolCall::requested(
        &request.task_id,
        &request.call_id,
        &request.tool_name,
        &request_audit_summary(request),
    ))
}

fn complete_audit(
    storage: &ManagedStorage,
    request: &AgentToolCallRequest,
    result: &AgentToolCallResult,
    duration_ms: i64,
    transaction_id: Option<&str>,
    command_run_id: Option<&str>,
) -> Result<AgentToolCallResult, ()> {
    let summary = result_audit_summary(result);
    let store = storage.store.lock().map_err(|_| ())?;
    let completed = AgentToolCallRepository::new(store.connection())
        .complete(
            &request.task_id,
            &request.call_id,
            CompleteAgentToolCall {
                status: &result.status,
                result_summary: Some(&summary),
                duration_ms: Some(duration_ms),
                transaction_id,
                command_run_id,
                artifact_refs_json: "[]",
            },
        )
        .map_err(|_| ())?;
    Ok(AgentToolCallResult {
        audit_status: completed.status,
        ..result.clone()
    })
}

fn request_audit_summary(request: &AgentToolCallRequest) -> String {
    let parameter_keys = request
        .params
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    json!({"tool": request.tool_name, "parameterKeys": parameter_keys}).to_string()
}

fn result_audit_summary(result: &AgentToolCallResult) -> String {
    json!({
        "status": result.status,
        "tool": result.tool_name,
        "errorCode": result.error.as_ref().map(|error| error.code.as_str()),
        "outputKeys": result.output.as_object().map(|object| object.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
    })
    .to_string()
}

fn unrecorded_failure(
    request: AgentToolCallRequest,
    code: &'static str,
    message: &'static str,
) -> AgentToolCallResult {
    AgentToolCallResult {
        task_id: request.task_id,
        call_id: request.call_id,
        tool_name: request.tool_name,
        status: "failed".to_string(),
        output: json!({}),
        error: Some(AgentToolCallError {
            code: code.to_string(),
            message: message.to_string(),
        }),
        replayed: false,
        audit_status: "failed".to_string(),
    }
}

async fn dispatch_fresh(
    storage: &ManagedStorage,
    runtime: Option<DispatchRuntime<'_>>,
    request: &AgentToolCallRequest,
) -> Result<(Value, Option<String>, Option<String>), ToolFailure> {
    let task = load_task(storage, &request.task_id)?;
    match request.tool_name.as_str() {
        "list_files" => Ok((
            list_files(&task, parse_params(&request.params)?)?,
            None,
            None,
        )),
        "search_text" => Ok((
            search_text(&task, parse_params(&request.params)?)?,
            None,
            None,
        )),
        "read_file" => Ok((
            read_file(&task, parse_params(&request.params)?)?,
            None,
            None,
        )),
        "git_status" => Ok((
            git_status(&task, parse_params(&request.params)?)?,
            None,
            None,
        )),
        "git_diff" => Ok((git_diff(&task, parse_params(&request.params)?)?, None, None)),
        "apply_file_edits" => {
            let params: ApplyFileEditsParams = parse_params(&request.params)?;
            let operations = strict_file_operations(params.operations)?;
            let transaction = execute_transaction(
                storage,
                ExecuteSafeFileOperationsRequest {
                    task_id: request.task_id.clone(),
                    request_id: request.call_id.clone(),
                    operations,
                    approval_id: params.approval_id,
                    diff_artifact_id: None,
                    validation_round_id: None,
                    proof_pack_id: None,
                },
            )
            .map_err(|_| {
                ToolFailure::new(
                    "tool.executionFailed",
                    "The file edit could not be completed.",
                )
            })?;
            let transaction_id = transaction.transaction_id.clone();
            Ok((
                json!({"requestId": request.call_id, "transactionId": transaction_id, "status": transaction.status, "results": transaction.results}),
                Some(transaction.transaction_id),
                None,
            ))
        }
        "run_command" => {
            let runtime = runtime.ok_or_else(|| {
                ToolFailure::new(
                    "tool.runtimeUnavailable",
                    "Command execution is unavailable.",
                )
            })?;
            let params: RunCommandParams = parse_params(&request.params)?;
            let cwd = resolve_optional_path(&task, params.cwd.as_deref())?;
            let command = run_task_command(
                runtime.app,
                storage,
                runtime.registry,
                CommandRequest {
                    task_id: request.task_id.clone(),
                    run_id: Some(request.call_id.clone()),
                    command: params.command,
                    cwd: cwd.to_string_lossy().to_string(),
                    env: params.env,
                    timeout_ms: params.timeout_ms,
                    purpose: params.purpose,
                    approval_id: params.approval_id,
                },
            )
            .await
            .map_err(|_| {
                ToolFailure::new(
                    "tool.executionFailed",
                    "The command could not be completed.",
                )
            })?;
            let run_id = command.run_id.clone();
            Ok((
                json!({"runId": run_id, "status": command.status, "exitCode": command.exit_code, "durationMs": command.duration_ms, "timedOut": command.timed_out, "cancelled": command.cancelled}),
                None,
                Some(command.run_id),
            ))
        }
        "update_todos" => Ok((
            update_todos(storage, &request.task_id, parse_params(&request.params)?)?,
            None,
            None,
        )),
        "request_approval" => Ok((
            request_approval(storage, &request.task_id, parse_params(&request.params)?)?,
            None,
            None,
        )),
        "complete_task" => Ok((
            complete_task(storage, &request.task_id, parse_params(&request.params)?)?,
            None,
            None,
        )),
        _ => Err(ToolFailure::new(
            "tool.unknown",
            "The requested tool is not available.",
        )),
    }
}

fn load_task(storage: &ManagedStorage, task_id: &str) -> Result<TaskRecord, ToolFailure> {
    let store = storage.store.lock().map_err(|_| {
        ToolFailure::new("tool.storageUnavailable", "Local storage is unavailable.")
    })?;
    TaskRepository::new(store.connection())
        .get_required(task_id)
        .map_err(|_| ToolFailure::new("tool.taskUnavailable", "The task is unavailable."))
}

fn parse_params<T: for<'de> Deserialize<'de>>(params: &Value) -> Result<T, ToolFailure> {
    serde_json::from_value(params.clone())
        .map_err(|_| ToolFailure::new("tool.invalidParams", "Tool parameters are invalid."))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ListFilesParams {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    max_entries: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchTextParams {
    query: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    max_results: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadFileParams {
    path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GitStatusParams {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GitDiffParams {
    #[serde(default)]
    base_ref: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplyFileEditsParams {
    operations: Vec<Value>,
    #[serde(default)]
    approval_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunCommandParams {
    command: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    purpose: Option<String>,
    #[serde(default)]
    approval_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateTodosParams {
    todos: Vec<TodoInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TodoInput {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestApprovalParams {
    approval_type: String,
    risk_level: String,
    reason: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompleteTaskParams {
    summary: String,
}

fn list_files(task: &TaskRecord, params: ListFilesParams) -> Result<Value, ToolFailure> {
    let root = resolve_optional_path(task, params.path.as_deref())?;
    let max_entries = params
        .max_entries
        .unwrap_or(MAX_LIST_ENTRIES)
        .clamp(1, MAX_LIST_ENTRIES);
    let mut entries = Vec::new();
    collect_files(&worktree_root(task)?, &root, max_entries, &mut entries)?;
    Ok(json!({"entries": entries, "truncated": entries.len() >= max_entries}))
}

fn collect_files(
    root: &Path,
    directory: &Path,
    max_entries: usize,
    entries: &mut Vec<Value>,
) -> Result<(), ToolFailure> {
    for entry in fs::read_dir(directory).map_err(|_| {
        ToolFailure::new(
            "tool.executionFailed",
            "Workspace files could not be listed.",
        )
    })? {
        if entries.len() >= max_entries {
            return Ok(());
        }
        let entry = entry.map_err(|_| {
            ToolFailure::new(
                "tool.executionFailed",
                "Workspace files could not be listed.",
            )
        })?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| {
            ToolFailure::new(
                "tool.executionFailed",
                "Workspace files could not be listed.",
            )
        })?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| ToolFailure::new("tool.invalidPath", "The requested path is invalid."))?;
        if metadata.is_dir() {
            entries.push(json!({"path": relative.to_string_lossy(), "type": "directory"}));
            collect_files(root, &entry.path(), max_entries, entries)?;
        } else if metadata.is_file() {
            entries.push(json!({"path": relative.to_string_lossy(), "type": "file", "sizeBytes": metadata.len()}));
        }
    }
    Ok(())
}

fn search_text(task: &TaskRecord, params: SearchTextParams) -> Result<Value, ToolFailure> {
    if params.query.trim().is_empty() {
        return Err(ToolFailure::new(
            "tool.invalidParams",
            "Tool parameters are invalid.",
        ));
    }
    let root = worktree_root(task)?;
    let start = resolve_optional_path(task, params.path.as_deref())?;
    let max_results = params
        .max_results
        .unwrap_or(MAX_SEARCH_RESULTS)
        .clamp(1, MAX_SEARCH_RESULTS);
    let mut files = Vec::new();
    collect_regular_files(&start, &mut files)?;
    let mut matches = Vec::new();
    for file in files {
        if matches.len() >= max_results
            || fs::metadata(&file)
                .map(|metadata| metadata.len() > MAX_SEARCH_FILE_BYTES)
                .unwrap_or(true)
        {
            continue;
        }
        let relative = file
            .strip_prefix(&root)
            .map_err(|_| ToolFailure::new("tool.invalidPath", "The requested path is invalid."))?;
        let text = match safe_fs::read_utf8(&root, &relative.to_string_lossy()) {
            Ok(text) => text,
            Err(_) => continue,
        };
        for (index, line) in text.lines().enumerate() {
            if line.contains(&params.query) {
                matches.push(json!({"path": relative.to_string_lossy(), "line": index + 1}));
                if matches.len() >= max_results {
                    break;
                }
            }
        }
    }
    Ok(json!({"matches": matches, "truncated": matches.len() >= max_results}))
}

fn collect_regular_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), ToolFailure> {
    for entry in fs::read_dir(directory).map_err(|_| {
        ToolFailure::new(
            "tool.executionFailed",
            "Workspace files could not be searched.",
        )
    })? {
        let entry = entry.map_err(|_| {
            ToolFailure::new(
                "tool.executionFailed",
                "Workspace files could not be searched.",
            )
        })?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| {
            ToolFailure::new(
                "tool.executionFailed",
                "Workspace files could not be searched.",
            )
        })?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_regular_files(&entry.path(), files)?;
        } else if metadata.is_file() {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn read_file(task: &TaskRecord, params: ReadFileParams) -> Result<Value, ToolFailure> {
    let root = worktree_root(task)?;
    let relative = strict_relative(&params.path)?;
    let content = safe_fs::read_utf8(&root, &relative.to_string_lossy()).map_err(|_| {
        ToolFailure::new(
            "tool.executionFailed",
            "The requested file could not be read.",
        )
    })?;
    Ok(json!({"path": relative.to_string_lossy(), "content": content}))
}

fn git_status(task: &TaskRecord, _: GitStatusParams) -> Result<Value, ToolFailure> {
    let status = git::worktree_status(&task.id, worktree_root(task)?)
        .map_err(|_| ToolFailure::new("tool.executionFailed", "Git status could not be read."))?;
    serde_json::to_value(status)
        .map_err(|_| ToolFailure::new("tool.executionFailed", "Git status could not be encoded."))
}

fn git_diff(task: &TaskRecord, params: GitDiffParams) -> Result<Value, ToolFailure> {
    let root = worktree_root(task)?;
    let base_ref = params
        .base_ref
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| task.target_branch.clone());
    let diff = git::task_diff(&task.id, root, &base_ref)
        .map_err(|_| ToolFailure::new("tool.executionFailed", "Git diff could not be read."))?;
    serde_json::to_value(diff)
        .map_err(|_| ToolFailure::new("tool.executionFailed", "Git diff could not be encoded."))
}

fn update_todos(
    storage: &ManagedStorage,
    task_id: &str,
    params: UpdateTodosParams,
) -> Result<Value, ToolFailure> {
    let store = storage.store.lock().map_err(|_| {
        ToolFailure::new("tool.storageUnavailable", "Local storage is unavailable.")
    })?;
    let todos = TodoRepository::new(store.connection());
    let mut updated = Vec::new();
    for todo in params.todos {
        if strict_identifier(&todo.id).is_err()
            || todo.title.trim().is_empty()
            || !matches!(
                todo.status.as_str(),
                "pending" | "in_progress" | "completed" | "failed" | "skipped"
            )
        {
            return Err(ToolFailure::new(
                "tool.invalidParams",
                "Tool parameters are invalid.",
            ));
        }
        let id = format!("todo-{task_id}-{}", todo.id);
        let item = todos
            .upsert(NewTodo {
                id: &id,
                task_id,
                title: todo.title.trim(),
                description: todo.description.trim(),
                status: &todo.status,
            })
            .map_err(|_| ToolFailure::new("tool.executionFailed", "Todos could not be updated."))?;
        updated.push(json!({"id": item.id, "status": item.status}));
    }
    Ok(json!({"todos": updated}))
}

fn request_approval(
    storage: &ManagedStorage,
    task_id: &str,
    params: RequestApprovalParams,
) -> Result<Value, ToolFailure> {
    if params.approval_type.trim().is_empty()
        || params.reason.trim().is_empty()
        || !matches!(
            params.risk_level.as_str(),
            "low" | "medium" | "high" | "critical"
        )
    {
        return Err(ToolFailure::new(
            "tool.invalidParams",
            "Tool parameters are invalid.",
        ));
    }
    let id = format!("approval-{}", Uuid::new_v4());
    let store = storage.store.lock().map_err(|_| {
        ToolFailure::new("tool.storageUnavailable", "Local storage is unavailable.")
    })?;
    let approval = ApprovalRepository::new(store.connection())
        .create(NewApproval {
            id: &id,
            task_id,
            approval_type: params.approval_type.trim(),
            risk_level: &params.risk_level,
            content: "Agent requested approval.",
            reason: params.reason.trim(),
        })
        .map_err(|_| {
            ToolFailure::new("tool.executionFailed", "Approval could not be requested.")
        })?;
    Ok(json!({"approvalId": approval.id, "status": "pending", "decision": approval.decision}))
}

fn complete_task(
    storage: &ManagedStorage,
    task_id: &str,
    params: CompleteTaskParams,
) -> Result<Value, ToolFailure> {
    if params.summary.trim().is_empty() {
        return Err(ToolFailure::new(
            "tool.invalidParams",
            "Tool parameters are invalid.",
        ));
    }
    let completed_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let store = storage.store.lock().map_err(|_| {
        ToolFailure::new("tool.storageUnavailable", "Local storage is unavailable.")
    })?;
    TaskRepository::new(store.connection())
        .update_status(task_id, "completed", Some(&completed_at))
        .map_err(|_| {
            ToolFailure::new("tool.executionFailed", "The task could not be completed.")
        })?;
    Ok(json!({"status": "completed"}))
}

fn worktree_root(task: &TaskRecord) -> Result<PathBuf, ToolFailure> {
    let path = task.worktree_path.as_deref().ok_or_else(|| {
        ToolFailure::new(
            "tool.worktreeUnavailable",
            "The task worktree is unavailable.",
        )
    })?;
    let root = PathBuf::from(path).canonicalize().map_err(|_| {
        ToolFailure::new(
            "tool.worktreeUnavailable",
            "The task worktree is unavailable.",
        )
    })?;
    if !root.is_dir() {
        return Err(ToolFailure::new(
            "tool.worktreeUnavailable",
            "The task worktree is unavailable.",
        ));
    }
    Ok(root)
}

fn resolve_optional_path(task: &TaskRecord, path: Option<&str>) -> Result<PathBuf, ToolFailure> {
    let root = worktree_root(task)?;
    let Some(path) = path.map(str::trim).filter(|path| !path.is_empty()) else {
        return Ok(root);
    };
    let relative = strict_relative(path)?;
    let candidate = root.join(relative);
    let resolved = candidate
        .canonicalize()
        .map_err(|_| ToolFailure::new("tool.invalidPath", "The requested path is invalid."))?;
    if !resolved.starts_with(&root) {
        return Err(ToolFailure::new(
            "tool.invalidPath",
            "The requested path is invalid.",
        ));
    }
    Ok(resolved)
}

fn strict_relative(value: &str) -> Result<PathBuf, ToolFailure> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ToolFailure::new(
            "tool.invalidPath",
            "The requested path is invalid.",
        ));
    }
    #[cfg(windows)]
    if path
        .components()
        .any(|component| component.as_os_str().to_string_lossy().contains(':'))
    {
        return Err(ToolFailure::new(
            "tool.invalidPath",
            "The requested path is invalid.",
        ));
    }
    Ok(path.to_path_buf())
}

fn strict_identifier(value: &str) -> Result<(), ToolFailure> {
    if value.is_empty()
        || value.len() > 120
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(ToolFailure::new(
            "tool.invalidParams",
            "Tool parameters are invalid.",
        ));
    }
    Ok(())
}

fn strict_file_operations(values: Vec<Value>) -> Result<Vec<SafeFileOperation>, ToolFailure> {
    values
        .into_iter()
        .map(|value| {
            let object = value.as_object().ok_or_else(|| {
                ToolFailure::new("tool.invalidParams", "Tool parameters are invalid.")
            })?;
            let operation = object
                .get("operation")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ToolFailure::new("tool.invalidParams", "Tool parameters are invalid.")
                })?;
            let allowed_keys: &[&str] = match operation {
                "create" | "update" => &["operation", "path", "content"],
                "delete" => &["operation", "path"],
                "rename" => &["operation", "path", "destination"],
                _ => {
                    return Err(ToolFailure::new(
                        "tool.invalidParams",
                        "Tool parameters are invalid.",
                    ))
                }
            };
            if object.len() != allowed_keys.len()
                || object
                    .keys()
                    .any(|key| !allowed_keys.contains(&key.as_str()))
            {
                return Err(ToolFailure::new(
                    "tool.invalidParams",
                    "Tool parameters are invalid.",
                ));
            }
            serde_json::from_value(value)
                .map_err(|_| ToolFailure::new("tool.invalidParams", "Tool parameters are invalid."))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, sync::Mutex};

    use serde_json::json;
    use uuid::Uuid;

    use crate::storage::{
        AgentToolCallRepository, ManagedStorage, NewTask, SqliteStore, StorageRoots, TaskRepository,
    };

    use super::{dispatch_agent_tool_call, AgentToolCallRequest};

    fn test_storage() -> (ManagedStorage, PathBuf) {
        let root = std::env::temp_dir().join(format!("codemax-dispatch-{}", Uuid::new_v4()));
        let worktree = root.join("worktree");
        fs::create_dir_all(&worktree).expect("create worktree");
        fs::write(worktree.join("needle.txt"), "find this needle\n").expect("write fixture");

        let store = SqliteStore::open_in_memory().expect("open sqlite");
        store.migrate().expect("migrate sqlite");
        TaskRepository::new(store.connection())
            .create(NewTask {
                id: "task-001",
                title: "Dispatcher test",
                description: "Exercise the runtime dispatcher",
                task_type: "custom",
                status: "editing",
                repository_path: root.to_string_lossy().as_ref(),
                worktree_path: Some(worktree.to_string_lossy().as_ref()),
                branch_name: Some("codex/task-001"),
                target_branch: "main",
                workspace_kind: "git_worktree",
                source_path: root.to_string_lossy().as_ref(),
                original_write_authorized: false,
                workspace_estimated_bytes: 0,
                model_id: None,
            })
            .expect("create task");

        (
            ManagedStorage {
                roots: StorageRoots::from_app_data_dir(&root),
                store: Mutex::new(store),
            },
            root,
        )
    }

    fn request(call_id: &str, tool_name: &str, params: serde_json::Value) -> AgentToolCallRequest {
        AgentToolCallRequest {
            task_id: "task-001".to_string(),
            call_id: call_id.to_string(),
            tool_name: tool_name.to_string(),
            params,
        }
    }

    #[tokio::test]
    async fn dispatch_search_text_returns_workspace_matches_and_completes_audit() {
        let (storage, root) = test_storage();

        let result = dispatch_agent_tool_call(
            &storage,
            request("call-search", "search_text", json!({"query": "needle"})),
        )
        .await;

        assert_eq!(result.status, "succeeded");
        assert_eq!(result.output["matches"][0]["path"], "needle.txt");
        assert_eq!(result.output["matches"][0]["line"], 1);
        assert!(result.error.is_none());
        assert_eq!(result.audit_status, "succeeded");

        fs::remove_dir_all(root).expect("clean temp files");
    }

    #[tokio::test]
    async fn dispatch_rejects_paths_outside_the_task_worktree_without_side_effects() {
        let (storage, root) = test_storage();

        let result = dispatch_agent_tool_call(
            &storage,
            request(
                "call-escape",
                "read_file",
                json!({"path": "../outside.txt"}),
            ),
        )
        .await;

        assert_eq!(result.status, "failed");
        assert_eq!(
            result.error.as_ref().map(|error| error.code.as_str()),
            Some("tool.invalidPath")
        );
        assert_eq!(result.audit_status, "failed");
        assert!(!result
            .error
            .expect("failure")
            .message
            .contains("outside.txt"));

        fs::remove_dir_all(root).expect("clean temp files");
    }

    #[tokio::test]
    async fn dispatch_rejects_unknown_tools_with_a_completed_sanitized_audit() {
        let (storage, root) = test_storage();

        let result = dispatch_agent_tool_call(
            &storage,
            request(
                "call-unknown",
                "erase_everything",
                json!({"secret": "do-not-echo"}),
            ),
        )
        .await;

        assert_eq!(result.status, "failed");
        assert_eq!(
            result.error.as_ref().map(|error| error.code.as_str()),
            Some("tool.unknown")
        );
        assert_eq!(result.audit_status, "failed");
        assert!(!result
            .error
            .expect("failure")
            .message
            .contains("erase_everything"));
        let store = storage.store.lock().expect("storage lock");
        let audit = AgentToolCallRepository::new(store.connection())
            .get_required("task-001", "call-unknown")
            .expect("load failed audit");
        assert_eq!(audit.status, "failed");
        assert!(!audit.request_summary.contains("do-not-echo"));
        assert!(!audit
            .result_summary
            .expect("result summary")
            .contains("erase_everything"));
        drop(store);

        fs::remove_dir_all(root).expect("clean temp files");
    }

    #[tokio::test]
    async fn dispatch_apply_file_edits_uses_the_call_id_as_its_transaction_request_id() {
        let (storage, root) = test_storage();

        let result = dispatch_agent_tool_call(
            &storage,
            request(
                "call-edit",
                "apply_file_edits",
                json!({"operations": [{"operation": "create", "path": "created.txt", "content": "written"}]}),
            ),
        )
        .await;

        assert_eq!(result.status, "succeeded");
        assert_eq!(
            fs::read_to_string(root.join("worktree").join("created.txt"))
                .expect("read created file"),
            "written"
        );
        assert_eq!(result.output["requestId"], "call-edit");
        assert!(result.output["transactionId"].is_string());
        let store = storage.store.lock().expect("storage lock");
        let request_id: String = store
            .connection()
            .query_row(
                "SELECT request_id FROM file_edit_transactions WHERE task_id = ?1",
                ["task-001"],
                |row| row.get(0),
            )
            .expect("load edit transaction");
        assert_eq!(request_id, "call-edit");
        drop(store);

        fs::remove_dir_all(root).expect("clean temp files");
    }

    #[tokio::test]
    async fn dispatch_replays_completed_calls_without_repeating_file_edits() {
        let (storage, root) = test_storage();
        let first = dispatch_agent_tool_call(
            &storage,
            request(
                "call-replay",
                "apply_file_edits",
                json!({"operations": [{"operation": "create", "path": "once.txt", "content": "once"}]}),
            ),
        )
        .await;
        let second = dispatch_agent_tool_call(
            &storage,
            request(
                "call-replay",
                "apply_file_edits",
                json!({"operations": [{"operation": "create", "path": "once.txt", "content": "once"}]}),
            ),
        )
        .await;

        assert_eq!(first.status, "succeeded");
        assert_eq!(second.status, "succeeded");
        assert!(second.replayed);
        assert_eq!(
            fs::read_to_string(root.join("worktree").join("once.txt")).expect("read edited file"),
            "once"
        );

        fs::remove_dir_all(root).expect("clean temp files");
    }

    #[tokio::test]
    async fn dispatch_rejects_unknown_file_edit_operation_fields_before_the_transaction() {
        let (storage, root) = test_storage();

        let result = dispatch_agent_tool_call(
            &storage,
            request(
                "call-invalid-edit",
                "apply_file_edits",
                json!({"operations": [{"operation": "create", "path": "created.txt", "content": "written", "unexpected": true}]}),
            ),
        )
        .await;

        assert_eq!(result.status, "failed");
        assert_eq!(
            result.error.as_ref().map(|error| error.code.as_str()),
            Some("tool.invalidParams")
        );
        assert!(!root.join("worktree").join("created.txt").exists());

        fs::remove_dir_all(root).expect("clean temp files");
    }

    #[tokio::test]
    async fn dispatch_request_approval_creates_a_pending_approval_without_approving_it() {
        let (storage, root) = test_storage();

        let result = dispatch_agent_tool_call(
            &storage,
            request(
                "call-approval",
                "request_approval",
                json!({"approvalType": "command", "riskLevel": "high", "reason": "requires review"}),
            ),
        )
        .await;

        assert_eq!(result.status, "succeeded");
        assert_eq!(result.output["decision"], serde_json::Value::Null);
        assert_eq!(result.output["status"], "pending");

        fs::remove_dir_all(root).expect("clean temp files");
    }

    #[tokio::test]
    async fn dispatch_complete_task_marks_the_task_completed() {
        let (storage, root) = test_storage();

        let result = dispatch_agent_tool_call(
            &storage,
            request(
                "call-complete",
                "complete_task",
                json!({"summary": "ready for review"}),
            ),
        )
        .await;

        assert_eq!(result.status, "succeeded");
        assert_eq!(result.output["status"], "completed");
        let store = storage.store.lock().expect("storage lock");
        let task = TaskRepository::new(store.connection())
            .get_required("task-001")
            .expect("load completed task");
        assert_eq!(task.status, "completed");
        assert!(task.completed_at.is_some());
        drop(store);

        fs::remove_dir_all(root).expect("clean temp files");
    }
}
