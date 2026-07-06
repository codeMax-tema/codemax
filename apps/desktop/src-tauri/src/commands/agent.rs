use tauri::State;

use crate::{
    agent::{AgentHealthResponse, AgentService, AgentServiceError, AgentServiceStatus},
    core::error::{AppResult, CommandError},
};

#[tauri::command]
pub async fn start_agent_service(
    agent: State<'_, AgentService>,
) -> AppResult<AgentServiceStatus> {
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
pub async fn check_agent_health(
    agent: State<'_, AgentService>,
) -> AppResult<AgentHealthResponse> {
    agent.health_check().await.map_err(agent_error)
}

fn agent_error(error: AgentServiceError) -> CommandError {
    match error {
        AgentServiceError::LockUnavailable => CommandError::new(
            "agent.lockUnavailable",
            "Python Agent service state is temporarily unavailable.",
        ),
        AgentServiceError::AgentDirMissing(path) => CommandError::new(
            "agent.directoryMissing",
            format!("Python Agent service directory does not exist: {}", path.to_string_lossy()),
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
