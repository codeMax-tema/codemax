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
    storage::ManagedStorage,
};

use super::exec::run_task_command;

const VALIDATION_LOG_TAIL_BYTES: usize = 64 * 1024;
const DEFAULT_VALIDATION_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_VALIDATION_LOOP_LIMIT: usize = 12;

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
    request: AgentTaskCreateRequest,
) -> AppResult<Value> {
    let body = serde_json::to_value(request).map_err(json_error)?;
    agent
        .api_json("POST", "/api/v1/tasks", Some(body))
        .await
        .map_err(agent_error)
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
    task_id: String,
    request: AgentValidationResultRequest,
) -> AppResult<Value> {
    let path = format!(
        "/api/v1/tasks/{}/validation-result",
        encode_path_segment(&task_id)
    );
    let body = serde_json::to_value(request).map_err(json_error)?;
    agent
        .api_json("POST", &path, Some(body))
        .await
        .map_err(agent_error)
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
        let result_body = json!({
            "runId": command_result.run_id,
            "command": command_result.command,
            "cwd": command_result.cwd,
            "stdout": stdout,
            "stderr": stderr,
            "exitCode": command_result.exit_code,
            "timedOut": command_result.timed_out,
            "cancelled": command_result.cancelled,
        });
        command_results.push(command_result);

        response = agent
            .api_json(
                "POST",
                &format!("/api/v1/tasks/{task_path}/validation-result"),
                Some(result_body),
            )
            .await
            .map_err(agent_error)?;
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
