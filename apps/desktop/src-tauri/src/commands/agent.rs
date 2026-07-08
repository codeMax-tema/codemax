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
        sanitize_for_model_context, ContextObservation, SanitizedContent, TokenBudgetObservation,
    },
    storage::{
        AgentEventRepository, AgentSessionRepository, ManagedStorage, NewAgentEvent,
        NewAgentSession, NewTodo, RunContractRepository, StorageError, TaskRepository,
        TodoRepository,
    },
};

use super::exec::run_task_command;

const VALIDATION_LOG_TAIL_BYTES: usize = 64 * 1024;
const DEFAULT_VALIDATION_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_VALIDATION_LOOP_LIMIT: usize = 12;
const DEFAULT_AGENT_BUDGET_LIMIT: i64 = 120_000;

#[derive(Debug, Clone)]
struct ContractBudget {
    model_id: Option<String>,
    token_budget_total: i64,
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
pub async fn start_agent_service(agent: State<'_, AgentService>) -> AppResult<AgentServiceStatus> {
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
    let contract = load_contract_budget(storage.inner(), &request.task_id)?;
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
    task_id: String,
) -> AppResult<Value> {
    let path = format!("/api/v1/tasks/{}", encode_path_segment(&task_id));
    agent
        .api_json("GET", &path, None)
        .await
        .map_err(agent_error)
}

#[tauri::command]
pub async fn advance_agent_task(
    agent: State<'_, AgentService>,
    task_id: String,
    request: AgentTaskAdvanceRequest,
) -> AppResult<Value> {
    let path = format!("/api/v1/tasks/{}/advance", encode_path_segment(&task_id));
    let body = serde_json::to_value(request).map_err(json_error)?;
    agent
        .api_json("POST", &path, Some(body))
        .await
        .map_err(agent_error)
}

#[tauri::command]
pub async fn submit_agent_validation_result(
    agent: State<'_, AgentService>,
    storage: State<'_, ManagedStorage>,
    task_id: String,
    mut request: AgentValidationResultRequest,
) -> AppResult<Value> {
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
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();

    TaskRepository::new(connection)
        .update_status(task_id, task_status, None)
        .map_err(storage_error)?;
    AgentSessionRepository::new(connection)
        .upsert(NewAgentSession {
            id: &format!("agent-session-{task_id}"),
            task_id,
            status: phase,
            stage: task_status,
            checkpoint_id: checkpoint_id.as_deref(),
        })
        .map_err(storage_error)?;

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

    if phase == "repairing" {
        record_agent_event_with_connection(
            &AgentEventRepository::new(connection),
            task_id,
            "repair.started",
            "repairing",
            "Agent started an automatic repair round.",
            json!({
                "repair_round": repair_round,
                "repair_plan": state.get("repairPlan").cloned().unwrap_or_else(|| json!({})),
            }),
        )?;
    } else if phase == "validating" && repair_round > 0 {
        record_agent_event_with_connection(
            &AgentEventRepository::new(connection),
            task_id,
            "repair.finished",
            "validating",
            "Agent finished a repair round and requested validation.",
            json!({
                "repair_round": repair_round,
                "validation_request": state.get("validationRequest").cloned().unwrap_or_else(|| json!({})),
            }),
        )?;
    }

    Ok(())
}

fn task_status_from_agent_phase(phase: &str) -> &'static str {
    match phase {
        "created" | "planned" => "planning",
        "editing" => "editing",
        "validating" => "validating",
        "analyzing_error" | "repairing" => "repairing",
        "waiting_approval" => "awaitingApproval",
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

fn load_contract_budget(storage: &ManagedStorage, task_id: &str) -> AppResult<ContractBudget> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let contract = RunContractRepository::new(store.connection())
        .get_for_task(task_id)
        .map_err(storage_error)?;

    Ok(contract
        .map(|contract| ContractBudget {
            model_id: contract.model_id,
            token_budget_total: contract.token_budget_total,
            overflow_policy: contract.budget_overflow_policy,
        })
        .unwrap_or(ContractBudget {
            model_id: None,
            token_budget_total: DEFAULT_AGENT_BUDGET_LIMIT,
            overflow_policy: "pause_for_approval".to_string(),
        }))
}

fn allowed_context(content: &str) -> SanitizedContent {
    let tokens_estimate = estimate_tokens(content);
    SanitizedContent {
        content: content.to_string(),
        action: "allowed".to_string(),
        sensitivity_level: "none".to_string(),
        findings: Vec::new(),
        redacted: false,
        blocked: false,
        reason: "Context source reference allowed.".to_string(),
        original_size_bytes: content.len() as i64,
        tokens_estimate,
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
    if let Some(cwd) = cwd.as_ref() {
        request.cwd = Some(cwd.content.clone());
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
    fn encode_path_segment_escapes_slashes_and_spaces() {
        assert_eq!(encode_path_segment("task 1/a"), "task%201%2Fa");
    }
}
