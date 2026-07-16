use std::{collections::BTreeMap, fs, io::Read, path::Path};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::{
    agent::{AgentHealthResponse, AgentService, AgentServiceError, AgentServiceStatus},
    core::error::{AppResult, CommandError},
    exec::{CommandExecutionResult, CommandRequest, CommandRunRegistry},
    privacy::{
        estimate_tokens, record_context_observation, record_token_budget_observation,
        sanitize_for_model_context, sync_model_request_audits, ContextObservation,
        SanitizedContent, TokenBudgetObservation,
    },
    safe_fs::SafeFileOperation,
    storage::{
        AgentEventRepository, AgentSessionRepository, CommandRunRepository, ManagedStorage,
        NewAgentEvent, NewAgentSession, NewTodo, RunContractRepository, StorageError,
        TaskRepository, TodoRepository,
    },
};

use super::{
    exec::run_task_command,
    files::{
        execute_transaction, ExecuteSafeFileOperationsRequest, ExecuteSafeFileOperationsResponse,
    },
    models::load_agent_runtime_env,
};

const VALIDATION_LOG_TAIL_BYTES: usize = 64 * 1024;
const DEFAULT_VALIDATION_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_VALIDATION_LOOP_LIMIT: usize = 12;
const DEFAULT_AGENT_BUDGET_LIMIT: i64 = 120_000;

#[derive(Debug, Clone)]
struct ContractBudget {
    model_id: Option<String>,
    token_budget_total: i64,
    token_budget_per_call: i64,
    overflow_policy: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskCreateRequest {
    pub task_id: String,
    pub repository_path: String,
    pub worktree_path: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub model_id: Option<String>,
    pub validation_command: Option<String>,
    #[serde(default)]
    pub token_budget: Option<i64>,
    #[serde(default)]
    pub token_budget_per_call: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskAdvanceRequest {
    pub reason: Option<String>,
    pub user_message: Option<String>,
    #[serde(default)]
    pub require_approval: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentValidationResultRequest {
    pub run_id: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub timed_out: bool,
    #[serde(default)]
    pub cancelled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunAgentValidationCycleRequest {
    pub task_id: String,
    pub reason: Option<String>,
    pub timeout_ms: Option<u64>,
    pub max_iterations: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunAgentValidationCycleResponse {
    pub task_id: String,
    pub phase: String,
    pub iterations: usize,
    pub command_results: Vec<CommandExecutionResult>,
    pub state: Value,
}

#[tauri::command]
pub async fn start_agent_service(
    agent: State<'_, AgentService>,
    storage: State<'_, ManagedStorage>,
) -> AppResult<AgentServiceStatus> {
    prepare_agent_runtime(agent.inner(), storage.inner()).await?;
    agent.start().await.map_err(agent_error)
}

#[tauri::command]
pub async fn stop_agent_service(agent: State<'_, AgentService>) -> AppResult<AgentServiceStatus> {
    agent.stop().await.map_err(agent_error)
}

#[tauri::command]
pub async fn get_agent_service_status(
    agent: State<'_, AgentService>,
) -> AppResult<AgentServiceStatus> {
    agent.status().await.map_err(agent_error)
}

#[tauri::command]
pub async fn check_agent_health(agent: State<'_, AgentService>) -> AppResult<AgentHealthResponse> {
    agent.health_check().await.map_err(agent_error)
}

#[tauri::command]
pub async fn create_agent_task(
    agent: State<'_, AgentService>,
    storage: State<'_, ManagedStorage>,
    mut request: AgentTaskCreateRequest,
) -> AppResult<Value> {
    prepare_agent_runtime(agent.inner(), storage.inner()).await?;
    let mut contract = load_contract_budget(storage.inner(), &request.task_id)?;
    let model_id =
        resolve_new_task_model_id(request.model_id.as_deref(), contract.model_id.as_deref())?;
    request.model_id = Some(model_id.clone());
    contract.model_id = Some(model_id);
    request.token_budget = Some(contract.token_budget_total);
    request.token_budget_per_call = Some(contract.token_budget_per_call);
    let sanitized_title = sanitize_for_model_context(&request.title, "agent.task.title");
    let sanitized_description =
        sanitize_for_model_context(&request.description, "agent.task.description");
    let sanitized_validation_command = request
        .validation_command
        .as_deref()
        .map(|command| sanitize_for_model_context(command, "agent.task.validationCommand"));
    let repository_path_context = allowed_context(&request.repository_path);
    let worktree_path_context = allowed_context(&request.worktree_path);

    request.title = sanitized_title.content.clone();
    request.description = sanitized_description.content.clone();
    if let Some(sanitized) = sanitized_validation_command.as_ref() {
        request.validation_command = Some(sanitized.content.clone());
    }

    record_agent_create_contexts(
        storage.inner(),
        &request.task_id,
        contract.model_id.as_deref(),
        &sanitized_title,
        &sanitized_description,
        sanitized_validation_command.as_ref(),
        &repository_path_context,
        &worktree_path_context,
    )?;
    let input_tokens = sanitized_title.tokens_estimate
        + sanitized_description.tokens_estimate
        + sanitized_validation_command
            .as_ref()
            .map(|content| content.tokens_estimate)
            .unwrap_or(0)
        + repository_path_context.tokens_estimate
        + worktree_path_context.tokens_estimate;
    record_agent_token_budget(
        storage.inner(),
        &request.task_id,
        Some("agent-create"),
        "agent_task_create",
        "created",
        input_tokens,
        &contract,
    )?;

    let task_id = request.task_id.clone();
    let body = serde_json::to_value(request).map_err(json_error)?;
    let response = agent
        .api_json("POST", "/api/v1/tasks", Some(body))
        .await
        .map_err(agent_error)?;
    if let Ok(state) = response_state(&response) {
        sync_agent_state(storage.inner(), &task_id, state)?;
    }
    Ok(response)
}

#[tauri::command]
pub async fn get_agent_task_state(
    agent: State<'_, AgentService>,
    storage: State<'_, ManagedStorage>,
    task_id: String,
) -> AppResult<Value> {
    prepare_agent_runtime(agent.inner(), storage.inner()).await?;
    let path = format!("/api/v1/tasks/{}", encode_path_segment(&task_id));
    agent
        .api_json("GET", &path, None)
        .await
        .map_err(agent_error)
}

#[tauri::command]
pub async fn advance_agent_task(
    agent: State<'_, AgentService>,
    storage: State<'_, ManagedStorage>,
    task_id: String,
    request: AgentTaskAdvanceRequest,
) -> AppResult<Value> {
    prepare_agent_runtime(agent.inner(), storage.inner()).await?;
    let task_id = task_id.trim().to_string();
    if task_id.is_empty() {
        return Err(CommandError::new(
            "agent.taskIdRequired",
            "Agent task id is required.",
        ));
    }
    let path = format!("/api/v1/tasks/{}/advance", encode_path_segment(&task_id));
    let body = serde_json::to_value(request).map_err(json_error)?;
    let response = agent
        .api_json("POST", &path, Some(body))
        .await
        .map_err(agent_error)?;
    let response =
        complete_pending_file_commit(agent.inner(), storage.inner(), &task_id, response).await?;
    if let Ok(state) = response_state(&response) {
        sync_agent_state(storage.inner(), &task_id, state)?;
    }
    Ok(response)
}

#[tauri::command]
pub async fn submit_agent_validation_result(
    agent: State<'_, AgentService>,
    storage: State<'_, ManagedStorage>,
    task_id: String,
    mut request: AgentValidationResultRequest,
) -> AppResult<Value> {
    prepare_agent_runtime(agent.inner(), storage.inner()).await?;
    let path = format!(
        "/api/v1/tasks/{}/validation-result",
        encode_path_segment(&task_id)
    );
    sanitize_and_record_validation_result(storage.inner(), &task_id, &mut request)?;
    let body = serde_json::to_value(request).map_err(json_error)?;
    let response = agent
        .api_json("POST", &path, Some(body))
        .await
        .map_err(agent_error)?;
    let response =
        complete_pending_file_commit(agent.inner(), storage.inner(), &task_id, response).await?;
    if let Ok(state) = response_state(&response) {
        sync_agent_state(storage.inner(), &task_id, state)?;
    }
    Ok(response)
}

#[tauri::command]
pub async fn run_agent_validation_cycle(
    app: AppHandle,
    agent: State<'_, AgentService>,
    storage: State<'_, ManagedStorage>,
    registry: State<'_, CommandRunRegistry>,
    request: RunAgentValidationCycleRequest,
) -> AppResult<RunAgentValidationCycleResponse> {
    prepare_agent_runtime(agent.inner(), storage.inner()).await?;
    let task_id = request.task_id.trim().to_string();
    if task_id.is_empty() {
        return Err(CommandError::new(
            "agent.taskIdRequired",
            "Agent task id is required.",
        ));
    }

    let task_path = encode_path_segment(&task_id);
    let advance_body = json!({
        "reason": request.reason,
        "requireApproval": false,
    });
    let mut response = agent
        .api_json(
            "POST",
            &format!("/api/v1/tasks/{task_path}/advance"),
            Some(advance_body),
        )
        .await
        .map_err(agent_error)?;
    response =
        complete_pending_file_commit(agent.inner(), storage.inner(), &task_id, response).await?;
    sync_agent_state(storage.inner(), &task_id, response_state(&response)?)?;

    let limit = request
        .max_iterations
        .unwrap_or(DEFAULT_VALIDATION_LOOP_LIMIT)
        .clamp(1, 25);
    let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_VALIDATION_TIMEOUT_MS);
    let mut command_results = Vec::new();

    for iteration in 0..limit {
        let state = response_state(&response)?;
        let Some(validation) = validation_request(state) else {
            break;
        };

        let command_result = run_task_command(
            &app,
            storage.inner(),
            registry.inner(),
            CommandRequest {
                task_id: task_id.clone(),
                run_id: Some(format!("validation-{}-{}", iteration + 1, Uuid::new_v4())),
                command: validation.command.clone(),
                cwd: validation.cwd.clone(),
                env: BTreeMap::new(),
                timeout_ms: Some(timeout_ms),
                purpose: Some("validation".to_string()),
                approval_id: None,
            },
        )
        .await?;

        let stdout = read_log_tail(Path::new(&command_result.stdout_path));
        let stderr = read_log_tail(Path::new(&command_result.stderr_path));
        let mut validation_result = AgentValidationResultRequest {
            run_id: Some(command_result.run_id.clone()),
            command: Some(command_result.command.clone()),
            cwd: Some(command_result.cwd.clone()),
            stdout,
            stderr,
            exit_code: command_result.exit_code,
            timed_out: command_result.timed_out,
            cancelled: command_result.cancelled,
        };
        sanitize_and_record_validation_result(storage.inner(), &task_id, &mut validation_result)?;
        let result_body = serde_json::to_value(&validation_result).map_err(json_error)?;
        command_results.push(command_result);

        response = agent
            .api_json(
                "POST",
                &format!("/api/v1/tasks/{task_path}/validation-result"),
                Some(result_body),
            )
            .await
            .map_err(agent_error)?;
        response = complete_pending_file_commit(agent.inner(), storage.inner(), &task_id, response)
            .await?;
        sync_agent_state(storage.inner(), &task_id, response_state(&response)?)?;
    }

    let state = response_state(&response)?.clone();
    let phase = state
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    Ok(RunAgentValidationCycleResponse {
        task_id,
        phase,
        iterations: command_results.len(),
        command_results,
        state,
    })
}

async fn prepare_agent_runtime(agent: &AgentService, storage: &ManagedStorage) -> AppResult<()> {
    let runtime_env = load_agent_runtime_env(storage, None)?;
    let changed = agent.set_runtime_env(runtime_env).map_err(agent_error)?;
    if changed {
        agent.stop().await.map_err(agent_error)?;
    }
    Ok(())
}

async fn complete_pending_file_commit(
    agent: &AgentService,
    storage: &ManagedStorage,
    task_id: &str,
    response: Value,
) -> AppResult<Value> {
    let state = response_state(&response)?;
    if state.get("phase").and_then(Value::as_str) != Some("awaiting_file_commit") {
        return Ok(response);
    }
    let commit_id = state
        .get("pendingFileCommitId")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CommandError::new(
                "agent.fileCommitIdMissing",
                "Pending file commit has no id.",
            )
        })?;
    let edits = state
        .get("editPlan")
        .and_then(|plan| plan.get("edits"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            CommandError::new(
                "agent.fileCommitPlanMissing",
                "Pending file commit has no edit plan.",
            )
        })?;
    record_file_commit_event(
        storage,
        task_id,
        commit_id,
        "file_commit_intent",
        "pending",
        edits,
        None,
        None,
    )?;
    let result = execute_file_commit_transaction(storage, task_id, commit_id, edits);
    let (success, error) = match result {
        Ok(transaction) => {
            record_file_commit_event(
                storage,
                task_id,
                commit_id,
                "file_commit_completed",
                "editing",
                edits,
                None,
                Some(&transaction),
            )?;
            (true, None)
        }
        Err(error)
            if matches!(
                error.code.as_str(),
                "approval.required" | "approval.pending" | "approval.reviseRequested"
            ) =>
        {
            record_file_commit_event(
                storage,
                task_id,
                commit_id,
                "file_commit_awaiting_approval",
                "awaitingApproval",
                edits,
                Some(&error.message),
                None,
            )?;
            return Ok(response);
        }
        Err(error) => {
            let message = error.message.clone();
            record_file_commit_event(
                storage,
                task_id,
                commit_id,
                "file_commit_failed",
                "failed",
                edits,
                Some(&message),
                None,
            )?;
            (false, Some(message))
        }
    };
    let path = format!(
        "/api/v1/tasks/{}/file-commit-result",
        encode_path_segment(task_id)
    );
    agent
        .api_json(
            "POST",
            &path,
            Some(json!({"commitId": commit_id, "success": success, "error": error})),
        )
        .await
        .map_err(agent_error)
}

fn execute_file_commit_transaction(
    storage: &ManagedStorage,
    task_id: &str,
    commit_id: &str,
    edits: &[Value],
) -> AppResult<ExecuteSafeFileOperationsResponse> {
    execute_transaction(
        storage,
        ExecuteSafeFileOperationsRequest {
            task_id: task_id.to_string(),
            request_id: commit_id.to_string(),
            operations: file_commit_operations_from_edits(edits)?,
            approval_id: None,
            diff_artifact_id: None,
            validation_round_id: None,
            proof_pack_id: None,
        },
    )
}

fn file_commit_operations_from_edits(edits: &[Value]) -> AppResult<Vec<SafeFileOperation>> {
    edits.iter().map(file_commit_operation_from_edit).collect()
}

fn file_commit_operation_from_edit(edit: &Value) -> AppResult<SafeFileOperation> {
    let operation = edit
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CommandError::new("agent.fileCommitInvalid", "Edit operation is missing.")
        })?;
    let path = edit
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| CommandError::new("agent.fileCommitInvalid", "Edit path is missing."))?
        .to_string();
    match operation {
        "create" => Ok(SafeFileOperation::Create {
            path,
            content: edit_content(edit, "Create")?,
        }),
        "update" => Ok(SafeFileOperation::Update {
            path,
            content: edit_content(edit, "Update")?,
        }),
        "delete" => Ok(SafeFileOperation::Delete { path }),
        _ => Err(CommandError::new(
            "agent.fileCommitInvalid",
            "Unsupported edit operation.",
        )),
    }
}

fn edit_content(edit: &Value, label: &str) -> AppResult<String> {
    edit.get("content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            CommandError::new(
                "agent.fileCommitInvalid",
                format!("{label} content is missing."),
            )
        })
}

fn record_file_commit_event(
    storage: &ManagedStorage,
    task_id: &str,
    commit_id: &str,
    event_type: &str,
    stage: &str,
    edits: &[Value],
    error: Option<&str>,
    transaction: Option<&ExecuteSafeFileOperationsResponse>,
) -> AppResult<()> {
    let payload = serde_json::to_string(&json!({
        "commitId": commit_id,
        "operationCount": edits.len(),
        "operations": edits.iter().map(|edit| json!({"operation": edit.get("operation"), "path": edit.get("path")})).collect::<Vec<_>>(),
        "transactionId": transaction.map(|transaction| transaction.transaction_id.as_str()),
        "transactionStatus": transaction.map(|transaction| transaction.status.as_str()),
        "transactionResultCount": transaction.map(|transaction| transaction.results.len()),
        "errorCategory": error.map(|_| if event_type == "file_commit_awaiting_approval" { "approval_required" } else { "safe_file_operation_failed" })
    })).map_err(json_error)?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let events = AgentEventRepository::new(store.connection());
    let event_id = format!("{}-{}", event_type, commit_id);
    if events
        .list_for_task(task_id)
        .map_err(storage_error)?
        .iter()
        .any(|event| event.event_id == event_id)
    {
        return Ok(());
    }
    events
        .create(NewAgentEvent {
            event_id: &event_id,
            task_id,
            event_type,
            stage,
            message: if error.is_some() {
                "Rust safe file commit failed."
            } else {
                "Rust safe file commit checkpoint."
            },
            payload: &payload,
        })
        .map_err(storage_error)?;
    Ok(())
}

fn sync_agent_state(storage: &ManagedStorage, task_id: &str, state: &Value) -> AppResult<()> {
    let phase = state
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or("created");
    let task_status = task_status_from_agent_phase(phase);
    let checkpoint_id = state
        .get("checkpointIndex")
        .and_then(Value::as_i64)
        .map(|index| format!("{task_id}:checkpoint:{index}"));
    let repair_round = state
        .get("repairRound")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let max_repair_rounds = state
        .get("maxRepairRounds")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let validation_request = state
        .get("validationRequest")
        .cloned()
        .filter(|request| !request.is_null())
        .unwrap_or_else(|| json!({}));
    let validation_request_json = serde_json::to_string(&validation_request).map_err(json_error)?;
    let repair_file_edits = current_repair_file_edits(state);
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    sync_model_request_audits(connection, task_id, state).map_err(storage_error)?;
    let previous_session = AgentSessionRepository::new(connection)
        .get_for_task(task_id)
        .map_err(storage_error)?;
    let validation_runs = CommandRunRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?;
    let validation_round = validation_runs
        .iter()
        .filter(|run| run.purpose == "validation")
        .count() as i64;
    let iterations = state
        .get("iterations")
        .and_then(Value::as_i64)
        .unwrap_or(validation_round);

    TaskRepository::new(connection)
        .update_status(task_id, task_status, None)
        .map_err(storage_error)?;
    let session_id = previous_session
        .as_ref()
        .map(|session| session.id.clone())
        .unwrap_or_else(|| format!("agent-session-{task_id}"));
    AgentSessionRepository::new(connection)
        .upsert(NewAgentSession {
            id: &session_id,
            task_id,
            status: phase,
            stage: task_status,
            checkpoint_id: checkpoint_id.as_deref(),
            iterations,
            repair_round,
            max_repair_rounds,
            validation_request_json: &validation_request_json,
            validation_round,
        })
        .map_err(storage_error)?;

    if should_record_stage_change(previous_session.as_ref(), phase, checkpoint_id.as_deref()) {
        record_agent_event_with_connection(
            &AgentEventRepository::new(connection),
            task_id,
            "task.stage.changed",
            task_status,
            &format!("Agent moved into {phase}."),
            json!({
                "phase": phase,
                "task_status": task_status,
                "checkpoint_id": checkpoint_id,
                "repair_round": repair_round,
            }),
        )?;
    }

    if let Some(Value::Array(todos)) = state.get("todos") {
        let todos_repo = TodoRepository::new(connection);
        let events = AgentEventRepository::new(connection);
        for todo in todos {
            let Some(agent_todo_id) = todo.get("id").and_then(Value::as_str) else {
                continue;
            };
            let title = todo
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Agent todo");
            let description = todo
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let status = todo
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("pending");
            let todo_id = format!("todo-{task_id}-{}", safe_id_segment(agent_todo_id));
            todos_repo
                .upsert(NewTodo {
                    id: &todo_id,
                    task_id,
                    title,
                    description,
                    status,
                })
                .map_err(storage_error)?;
            record_agent_event_with_connection(
                &events,
                task_id,
                "todo.updated",
                task_status,
                &format!("Agent todo updated: {title}"),
                json!({
                    "todo_id": todo_id,
                    "agent_todo_id": agent_todo_id,
                    "status": status,
                }),
            )?;
        }
    }

    let repair_round_advanced = repair_round > 0
        && previous_session
            .as_ref()
            .map_or(true, |session| session.repair_round < repair_round);
    let repair_started = repair_round_advanced
        || (phase == "repairing"
            && previous_session.as_ref().map_or(true, |session| {
                session.status != phase || session.repair_round != repair_round
            }));
    let repair_finished = phase == "validating"
        && repair_round > 0
        && previous_session.as_ref().map_or(true, |session| {
            session.status != phase || session.repair_round != repair_round
        });
    let repair_failed = phase == "failed" && repair_round > 0 && repair_round_advanced;

    if repair_started {
        record_agent_event_with_connection(
            &AgentEventRepository::new(connection),
            task_id,
            "repair.started",
            "repairing",
            "Agent started an automatic repair round.",
            json!({
                "repair_round": repair_round,
                "max_repair_rounds": max_repair_rounds,
                "repair_plan": state.get("repairPlan").cloned().unwrap_or_else(|| json!({})),
                "file_edits": repair_file_edits.clone(),
            }),
        )?;
    }

    if repair_failed {
        record_agent_event_with_connection(
            &AgentEventRepository::new(connection),
            task_id,
            "repair.failed",
            "failed",
            "Agent repair plan generation failed before workspace edits were applied.",
            json!({
                "repair_round": repair_round,
                "max_repair_rounds": max_repair_rounds,
                "repair_plan": state.get("repairPlan").cloned().unwrap_or_else(|| json!({})),
                "file_edits": repair_file_edits.clone(),
            }),
        )?;
    }

    if repair_finished {
        record_agent_event_with_connection(
            &AgentEventRepository::new(connection),
            task_id,
            "repair.finished",
            "validating",
            "Agent finished a repair round and requested validation.",
            json!({
                "repair_round": repair_round,
                "validation_round": validation_round,
                "validation_request": validation_request,
                "file_edits": repair_file_edits,
            }),
        )?;
    }

    Ok(())
}

fn current_repair_file_edits(state: &Value) -> Value {
    let Some(edits) = state.get("repairFileEdits").and_then(Value::as_array) else {
        return json!([]);
    };

    Value::Array(
        edits
            .iter()
            .map(|edit| {
                let mut sanitized = edit.clone();
                let summary = edit.get("summary").and_then(Value::as_str);
                if let (Some(summary), Some(object)) = (summary, sanitized.as_object_mut()) {
                    let safe_summary =
                        sanitize_for_model_context(summary, "agent.repairFileEdit.summary");
                    object.insert("summary".to_string(), Value::String(safe_summary.content));
                }
                sanitized
            })
            .collect(),
    )
}

fn should_record_stage_change(
    previous_session: Option<&crate::storage::AgentSessionRecord>,
    phase: &str,
    checkpoint_id: Option<&str>,
) -> bool {
    previous_session.map_or(true, |session| {
        session.status != phase || session.checkpoint_id.as_deref() != checkpoint_id
    })
}

fn task_status_from_agent_phase(phase: &str) -> &'static str {
    match phase {
        "created" | "planned" => "planning",
        "editing" => "editing",
        "validating" => "validating",
        "analyzing_error" | "repairing" => "repairing",
        "waiting_approval" | "awaiting_file_commit" => "awaitingApproval",
        "needs_intervention" => "needsIntervention",
        "completed" => "readyToMerge",
        "failed" => "failed",
        _ => "planning",
    }
}

fn safe_id_segment(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            output.push(character);
        } else {
            output.push('-');
        }
    }
    let output = output.trim_matches('-');
    if output.is_empty() {
        "todo".to_string()
    } else {
        output.chars().take(64).collect()
    }
}

fn record_agent_event_with_connection(
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

fn resolve_new_task_model_id(
    request_model_id: Option<&str>,
    contract_model_id: Option<&str>,
) -> AppResult<String> {
    request_model_id
        .into_iter()
        .chain(contract_model_id)
        .map(str::trim)
        .find(|model_id| !model_id.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            CommandError::new(
                "agent.modelRequired",
                "A configured model is required to create a new Agent task.",
            )
        })
}

fn load_contract_budget(storage: &ManagedStorage, task_id: &str) -> AppResult<ContractBudget> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let contract = RunContractRepository::new(store.connection())
        .get_for_task(task_id)
        .map_err(storage_error)?;

    Ok(contract
        .map(|contract| ContractBudget {
            model_id: contract.model_id,
            token_budget_total: contract.token_budget_total,
            token_budget_per_call: contract.token_budget_per_call,
            overflow_policy: contract.budget_overflow_policy,
        })
        .unwrap_or(ContractBudget {
            model_id: None,
            token_budget_total: DEFAULT_AGENT_BUDGET_LIMIT,
            token_budget_per_call: 24_000,
            overflow_policy: "pause_for_approval".to_string(),
        }))
}

fn allowed_context(content: &str) -> SanitizedContent {
    let placeholder = "[LOCAL_PATH]";
    SanitizedContent {
        content: placeholder.to_string(),
        action: "redacted".to_string(),
        sensitivity_level: "local_path".to_string(),
        findings: Vec::new(),
        redacted: true,
        blocked: false,
        reason: "Local path retained only at the runtime boundary.".to_string(),
        original_size_bytes: content.len() as i64,
        tokens_estimate: estimate_tokens(placeholder),
    }
}

#[allow(clippy::too_many_arguments)]
fn record_agent_create_contexts(
    storage: &ManagedStorage,
    task_id: &str,
    model_id: Option<&str>,
    title: &SanitizedContent,
    description: &SanitizedContent,
    validation_command: Option<&SanitizedContent>,
    repository_path: &SanitizedContent,
    worktree_path: &SanitizedContent,
) -> AppResult<()> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();

    for (data_kind, source_ref, layer, sanitized) in [
        (
            "task_title",
            "agent.task.title",
            "recent_user_request",
            title,
        ),
        (
            "task_description",
            "agent.task.description",
            "recent_user_request",
            description,
        ),
        (
            "repository_path",
            "agent.task.repositoryPath",
            "runtime_boundary",
            repository_path,
        ),
        (
            "worktree_path",
            "agent.task.worktreePath",
            "runtime_boundary",
            worktree_path,
        ),
    ] {
        record_context_observation(
            connection,
            ContextObservation {
                task_id,
                run_id: Some("agent-create"),
                event_type: "model_context",
                data_kind,
                source_type: if data_kind.ends_with("_path") {
                    "runtime_path"
                } else {
                    "user_input"
                },
                source_ref,
                destination: "python_agent",
                provider: Some("local-agent"),
                model_id,
                layer,
            },
            sanitized,
        )
        .map_err(storage_error)?;
    }

    if let Some(validation_command) = validation_command {
        record_context_observation(
            connection,
            ContextObservation {
                task_id,
                run_id: Some("agent-create"),
                event_type: "model_context",
                data_kind: "validation_command",
                source_type: "user_input",
                source_ref: "agent.task.validationCommand",
                destination: "python_agent",
                provider: Some("local-agent"),
                model_id,
                layer: "validation_policy",
            },
            validation_command,
        )
        .map_err(storage_error)?;
    }

    Ok(())
}

fn record_agent_token_budget(
    storage: &ManagedStorage,
    task_id: &str,
    run_id: Option<&str>,
    call_type: &str,
    phase: &str,
    input_tokens: i64,
    contract: &ContractBudget,
) -> AppResult<()> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    record_token_budget_observation(
        store.connection(),
        TokenBudgetObservation {
            task_id,
            run_id,
            call_type,
            provider: Some("local-agent"),
            model_id: contract.model_id.as_deref(),
            phase,
            input_tokens_estimate: input_tokens,
            output_tokens_estimate: 0,
            budget_limit: contract.token_budget_total,
            overflow_policy: &contract.overflow_policy,
            quality_fallback: "",
        },
    )
    .map_err(storage_error)
}

fn sanitize_and_record_validation_result(
    storage: &ManagedStorage,
    task_id: &str,
    request: &mut AgentValidationResultRequest,
) -> AppResult<()> {
    let contract = load_contract_budget(storage, task_id)?;
    let run_id = request.run_id.as_deref();
    let command = request
        .command
        .as_deref()
        .map(|command| sanitize_for_model_context(command, "agent.validation.command"));
    let cwd = request.cwd.as_deref().map(allowed_context);
    let stdout = sanitize_for_model_context(&request.stdout, "agent.validation.stdout");
    let stderr = sanitize_for_model_context(&request.stderr, "agent.validation.stderr");

    if let Some(command) = command.as_ref() {
        request.command = Some(command.content.clone());
    }
    request.stdout = stdout.content.clone();
    request.stderr = stderr.content.clone();

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();

    if let Some(command) = command.as_ref() {
        record_context_observation(
            connection,
            ContextObservation {
                task_id,
                run_id,
                event_type: "validation_result",
                data_kind: "validation_command",
                source_type: "tool_result",
                source_ref: "agent.validation.command",
                destination: "python_agent",
                provider: Some("local-agent"),
                model_id: contract.model_id.as_deref(),
                layer: "tool_result",
            },
            command,
        )
        .map_err(storage_error)?;
    }

    if let Some(cwd) = cwd.as_ref() {
        record_context_observation(
            connection,
            ContextObservation {
                task_id,
                run_id,
                event_type: "validation_result",
                data_kind: "validation_cwd",
                source_type: "runtime_path",
                source_ref: "agent.validation.cwd",
                destination: "python_agent",
                provider: Some("local-agent"),
                model_id: contract.model_id.as_deref(),
                layer: "tool_result",
            },
            cwd,
        )
        .map_err(storage_error)?;
    }

    for (data_kind, source_ref, sanitized) in [
        ("validation_stdout", "agent.validation.stdout", &stdout),
        ("validation_stderr", "agent.validation.stderr", &stderr),
    ] {
        record_context_observation(
            connection,
            ContextObservation {
                task_id,
                run_id,
                event_type: "validation_result",
                data_kind,
                source_type: "tool_result",
                source_ref,
                destination: "python_agent",
                provider: Some("local-agent"),
                model_id: contract.model_id.as_deref(),
                layer: "tool_result",
            },
            sanitized,
        )
        .map_err(storage_error)?;
    }

    let input_tokens = command
        .as_ref()
        .map(|content| content.tokens_estimate)
        .unwrap_or(0)
        + cwd
            .as_ref()
            .map(|content| content.tokens_estimate)
            .unwrap_or(0)
        + stdout.tokens_estimate
        + stderr.tokens_estimate;
    record_token_budget_observation(
        connection,
        TokenBudgetObservation {
            task_id,
            run_id,
            call_type: "validation_result",
            provider: Some("local-agent"),
            model_id: contract.model_id.as_deref(),
            phase: "validating",
            input_tokens_estimate: input_tokens,
            output_tokens_estimate: 0,
            budget_limit: contract.token_budget_total,
            overflow_policy: &contract.overflow_policy,
            quality_fallback: if request.exit_code == Some(0) {
                ""
            } else {
                "validation_failure_context_recorded"
            },
        },
    )
    .map_err(storage_error)?;

    Ok(())
}

fn agent_error(error: AgentServiceError) -> CommandError {
    match error {
        AgentServiceError::LockUnavailable => CommandError::new(
            "agent.lockUnavailable",
            "Python Agent service state is temporarily unavailable.",
        ),
        AgentServiceError::AgentDirMissing(path) => CommandError::new(
            "agent.directoryMissing",
            format!(
                "Python Agent service directory does not exist: {}",
                path.to_string_lossy()
            ),
        ),
        AgentServiceError::Spawn { python, source } => CommandError::new(
            "agent.spawnFailed",
            format!("Unable to start Python Agent with {python}: {source}"),
        ),
        AgentServiceError::ProcessExited { status } => CommandError::new(
            "agent.exitedEarly",
            format!("Python Agent exited before it became healthy: {status}"),
        ),
        AgentServiceError::StartupTimeout { timeout_ms } => CommandError::new(
            "agent.startupTimeout",
            format!("Python Agent did not become healthy within {timeout_ms} ms."),
        ),
        AgentServiceError::Health(message) => CommandError::new(
            "agent.healthUnavailable",
            format!("Python Agent health check failed: {message}"),
        ),
        AgentServiceError::InvalidHealthResponse(message) => CommandError::new(
            "agent.invalidHealthResponse",
            format!("Python Agent returned an invalid health response: {message}"),
        ),
        AgentServiceError::Api(message) => CommandError::new(
            "agent.apiFailed",
            format!("Python Agent API request failed: {message}"),
        ),
        AgentServiceError::InvalidApiResponse(message) => CommandError::new(
            "agent.invalidApiResponse",
            format!("Python Agent returned an invalid API response: {message}"),
        ),
        AgentServiceError::Process(error) => CommandError::new(
            "agent.processFailed",
            format!("Python Agent process operation failed: {error}"),
        ),
        AgentServiceError::Join(message) => CommandError::new(
            "agent.healthTaskFailed",
            format!("Python Agent health check task failed: {message}"),
        ),
    }
}

#[derive(Debug, Clone)]
struct ValidationRequestView {
    command: String,
    cwd: String,
}

fn response_state(response: &Value) -> AppResult<&Value> {
    response.get("state").ok_or_else(|| {
        CommandError::new(
            "agent.stateMissing",
            "Python Agent response did not include task state.",
        )
    })
}

fn validation_request(state: &Value) -> Option<ValidationRequestView> {
    if state
        .get("phase")
        .and_then(Value::as_str)
        .is_some_and(|phase| {
            matches!(
                phase,
                "completed" | "failed" | "needs_intervention" | "waiting_approval"
            )
        })
    {
        return None;
    }

    let request = state.get("validationRequest")?;
    if request.is_null() {
        return None;
    }

    let command = request.get("command")?.as_str()?.trim();
    let cwd = request.get("cwd")?.as_str()?.trim();
    if command.is_empty() || cwd.is_empty() {
        return None;
    }

    Some(ValidationRequestView {
        command: command.to_string(),
        cwd: cwd.to_string(),
    })
}

fn read_log_tail(path: &Path) -> String {
    let result = if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"))
    {
        read_gzip_tail(path)
    } else {
        read_plain_tail(path)
    };

    result.unwrap_or_default()
}

fn read_plain_tail(path: &Path) -> std::io::Result<String> {
    let bytes = fs::read(path)?;
    let start = bytes.len().saturating_sub(VALIDATION_LOG_TAIL_BYTES);
    Ok(String::from_utf8_lossy(&bytes[start..]).to_string())
}

fn read_gzip_tail(path: &Path) -> std::io::Result<String> {
    let file = fs::File::open(path)?;
    let mut decoder = GzDecoder::new(file);
    let mut bytes = Vec::new();
    decoder.read_to_end(&mut bytes)?;
    let start = bytes.len().saturating_sub(VALIDATION_LOG_TAIL_BYTES);
    Ok(String::from_utf8_lossy(&bytes[start..]).to_string())
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn json_error(error: serde_json::Error) -> CommandError {
    CommandError::new(
        "agent.invalidJson",
        format!("Unable to encode Agent API request: {error}"),
    )
}

fn storage_lock_error() -> CommandError {
    CommandError::new(
        "storage.lockUnavailable",
        "Local storage is temporarily unavailable.",
    )
}

fn storage_error(error: StorageError) -> CommandError {
    match error {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        AgentEventRepository, ApprovalRepository, ManagedStorage, NewTask, SqliteStore,
        StorageRoots, TaskRepository,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::Mutex,
    };
    use uuid::Uuid;

    #[test]
    fn new_agent_tasks_resolve_a_model_or_return_a_stable_error() {
        assert_eq!(
            resolve_new_task_model_id(Some(" request-model "), Some("contract-model"))
                .expect("request model wins"),
            "request-model"
        );
        assert_eq!(
            resolve_new_task_model_id(None, Some(" contract-model "))
                .expect("contract model is the fallback"),
            "contract-model"
        );

        let error = resolve_new_task_model_id(Some("   "), None)
            .expect_err("a new task without a model must be rejected");
        assert_eq!(error.code, "agent.modelRequired");
    }

    #[test]
    fn local_path_context_is_redacted_before_audit() {
        let sanitized = allowed_context(r"C:\Users\Example\private-repository");

        assert_eq!(sanitized.content, "[LOCAL_PATH]");
        assert_eq!(sanitized.action, "redacted");
        assert_eq!(sanitized.sensitivity_level, "local_path");
        assert!(sanitized.redacted);
        assert!(!sanitized.blocked);
        assert!(!sanitized.reason.contains("private-repository"));
    }

    #[test]
    fn validation_request_ignores_terminal_or_paused_phases() {
        let state = json!({
            "phase": "needs_intervention",
            "validationRequest": {
                "command": "npm run test",
                "cwd": "D:/repo/.worktrees/task-1"
            }
        });

        assert!(validation_request(&state).is_none());
    }

    #[test]
    fn awaiting_file_commit_maps_to_awaiting_approval_status() {
        assert_eq!(
            task_status_from_agent_phase("awaiting_file_commit"),
            "awaitingApproval"
        );
    }

    #[test]
    fn encode_path_segment_escapes_slashes_and_spaces() {
        assert_eq!(encode_path_segment("task 1/a"), "task%201%2Fa");
    }

    #[test]
    fn agent_file_commit_uses_recoverable_transaction() {
        let task_id = "task-agent-file-commit";
        let (storage, workspace) = test_storage_with_workspace("agent-file-commit", task_id);
        let edits = vec![json!({
            "operation": "create",
            "path": "created.txt",
            "content": "committed by transaction",
            "summary": "Create a file"
        })];

        let first = execute_file_commit_transaction(&storage, task_id, "commit-1", &edits)
            .expect("agent commit uses file transaction");
        let second = execute_file_commit_transaction(&storage, task_id, "commit-1", &edits)
            .expect("agent commit is idempotent");

        assert_eq!(first.transaction_id, second.transaction_id);
        assert_eq!(first.status, "committed");
        assert_eq!(
            fs::read_to_string(workspace.join("created.txt")).unwrap(),
            "committed by transaction"
        );
        let store = storage.store.lock().unwrap();
        let row: (String, String) = store
            .connection()
            .query_row(
                "SELECT request_id, status FROM file_edit_transactions WHERE task_id = ?1",
                [task_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, ("commit-1".to_string(), "committed".to_string()));
    }

    #[test]
    fn agent_file_commit_update_requires_and_consumes_file_authorization() {
        let task_id = "task-agent-file-approval";
        let (storage, workspace) = test_storage_with_workspace("agent-file-approval", task_id);
        fs::write(workspace.join("value.txt"), "before").unwrap();
        let edits = vec![json!({
            "operation": "update",
            "path": "value.txt",
            "content": "after",
            "summary": "Update a file"
        })];

        let approval_error =
            execute_file_commit_transaction(&storage, task_id, "commit-update", &edits)
                .expect_err("existing file update needs approval");

        assert_eq!(approval_error.code, "approval.required");
        assert_eq!(
            fs::read_to_string(workspace.join("value.txt")).unwrap(),
            "before"
        );
        {
            let store = storage.store.lock().unwrap();
            let transaction_count: i64 = store
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM file_edit_transactions WHERE task_id = ?1",
                    [task_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(transaction_count, 0);
            let approval = ApprovalRepository::new(store.connection())
                .list_pending()
                .unwrap()
                .into_iter()
                .find(|approval| approval.task_id == task_id)
                .expect("file approval created");
            assert_eq!(approval.action.as_deref(), Some("file.mutate"));
            ApprovalRepository::new(store.connection())
                .decide(&approval.id, "approved", None)
                .unwrap();
        }

        let committed = execute_file_commit_transaction(&storage, task_id, "commit-update", &edits)
            .expect("approved update commits");

        assert_eq!(committed.status, "committed");
        assert_eq!(
            fs::read_to_string(workspace.join("value.txt")).unwrap(),
            "after"
        );
        let store = storage.store.lock().unwrap();
        let consumed_by: String = store
            .connection()
            .query_row(
                "SELECT consumed_by_call_id FROM approvals WHERE task_id = ?1 AND action = 'file.mutate'",
                [task_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(consumed_by, "commit-update");
    }

    #[test]
    fn sync_agent_state_records_stage_change_event_once() {
        let storage = test_storage("agent-stage-events");
        seed_task(&storage, "task-stage-events");
        let state = json!({
            "phase": "editing",
            "checkpointIndex": 2,
            "todos": [{
                "id": "todo-1",
                "title": "Edit task files",
                "description": "Apply the requested changes",
                "status": "in_progress"
            }]
        });

        sync_agent_state(&storage, "task-stage-events", &state).expect("first sync succeeds");
        sync_agent_state(&storage, "task-stage-events", &state).expect("second sync succeeds");

        let store = storage.store.lock().expect("storage lock");
        let events = AgentEventRepository::new(store.connection())
            .list_for_task("task-stage-events")
            .expect("list task events");
        let stage_events: Vec<_> = events
            .iter()
            .filter(|event| event.event_type == "task.stage.changed")
            .collect();

        assert_eq!(stage_events.len(), 1);
        assert_eq!(stage_events[0].stage, "editing");
        assert!(stage_events[0].message.contains("editing"));
    }

    #[test]
    fn sync_agent_state_keeps_model_audit_canary_out_of_events_and_links_budget() {
        let storage = test_storage("agent-model-audit-events");
        let task_id = "task-model-audit-events";
        let request_id = "request-model-audit-events";
        let sensitive_canary = "rel-p0-006-event-sensitive-canary";
        seed_task(&storage, task_id);
        let state = json!({
            "phase": "editing",
            "checkpointIndex": 2,
            "todos": [],
            "modelRequestAudits": [{
                "requestId": request_id,
                "taskId": task_id,
                "provider": "openai-compatible",
                "modelId": "test-model",
                "phase": "planning",
                "status": "succeeded",
                "requestDigest": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                "inputTokensEstimate": 9,
                "outputTokens": 3,
                "totalTokens": 12,
                "budgetLimit": 120000,
                "budgetPerCall": 24000,
                "blockedReason": sensitive_canary,
                "sources": [{
                    "dataKind": "prompt",
                    "sourceRef": "messages[0].content",
                    "action": "redacted",
                    "sensitivityLevel": "high",
                    "findings": ["secret_assignment"],
                    "redacted": true,
                    "blocked": false,
                    "sizeBytes": 72,
                    "tokensEstimate": 9
                }]
            }]
        });

        sync_agent_state(&storage, task_id, &state).expect("sync succeeds");

        let store = storage.store.lock().expect("storage lock");
        let events = AgentEventRepository::new(store.connection())
            .list_for_task(task_id)
            .expect("list task events");
        let serialized_events = events
            .iter()
            .map(|event| {
                format!(
                    "{}{}{}{}",
                    event.event_type, event.stage, event.message, event.payload
                )
            })
            .collect::<String>();
        assert!(!serialized_events.contains(sensitive_canary));

        let ledger_source: String = store
            .connection()
            .query_row(
                "SELECT source_ref FROM privacy_ledger_entries WHERE task_id = ?1",
                [task_id],
                |row| row.get(0),
            )
            .expect("privacy ledger linkage");
        let budget_run_id: String = store
            .connection()
            .query_row(
                "SELECT run_id FROM token_budget_records WHERE task_id = ?1 AND call_type LIKE 'model_request_%'",
                [task_id],
                |row| row.get(0),
            )
            .expect("token budget linkage");
        assert!(ledger_source.contains(request_id));
        assert_eq!(budget_run_id, request_id);
    }

    #[test]
    fn sync_agent_state_records_repair_start_and_finish_when_round_advances_in_one_sync() {
        let storage = test_storage("agent-repair-events");
        seed_task(&storage, "task-repair-events");
        let before_repair = json!({
            "phase": "validating",
            "checkpointIndex": 2,
            "repairRound": 0,
            "maxRepairRounds": 3,
            "todos": []
        });
        let repaired = json!({
            "phase": "validating",
            "checkpointIndex": 3,
            "repairRound": 1,
            "maxRepairRounds": 3,
            "repairPlan": {
                "summary": "Fix the failing assertion",
                "suspectedCauses": ["Incorrect return value"],
                "nextActions": ["Update src/lib.rs"]
            },
            "validationRequest": {
                "command": "cargo test",
                "cwd": "D:/codemax/.worktrees/task-repair-events"
            },
            "editPlan": {
                "edits": [{
                    "operation": "update",
                    "path": "src/lib.rs",
                    "content": "pub fn fixed() -> bool { true }",
                    "summary": "Fix the assertion"
                }]
            },
            "fileEdits": [{
                "path": "src/lib.rs",
                "operation": "update",
                "summary": "Initial edit"
            }, {
                "path": "src/lib.rs",
                "operation": "update",
                "summary": "Fix the assertion"
            }],
            "repairFileEdits": [{
                "path": "src/lib.rs",
                "operation": "update",
                "summary": "token fictional-repair-summary-secret"
            }],
            "todos": []
        });

        sync_agent_state(&storage, "task-repair-events", &before_repair)
            .expect("initial validation sync succeeds");
        sync_agent_state(&storage, "task-repair-events", &repaired).expect("repair sync succeeds");
        sync_agent_state(&storage, "task-repair-events", &repaired)
            .expect("repeated repair sync succeeds");

        let store = storage.store.lock().expect("storage lock");
        let events = AgentEventRepository::new(store.connection())
            .list_for_task("task-repair-events")
            .expect("list repair events");
        let repair_started: Vec<_> = events
            .iter()
            .filter(|event| event.event_type == "repair.started")
            .collect();
        let repair_finished: Vec<_> = events
            .iter()
            .filter(|event| event.event_type == "repair.finished")
            .collect();

        assert_eq!(repair_started.len(), 1);
        assert_eq!(repair_finished.len(), 1);
        assert!(repair_started[0]
            .payload
            .contains("Fix the failing assertion"));
        assert!(repair_finished[0].payload.contains("cargo test"));
        assert!(repair_started[0].payload.contains("\"file_edits\""));
        assert!(repair_started[0].payload.contains("src/lib.rs"));
        assert!(!repair_started[0].payload.contains("Initial edit"));
        assert!(!repair_started[0]
            .payload
            .contains("fictional-repair-summary-secret"));
        assert!(repair_started[0].payload.contains("token [REDACTED]"));
        assert!(repair_finished[0].payload.contains("\"file_edits\""));
        assert!(repair_finished[0].payload.contains("src/lib.rs"));
        assert!(!repair_finished[0].payload.contains("Initial edit"));
        assert!(!repair_finished[0]
            .payload
            .contains("fictional-repair-summary-secret"));
        assert!(repair_finished[0].payload.contains("token [REDACTED]"));
    }

    #[test]
    fn sync_agent_state_records_repair_failure_when_model_plan_generation_fails() {
        let storage = test_storage("agent-repair-failed-events");
        seed_task(&storage, "task-repair-failed-events");
        let before_repair = json!({
            "phase": "validating",
            "checkpointIndex": 2,
            "repairRound": 0,
            "maxRepairRounds": 3,
            "todos": []
        });
        let failed_repair = json!({
            "phase": "failed",
            "checkpointIndex": 3,
            "repairRound": 1,
            "maxRepairRounds": 3,
            "repairPlan": {
                "summary": "Validation failed; repair generation was attempted",
                "suspectedCauses": ["Invalid model response"],
                "nextActions": ["Review model configuration"]
            },
            "repairFileEdits": [],
            "todos": []
        });

        sync_agent_state(&storage, "task-repair-failed-events", &before_repair)
            .expect("initial sync succeeds");
        sync_agent_state(&storage, "task-repair-failed-events", &failed_repair)
            .expect("failed repair sync succeeds");
        sync_agent_state(&storage, "task-repair-failed-events", &failed_repair)
            .expect("repeated failed repair sync succeeds");

        let store = storage.store.lock().expect("storage lock");
        let events = AgentEventRepository::new(store.connection())
            .list_for_task("task-repair-failed-events")
            .expect("list repair failure events");
        let repair_started: Vec<_> = events
            .iter()
            .filter(|event| event.event_type == "repair.started")
            .collect();
        let repair_failed: Vec<_> = events
            .iter()
            .filter(|event| event.event_type == "repair.failed")
            .collect();

        assert_eq!(repair_started.len(), 1);
        assert_eq!(repair_failed.len(), 1);
        assert_eq!(repair_failed[0].stage, "failed");
        assert!(repair_failed[0].payload.contains("repair_round"));
        assert!(repair_failed[0].payload.contains("repair_plan"));
    }

    fn test_storage(label: &str) -> ManagedStorage {
        let root = temp_path(label);
        let roots = StorageRoots::from_app_data_dir(&root);
        roots.ensure_base_dirs().expect("create storage roots");
        let store = SqliteStore::open_in_memory().expect("open sqlite");
        store.migrate().expect("run migrations");

        ManagedStorage {
            roots,
            store: Mutex::new(store),
        }
    }

    fn seed_task(storage: &ManagedStorage, task_id: &str) {
        seed_task_with_worktree(
            storage,
            task_id,
            Path::new("D:/codemax/.worktrees/task-stage-events"),
        );
    }

    fn seed_task_with_worktree(storage: &ManagedStorage, task_id: &str, workspace: &Path) {
        let workspace_text = workspace.to_string_lossy();
        let store = storage.store.lock().expect("storage lock");
        TaskRepository::new(store.connection())
            .create(NewTask {
                id: task_id,
                title: "Agent state sync fixture",
                description: "Fixture task for Agent stage sync tests.",
                task_type: "custom",
                status: "queued",
                repository_path: workspace_text.as_ref(),
                worktree_path: Some(workspace_text.as_ref()),
                branch_name: Some("codemax/task-stage-events"),
                target_branch: "main",
                workspace_kind: "git_worktree",
                source_path: workspace_text.as_ref(),
                original_write_authorized: false,
                workspace_estimated_bytes: 0,
                model_id: None,
            })
            .expect("create fixture task");
    }

    fn test_storage_with_workspace(label: &str, task_id: &str) -> (ManagedStorage, PathBuf) {
        let storage = test_storage(label);
        let workspace = temp_path(&format!("{label}-workspace"));
        fs::create_dir_all(&workspace).expect("create workspace");
        seed_task_with_worktree(&storage, task_id, &workspace);
        (storage, workspace)
    }

    fn temp_path(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("codemax-agent-{label}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }
}
