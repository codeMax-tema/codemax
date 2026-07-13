use std::{
    collections::BTreeMap,
    env,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::{process::Child, process::Command, time::sleep};

const DEFAULT_AGENT_HOST: &str = "127.0.0.1";
const DEFAULT_AGENT_PORT: u16 = 8765;
const DEFAULT_STARTUP_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_HEALTH_TIMEOUT_MS: u64 = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentServiceConfig {
    pub host: String,
    pub port: u16,
    pub python_executable: PathBuf,
    pub agent_dir: PathBuf,
    pub startup_timeout_ms: u64,
    pub health_timeout_ms: u64,
    pub runtime_env: BTreeMap<String, String>,
    pub standalone: bool,
}

impl Default for AgentServiceConfig {
    fn default() -> Self {
        let agent_dir = default_agent_dir();

        Self {
            host: env::var("CODEMAX_AGENT_HOST").unwrap_or_else(|_| DEFAULT_AGENT_HOST.to_string()),
            port: parse_env_u16("CODEMAX_AGENT_PORT", DEFAULT_AGENT_PORT),
            python_executable: discover_python_executable(&agent_dir),
            agent_dir,
            startup_timeout_ms: parse_env_u64(
                "CODEMAX_AGENT_STARTUP_TIMEOUT_MS",
                DEFAULT_STARTUP_TIMEOUT_MS,
            ),
            health_timeout_ms: parse_env_u64(
                "CODEMAX_AGENT_HEALTH_TIMEOUT_MS",
                DEFAULT_HEALTH_TIMEOUT_MS,
            ),
            runtime_env: BTreeMap::new(),
            standalone: false,
        }
    }
}

impl AgentServiceConfig {
    pub fn with_resource_dir(mut self, resource_dir: impl AsRef<Path>) -> Self {
        let executable = resource_dir.as_ref().join("agent").join(if cfg!(windows) {
            "codemax-agent.exe"
        } else {
            "codemax-agent"
        });
        if executable.is_file() {
            self.agent_dir = executable
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| resource_dir.as_ref().to_path_buf());
            self.python_executable = executable;
            self.standalone = true;
        }
        self
    }

    pub fn with_app_data_defaults(mut self, app_data_dir: impl AsRef<Path>) -> Self {
        let app_data_dir = app_data_dir.as_ref();
        self.set_env_default("CODEMAX_APP_DATA_DIR", app_data_dir);
        self.set_env_default(
            "CODEMAX_AGENT_CHECKPOINT_DIR",
            app_data_dir.join("agent").join("checkpoints"),
        );
        self.set_env_default(
            "CODEMAX_AGENT_MEMORY_DIR",
            app_data_dir.join("agent").join("memory"),
        );
        self
    }

    fn health_url(&self) -> String {
        format!("http://{}:{}/health", self.host, self.port)
    }

    fn set_env_default(&mut self, key: &str, path: impl AsRef<Path>) {
        if env::var(key).is_ok_and(|value| !value.trim().is_empty()) {
            return;
        }

        self.runtime_env
            .insert(key.to_string(), path.as_ref().to_string_lossy().to_string());
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentHealthResponse {
    pub service: String,
    pub status: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentServiceStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub host: String,
    pub port: u16,
    pub health_url: String,
    pub agent_dir: String,
    pub python_executable: String,
    pub health: Option<AgentHealthResponse>,
}

#[derive(Debug, Error)]
pub enum AgentServiceError {
    #[error("agent service lock is unavailable")]
    LockUnavailable,
    #[error("agent service directory does not exist: {0}")]
    AgentDirMissing(PathBuf),
    #[error("failed to spawn Python agent with {python}: {source}")]
    Spawn {
        python: String,
        source: std::io::Error,
    },
    #[error("agent process exited before it became healthy: {status}")]
    ProcessExited { status: String },
    #[error("agent service did not become healthy within {timeout_ms} ms")]
    StartupTimeout { timeout_ms: u64 },
    #[error("agent health check failed: {0}")]
    Health(String),
    #[error("agent health response was invalid: {0}")]
    InvalidHealthResponse(String),
    #[error("agent API request failed: {0}")]
    Api(String),
    #[error("agent API response was invalid: {0}")]
    InvalidApiResponse(String),
    #[error("agent process operation failed: {0}")]
    Process(std::io::Error),
    #[error("agent health check task failed: {0}")]
    Join(String),
}

pub struct AgentService {
    config: Mutex<AgentServiceConfig>,
    child: Mutex<Option<Child>>,
}

impl Default for AgentService {
    fn default() -> Self {
        Self {
            config: Mutex::new(AgentServiceConfig::default()),
            child: Mutex::new(None),
        }
    }
}

impl AgentService {
    pub fn with_runtime_paths(
        app_data_dir: impl AsRef<Path>,
        resource_dir: impl AsRef<Path>,
    ) -> Self {
        Self {
            config: Mutex::new(
                AgentServiceConfig::default()
                    .with_resource_dir(resource_dir)
                    .with_app_data_defaults(app_data_dir),
            ),
            child: Mutex::new(None),
        }
    }

    pub fn set_runtime_env(
        &self,
        values: BTreeMap<String, String>,
    ) -> Result<bool, AgentServiceError> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| AgentServiceError::LockUnavailable)?;
        let previous = config.runtime_env.clone();
        for key in [
            "CODEMAX_MODEL_PROVIDER",
            "CODEMAX_MODEL_BASE_URL",
            "CODEMAX_MODEL_NAME",
            "CODEMAX_MODEL_API_KEY",
        ] {
            config.runtime_env.remove(key);
        }
        config.runtime_env.extend(values);
        Ok(config.runtime_env != previous)
    }

    pub async fn start(&self) -> Result<AgentServiceStatus, AgentServiceError> {
        if let Ok(health) = self.health_check().await {
            if health.status == "ok" {
                return self.status_with_health(Some(health));
            }
        }

        let config = self
            .config
            .lock()
            .map_err(|_| AgentServiceError::LockUnavailable)?
            .clone();
        if !config.agent_dir.is_dir() {
            return Err(AgentServiceError::AgentDirMissing(config.agent_dir.clone()));
        }

        if let Some(mut child) = self.take_child()? {
            terminate_child(&mut child).await?;
        }

        let mut command = Command::new(&config.python_executable);
        if !config.standalone {
            command.arg("-m").arg("app.main");
        }
        command
            .current_dir(&config.agent_dir)
            .env("CODEMAX_AGENT_HOST", &config.host)
            .env("CODEMAX_AGENT_PORT", config.port.to_string())
            .envs(&config.runtime_env)
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = command.spawn().map_err(|source| AgentServiceError::Spawn {
            python: config.python_executable.to_string_lossy().to_string(),
            source,
        })?;
        self.replace_child(Some(child))?;

        match self.wait_until_healthy().await {
            Ok(health) => self.status_with_health(Some(health)),
            Err(error) => {
                if let Some(mut child) = self.take_child()? {
                    let _ = terminate_child(&mut child).await;
                }
                Err(error)
            }
        }
    }

    pub async fn stop(&self) -> Result<AgentServiceStatus, AgentServiceError> {
        if let Some(mut child) = self.take_child()? {
            terminate_child(&mut child).await?;
        }

        self.status().await
    }

    pub async fn status(&self) -> Result<AgentServiceStatus, AgentServiceError> {
        let health = self.health_check().await.ok();
        self.status_with_health(health)
    }

    pub async fn health_check(&self) -> Result<AgentHealthResponse, AgentServiceError> {
        let config = self
            .config
            .lock()
            .map_err(|_| AgentServiceError::LockUnavailable)?
            .clone();
        tokio::task::spawn_blocking(move || health_probe(&config))
            .await
            .map_err(|error| AgentServiceError::Join(error.to_string()))?
    }

    pub async fn api_json(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, AgentServiceError> {
        self.start().await?;

        let config = self
            .config
            .lock()
            .map_err(|_| AgentServiceError::LockUnavailable)?
            .clone();
        let method = method.to_string();
        let path = path.to_string();
        tokio::task::spawn_blocking(move || api_json_request(&config, &method, &path, body))
            .await
            .map_err(|error| AgentServiceError::Join(error.to_string()))?
    }

    fn status_with_health(
        &self,
        health: Option<AgentHealthResponse>,
    ) -> Result<AgentServiceStatus, AgentServiceError> {
        let config = self
            .config
            .lock()
            .map_err(|_| AgentServiceError::LockUnavailable)?
            .clone();
        let pid = self.current_pid()?;
        let running = health
            .as_ref()
            .is_some_and(|health| health.status == "ok" && health.service == "codemax-agent");

        Ok(AgentServiceStatus {
            running,
            pid,
            host: config.host.clone(),
            port: config.port,
            health_url: config.health_url(),
            agent_dir: config.agent_dir.to_string_lossy().to_string(),
            python_executable: config.python_executable.to_string_lossy().to_string(),
            health,
        })
    }

    fn current_pid(&self) -> Result<Option<u32>, AgentServiceError> {
        let mut child = self
            .child
            .lock()
            .map_err(|_| AgentServiceError::LockUnavailable)?;

        if let Some(process) = child.as_mut() {
            if process
                .try_wait()
                .map_err(AgentServiceError::Process)?
                .is_some()
            {
                *child = None;
                return Ok(None);
            }

            return Ok(process.id());
        }

        Ok(None)
    }

    fn child_exit_status(&self) -> Result<Option<String>, AgentServiceError> {
        let mut child = self
            .child
            .lock()
            .map_err(|_| AgentServiceError::LockUnavailable)?;

        if let Some(process) = child.as_mut() {
            if let Some(status) = process.try_wait().map_err(AgentServiceError::Process)? {
                *child = None;
                return Ok(Some(status.to_string()));
            }
        }

        Ok(None)
    }

    fn replace_child(&self, child: Option<Child>) -> Result<(), AgentServiceError> {
        let mut current = self
            .child
            .lock()
            .map_err(|_| AgentServiceError::LockUnavailable)?;
        *current = child;
        Ok(())
    }

    fn take_child(&self) -> Result<Option<Child>, AgentServiceError> {
        self.child
            .lock()
            .map_err(|_| AgentServiceError::LockUnavailable)
            .map(|mut child| child.take())
    }

    async fn wait_until_healthy(&self) -> Result<AgentHealthResponse, AgentServiceError> {
        let config = self
            .config
            .lock()
            .map_err(|_| AgentServiceError::LockUnavailable)?
            .clone();
        let deadline = Instant::now() + Duration::from_millis(config.startup_timeout_ms.max(1));

        loop {
            match self.health_check().await {
                Ok(health) if health.status == "ok" => return Ok(health),
                Ok(_) => {}
                Err(_) => {}
            }

            if let Some(status) = self.child_exit_status()? {
                return Err(AgentServiceError::ProcessExited { status });
            }

            if Instant::now() >= deadline {
                return Err(AgentServiceError::StartupTimeout {
                    timeout_ms: config.startup_timeout_ms,
                });
            }

            sleep(Duration::from_millis(100)).await;
        }
    }
}

fn health_probe(config: &AgentServiceConfig) -> Result<AgentHealthResponse, AgentServiceError> {
    let timeout = Duration::from_millis(config.health_timeout_ms.max(1));
    let address = (config.host.as_str(), config.port)
        .to_socket_addrs()
        .map_err(|error| AgentServiceError::Health(error.to_string()))?
        .next()
        .ok_or_else(|| AgentServiceError::Health("agent host did not resolve".to_string()))?;
    let mut stream = TcpStream::connect_timeout(&address, timeout).map_err(|error| {
        AgentServiceError::Health(format!(
            "unable to connect to {}: {error}",
            config.health_url()
        ))
    })?;

    stream
        .set_read_timeout(Some(timeout))
        .map_err(AgentServiceError::Process)?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(AgentServiceError::Process)?;

    let request = format!(
        "GET /health HTTP/1.1\r\nHost: {}:{}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
        config.host, config.port
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| AgentServiceError::Health(error.to_string()))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| AgentServiceError::Health(error.to_string()))?;

    let body = response_body(&response)?;
    serde_json::from_str(body.trim())
        .map_err(|error| AgentServiceError::InvalidHealthResponse(error.to_string()))
}

fn api_json_request(
    config: &AgentServiceConfig,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value, AgentServiceError> {
    let method = method.to_ascii_uppercase();
    if !matches!(method.as_str(), "GET" | "POST") {
        return Err(AgentServiceError::Api(format!(
            "unsupported agent API method: {method}"
        )));
    }

    let timeout = Duration::from_millis(config.health_timeout_ms.max(1) * 20);
    let address = (config.host.as_str(), config.port)
        .to_socket_addrs()
        .map_err(|error| AgentServiceError::Api(error.to_string()))?
        .next()
        .ok_or_else(|| AgentServiceError::Api("agent host did not resolve".to_string()))?;
    let mut stream = TcpStream::connect_timeout(&address, timeout).map_err(|error| {
        AgentServiceError::Api(format!(
            "unable to connect to {}: {error}",
            config.health_url()
        ))
    })?;

    stream
        .set_read_timeout(Some(timeout))
        .map_err(AgentServiceError::Process)?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(AgentServiceError::Process)?;

    let body = body
        .map(|value| value.to_string())
        .unwrap_or_else(|| "{}".to_string());
    let request = if method == "GET" {
        format!(
            "GET {path} HTTP/1.1\r\nHost: {}:{}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
            config.host, config.port
        )
    } else {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {}:{}\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            config.host,
            config.port,
            body.as_bytes().len()
        )
    };

    stream
        .write_all(request.as_bytes())
        .map_err(|error| AgentServiceError::Api(error.to_string()))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| AgentServiceError::Api(error.to_string()))?;

    let (status_line, body) = split_http_response(&response)?;
    let status = http_status_code(status_line).ok_or_else(|| {
        AgentServiceError::InvalidApiResponse(format!("invalid HTTP status: {status_line}"))
    })?;

    if !(200..300).contains(&status) {
        return Err(AgentServiceError::Api(format!(
            "{status_line}: {}",
            body.trim().chars().take(500).collect::<String>()
        )));
    }

    serde_json::from_str(body.trim())
        .map_err(|error| AgentServiceError::InvalidApiResponse(error.to_string()))
}

fn response_body(response: &str) -> Result<&str, AgentServiceError> {
    let (status_line, body) = split_http_response(response)?;

    if !status_line.contains(" 200 ") {
        return Err(AgentServiceError::Health(format!(
            "unexpected health status: {status_line}"
        )));
    }

    Ok(body)
}

fn split_http_response(response: &str) -> Result<(&str, &str), AgentServiceError> {
    let (headers, body) = response.split_once("\r\n\r\n").ok_or_else(|| {
        AgentServiceError::InvalidHealthResponse("missing HTTP response body".to_string())
    })?;
    let status_line = headers.lines().next().unwrap_or_default();
    Ok((status_line, body))
}

fn http_status_code(status_line: &str) -> Option<u16> {
    status_line.split_whitespace().nth(1)?.parse().ok()
}

async fn terminate_child(child: &mut Child) -> Result<(), AgentServiceError> {
    if child
        .try_wait()
        .map_err(AgentServiceError::Process)?
        .is_some()
    {
        return Ok(());
    }

    child.start_kill().map_err(AgentServiceError::Process)?;
    let _ = child.wait().await.map_err(AgentServiceError::Process)?;
    Ok(())
}

fn discover_python_executable(agent_dir: &std::path::Path) -> PathBuf {
    if let Ok(value) = env::var("CODEMAX_AGENT_PYTHON") {
        if !value.trim().is_empty() {
            return PathBuf::from(value);
        }
    }

    for candidate in [
        agent_dir.join(".venv").join("Scripts").join("python.exe"),
        agent_dir.join(".venv").join("bin").join("python"),
    ] {
        if candidate.is_file() {
            return candidate;
        }
    }

    PathBuf::from("python")
}

fn default_agent_dir() -> PathBuf {
    let candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("agent");

    candidate.canonicalize().unwrap_or(candidate)
}

fn parse_env_u16(key: &str, fallback: u16) -> u16 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(fallback)
}

fn parse_env_u64(key: &str, fallback: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_body_accepts_successful_health_response() {
        let body = response_body(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"status\":\"ok\"}",
        )
        .expect("body should parse");

        assert_eq!(body, "{\"status\":\"ok\"}");
    }

    #[test]
    fn default_agent_dir_points_at_workspace_agent_folder() {
        let path = default_agent_dir();

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("agent")
        );
    }

    #[test]
    fn app_data_defaults_fill_agent_storage_paths() {
        let root = PathBuf::from("D:/codemax/app-data");
        let config = AgentServiceConfig::default().with_app_data_defaults(&root);

        assert_eq!(
            config
                .runtime_env
                .get("CODEMAX_AGENT_CHECKPOINT_DIR")
                .map(PathBuf::from),
            Some(root.join("agent").join("checkpoints"))
        );
        assert_eq!(
            config
                .runtime_env
                .get("CODEMAX_AGENT_MEMORY_DIR")
                .map(PathBuf::from),
            Some(root.join("agent").join("memory"))
        );
    }

    #[test]
    fn runtime_model_env_replaces_stale_secret_without_exposing_status() {
        let service = AgentService::default();
        let old_secret = "sk-old-runtime-secret";
        let new_secret = "sk-new-runtime-secret";

        let changed = service
            .set_runtime_env(BTreeMap::from([
                (
                    "CODEMAX_MODEL_PROVIDER".to_string(),
                    "openai-compatible".to_string(),
                ),
                (
                    "CODEMAX_MODEL_BASE_URL".to_string(),
                    "https://old.example/v1".to_string(),
                ),
                ("CODEMAX_MODEL_NAME".to_string(), "old-model".to_string()),
                ("CODEMAX_MODEL_API_KEY".to_string(), old_secret.to_string()),
            ]))
            .expect("set initial runtime env");
        assert!(changed);

        let changed = service
            .set_runtime_env(BTreeMap::from([
                (
                    "CODEMAX_MODEL_PROVIDER".to_string(),
                    "openai-compatible".to_string(),
                ),
                (
                    "CODEMAX_MODEL_BASE_URL".to_string(),
                    "https://new.example/v1".to_string(),
                ),
                ("CODEMAX_MODEL_NAME".to_string(), "new-model".to_string()),
                ("CODEMAX_MODEL_API_KEY".to_string(), new_secret.to_string()),
            ]))
            .expect("replace runtime env");
        assert!(changed);

        let config = service.config.lock().expect("lock config").clone();
        assert_eq!(
            config
                .runtime_env
                .get("CODEMAX_MODEL_API_KEY")
                .map(String::as_str),
            Some(new_secret)
        );
        assert!(!config.runtime_env.values().any(|value| value == old_secret));

        let status = service.status_with_health(None).expect("status");
        let serialized = serde_json::to_string(&status).expect("serialize status");
        assert!(!serialized.contains(old_secret));
        assert!(!serialized.contains(new_secret));
    }

    #[test]
    fn packaged_agent_executable_is_preferred_when_present() {
        let root =
            std::env::temp_dir().join(format!("codemax-agent-resource-{}", uuid::Uuid::new_v4()));
        let agent_root = root.join("agent");
        std::fs::create_dir_all(&agent_root).expect("create resource agent directory");
        let executable = agent_root.join(if cfg!(windows) {
            "codemax-agent.exe"
        } else {
            "codemax-agent"
        });
        std::fs::write(&executable, b"fixture").expect("create packaged agent fixture");

        let config = AgentServiceConfig::default().with_resource_dir(&root);

        assert!(config.standalone);
        assert_eq!(config.python_executable, executable);
        assert_eq!(config.agent_dir, agent_root);
        std::fs::remove_dir_all(root).expect("cleanup resource fixture");
    }
}
