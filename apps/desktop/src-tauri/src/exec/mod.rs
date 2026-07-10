use std::{
    collections::{BTreeMap, HashMap},
    fs,
    future::pending,
    io,
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use flate2::{write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::oneshot,
};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    pub task_id: String,
    pub run_id: Option<String>,
    pub command: String,
    pub cwd: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub purpose: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLogPaths {
    pub stdout_path: PathBuf,
    pub stderr_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandOutputStream {
    Stdout,
    Stderr,
}

impl CommandOutputStream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutputEvent {
    pub task_id: String,
    pub run_id: String,
    pub stream: CommandOutputStream,
    pub chunk: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandFinishedEvent {
    pub result: CommandExecutionResult,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandExecutionResult {
    pub run_id: String,
    pub task_id: String,
    pub command: String,
    pub cwd: String,
    pub status: String,
    pub stdout_path: String,
    pub stderr_path: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub timed_out: bool,
    pub cancelled: bool,
    pub purpose: String,
}

#[derive(Debug, Error)]
pub enum CommandExecutionError {
    #[error("command is empty")]
    EmptyCommand,
    #[error("command run {0} is already running")]
    DuplicateRun(String),
    #[error("command run registry is unavailable")]
    RegistryUnavailable,
    #[error("unsupported command purpose: {0}")]
    InvalidPurpose(String),
    #[error("failed to create log file: {0}")]
    LogFile(#[from] std::io::Error),
    #[error("failed to spawn command: {0}")]
    Spawn(String),
    #[error("command output task failed: {0}")]
    Join(String),
}

pub type CommandExecutionResultType<T> = Result<T, CommandExecutionError>;
pub type CommandOutputSink = Arc<dyn Fn(CommandOutputEvent) + Send + Sync>;
pub const LOG_COMPRESSION_THRESHOLD_BYTES: u64 = 1_048_576;

#[derive(Clone, Default)]
pub struct CommandRunRegistry {
    commands: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
}

impl CommandRunRegistry {
    pub fn register(&self, run_id: &str) -> CommandExecutionResultType<oneshot::Receiver<()>> {
        let (sender, receiver) = oneshot::channel();
        let mut commands = self
            .commands
            .lock()
            .map_err(|_| CommandExecutionError::RegistryUnavailable)?;

        if commands.contains_key(run_id) {
            return Err(CommandExecutionError::DuplicateRun(run_id.to_string()));
        }

        commands.insert(run_id.to_string(), sender);
        Ok(receiver)
    }

    pub fn cancel(&self, run_id: &str) -> CommandExecutionResultType<bool> {
        let mut commands = self
            .commands
            .lock()
            .map_err(|_| CommandExecutionError::RegistryUnavailable)?;

        let Some(sender) = commands.remove(run_id) else {
            return Ok(false);
        };

        Ok(sender.send(()).is_ok())
    }

    pub fn unregister(&self, run_id: &str) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.remove(run_id);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CommandExecutor;

impl CommandExecutor {
    pub async fn run(
        &self,
        request: CommandRequest,
        log_paths: CommandLogPaths,
        registry: CommandRunRegistry,
        output_sink: CommandOutputSink,
        additional_redaction_values: Vec<String>,
    ) -> CommandExecutionResultType<CommandExecutionResult> {
        let purpose = normalize_command_purpose(request.purpose.as_deref(), request.run_id.as_deref())?;
        let request = request.with_run_id();
        let command_text = request.command.trim();
        if command_text.is_empty() {
            return Err(CommandExecutionError::EmptyCommand);
        }

        let run_id = request.run_id.clone().expect("run id is assigned");
        let mut cancel_rx = registry.register(&run_id)?;
        let _run_guard = CommandRunGuard {
            registry: registry.clone(),
            run_id: run_id.clone(),
        };
        let redactor =
            LogRedactor::from_command_env_and_values(&request.env, additional_redaction_values);

        let stdout_file = File::create(&log_paths.stdout_path).await?;
        let stderr_file = File::create(&log_paths.stderr_path).await?;
        let mut command = shell_command(command_text);
        command
            .current_dir(&request.cwd)
            .envs(&request.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = command
            .spawn()
            .map_err(|error| CommandExecutionError::Spawn(error.to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CommandExecutionError::Spawn("stdout pipe unavailable".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CommandExecutionError::Spawn("stderr pipe unavailable".to_string()))?;

        let sequence = Arc::new(AtomicU64::new(1));
        let stdout_task = spawn_output_capture(OutputCapture {
            task_id: request.task_id.clone(),
            run_id: run_id.clone(),
            stream: CommandOutputStream::Stdout,
            reader: stdout,
            file: stdout_file,
            redactor: redactor.clone(),
            sequence: Arc::clone(&sequence),
            sink: Arc::clone(&output_sink),
        });
        let stderr_task = spawn_output_capture(OutputCapture {
            task_id: request.task_id.clone(),
            run_id: run_id.clone(),
            stream: CommandOutputStream::Stderr,
            reader: stderr,
            file: stderr_file,
            redactor: redactor.clone(),
            sequence,
            sink: output_sink,
        });

        let started_at = Instant::now();
        let timeout = request.timeout_ms.map(Duration::from_millis);
        let timeout_future = async {
            match timeout {
                Some(duration) => tokio::time::sleep(duration).await,
                None => pending::<()>().await,
            }
        };
        tokio::pin!(timeout_future);

        let mut timed_out = false;
        let mut cancelled = false;
        let exit_code = tokio::select! {
            wait_result = child.wait() => {
                wait_result.map_err(CommandExecutionError::LogFile)?.code()
            }
            _ = &mut cancel_rx => {
                cancelled = true;
                let _ = child.start_kill();
                let _ = child.wait().await;
                None
            }
            _ = &mut timeout_future => {
                timed_out = true;
                let _ = child.start_kill();
                let _ = child.wait().await;
                None
            }
        };

        let stdout_result = stdout_task
            .await
            .map_err(|error| CommandExecutionError::Join(error.to_string()))?;
        let stderr_result = stderr_task
            .await
            .map_err(|error| CommandExecutionError::Join(error.to_string()))?;
        stdout_result?;
        stderr_result?;

        let stdout_path =
            maybe_compress_log_file(log_paths.stdout_path, LOG_COMPRESSION_THRESHOLD_BYTES);
        let stderr_path =
            maybe_compress_log_file(log_paths.stderr_path, LOG_COMPRESSION_THRESHOLD_BYTES);

        Ok(CommandExecutionResult {
            run_id,
            task_id: request.task_id,
            command: redactor.redact(&request.command),
            cwd: request.cwd,
            status: command_status(exit_code, timed_out, cancelled).to_string(),
            stdout_path: stdout_path.to_string_lossy().to_string(),
            stderr_path: stderr_path.to_string_lossy().to_string(),
            exit_code,
            duration_ms: started_at.elapsed().as_millis() as u64,
            timed_out,
            cancelled,
            purpose,
        })
    }
}

struct CommandRunGuard {
    registry: CommandRunRegistry,
    run_id: String,
}

impl Drop for CommandRunGuard {
    fn drop(&mut self) {
        self.registry.unregister(&self.run_id);
    }
}

impl CommandRequest {
    pub fn with_run_id(mut self) -> Self {
        let run_id = self
            .run_id
            .take()
            .filter(|run_id| !run_id.trim().is_empty())
            .unwrap_or_else(|| format!("cmd-{}", Uuid::new_v4()));
        self.run_id = Some(run_id);
        self
    }
}

#[derive(Clone)]
pub struct LogRedactor {
    secrets: Arc<Vec<String>>,
}

impl LogRedactor {
    pub fn from_command_env(env: &BTreeMap<String, String>) -> Self {
        Self::from_command_env_and_values(env, Vec::new())
    }

    pub fn from_command_env_and_values(
        env: &BTreeMap<String, String>,
        additional_values: Vec<String>,
    ) -> Self {
        let mut secrets = Vec::new();

        for (key, value) in env {
            if is_sensitive_key(key) && value.len() >= 4 {
                secrets.push(value.to_string());
            }
        }

        for (key, value) in std::env::vars() {
            if is_sensitive_key(&key) && value.len() >= 4 {
                secrets.push(value.to_string());
            }
        }

        for value in additional_values {
            if value.len() >= 4 {
                secrets.push(value);
            }
        }

        secrets.sort();
        secrets.dedup();
        secrets.sort_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));

        Self {
            secrets: Arc::new(secrets),
        }
    }

    pub fn from_values(values: Vec<String>) -> Self {
        Self::from_command_env_and_values(&BTreeMap::new(), values)
    }

    pub fn redact(&self, chunk: &str) -> String {
        let mut redacted = chunk.to_string();
        for secret in self.secrets.iter() {
            redacted = redacted.replace(secret, "[REDACTED]");
        }
        redacted
    }
}

struct OutputCapture<R> {
    task_id: String,
    run_id: String,
    stream: CommandOutputStream,
    reader: R,
    file: File,
    redactor: LogRedactor,
    sequence: Arc<AtomicU64>,
    sink: CommandOutputSink,
}

fn spawn_output_capture<R>(
    capture: OutputCapture<R>,
) -> tokio::task::JoinHandle<CommandExecutionResultType<()>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move { capture_output(capture).await })
}

async fn capture_output<R>(mut capture: OutputCapture<R>) -> CommandExecutionResultType<()>
where
    R: AsyncRead + Unpin,
{
    let mut buffer = [0_u8; 8192];

    loop {
        let read = capture.reader.read(&mut buffer).await?;
        if read == 0 {
            capture.file.flush().await?;
            return Ok(());
        }

        let chunk = String::from_utf8_lossy(&buffer[..read]).to_string();
        let chunk = capture.redactor.redact(&chunk);
        capture.file.write_all(chunk.as_bytes()).await?;
        capture.file.flush().await?;

        (capture.sink)(CommandOutputEvent {
            task_id: capture.task_id.clone(),
            run_id: capture.run_id.clone(),
            stream: capture.stream,
            chunk,
            sequence: capture.sequence.fetch_add(1, Ordering::SeqCst),
            timestamp_ms: now_millis(),
        });
    }
}

fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut shell = Command::new("cmd");
        shell.arg("/C").arg(command);
        shell
    }

    #[cfg(not(windows))]
    {
        let mut shell = Command::new("sh");
        shell.arg("-c").arg(command);
        shell
    }
}

fn is_validation_run_id(run_id: &str) -> bool {
    run_id.starts_with("validation-")
}

fn normalize_command_purpose(
    purpose: Option<&str>,
    run_id: Option<&str>,
) -> CommandExecutionResultType<String> {
    let value = purpose
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if run_id.is_some_and(is_validation_run_id) {
                "validation"
            } else {
                "diagnostic"
            }
        });

    match value {
        "validation" | "edit" | "diagnostic" => Ok(value.to_string()),
        other => Err(CommandExecutionError::InvalidPurpose(other.to_string())),
    }
}

fn command_status(exit_code: Option<i32>, timed_out: bool, cancelled: bool) -> &'static str {
    if timed_out {
        "timedOut"
    } else if cancelled {
        "cancelled"
    } else if exit_code == Some(0) {
        "passed"
    } else {
        "failed"
    }
}

fn maybe_compress_log_file(path: PathBuf, threshold_bytes: u64) -> PathBuf {
    match compress_log_file_if_large(&path, threshold_bytes) {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(
                path = %path.to_string_lossy(),
                error = %error,
                "failed to compress command log; keeping original log"
            );
            path
        }
    }
}

fn compress_log_file_if_large(path: &PathBuf, threshold_bytes: u64) -> io::Result<PathBuf> {
    if path.extension().is_some_and(|extension| extension == "gz") {
        return Ok(path.clone());
    }

    let metadata = fs::metadata(path)?;
    if metadata.len() <= threshold_bytes {
        return Ok(path.clone());
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "log path has no file name"))?;
    let compressed_path = path.with_file_name(format!("{file_name}.gz"));
    let temp_path = path.with_file_name(format!("{file_name}.gz.tmp"));

    let mut input = fs::File::open(path)?;
    let output = fs::File::create(&temp_path)?;
    let mut encoder = GzEncoder::new(output, Compression::default());
    io::copy(&mut input, &mut encoder)?;
    encoder.finish()?;

    if compressed_path.exists() {
        fs::remove_file(&compressed_path)?;
    }
    fs::rename(&temp_path, &compressed_path)?;
    fs::remove_file(path)?;

    Ok(compressed_path)
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    normalized.contains("api_key")
        || normalized.contains("apikey")
        || normalized.contains("token")
        || normalized.contains("secret")
        || normalized.contains("password")
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("codemax-exec-{label}-{}", Uuid::new_v4()))
    }

    #[test]
    fn explicit_command_purpose_overrides_legacy_run_id_fallback() {
        assert_eq!(
            normalize_command_purpose(Some("edit"), Some("validation-legacy"))
                .expect("explicit purpose is valid"),
            "edit"
        );
        assert_eq!(
            normalize_command_purpose(None, Some("validation-legacy"))
                .expect("legacy validation purpose is inferred"),
            "validation"
        );
        assert!(matches!(
            normalize_command_purpose(Some("unknown"), None),
            Err(CommandExecutionError::InvalidPurpose(value)) if value == "unknown"
        ));
    }

    fn test_command() -> String {
        if cfg!(windows) {
            "echo out && echo err 1>&2 && exit /b 7".to_string()
        } else {
            "printf 'out\\n'; printf 'err\\n' >&2; exit 7".to_string()
        }
    }

    fn slow_command() -> String {
        if cfg!(windows) {
            "powershell -NoProfile -Command \"Start-Sleep -Seconds 5; Write-Output done\""
                .to_string()
        } else {
            "sleep 5; printf 'done\\n'".to_string()
        }
    }

    fn log_paths(root: &Path) -> CommandLogPaths {
        CommandLogPaths {
            stdout_path: root.join("stdout.log"),
            stderr_path: root.join("stderr.log"),
        }
    }

    #[tokio::test]
    async fn command_executor_captures_stdout_stderr_and_exit_code() {
        let root = temp_path("capture");
        fs::create_dir_all(&root).expect("create temp log directory");
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink_events = Arc::clone(&events);
        let sink: CommandOutputSink = Arc::new(move |event| {
            sink_events.lock().expect("events lock").push(event);
        });

        let result = CommandExecutor
            .run(
                CommandRequest {
                    task_id: "task-001".to_string(),
                    run_id: Some("run-001".to_string()),
                    command: test_command(),
                    cwd: root.to_string_lossy().to_string(),
                    env: BTreeMap::new(),
                    timeout_ms: Some(30_000),
                    purpose: None,
                },
                log_paths(&root),
                CommandRunRegistry::default(),
                sink,
                Vec::new(),
            )
            .await
            .expect("run command");

        assert_eq!(result.status, "failed");
        assert_eq!(result.exit_code, Some(7));
        assert!(fs::read_to_string(root.join("stdout.log"))
            .expect("read stdout")
            .contains("out"));
        assert!(fs::read_to_string(root.join("stderr.log"))
            .expect("read stderr")
            .contains("err"));

        let events = events.lock().expect("events lock");
        assert!(events
            .iter()
            .any(|event| event.stream == CommandOutputStream::Stdout));
        assert!(events
            .iter()
            .any(|event| event.stream == CommandOutputStream::Stderr));

        fs::remove_dir_all(root).expect("clean temp log directory");
    }

    #[tokio::test]
    async fn command_executor_times_out_long_running_commands() {
        let root = temp_path("timeout");
        fs::create_dir_all(&root).expect("create temp log directory");
        let sink: CommandOutputSink = Arc::new(|_| {});

        let result = CommandExecutor
            .run(
                CommandRequest {
                    task_id: "task-001".to_string(),
                    run_id: Some("run-timeout".to_string()),
                    command: slow_command(),
                    cwd: root.to_string_lossy().to_string(),
                    env: BTreeMap::new(),
                    timeout_ms: Some(100),
                    purpose: None,
                },
                log_paths(&root),
                CommandRunRegistry::default(),
                sink,
                Vec::new(),
            )
            .await
            .expect("run timeout command");

        assert_eq!(result.status, "timedOut");
        assert!(result.timed_out);

        fs::remove_dir_all(root).expect("clean temp log directory");
    }

    #[test]
    fn log_redactor_masks_sensitive_environment_values() {
        let mut env = BTreeMap::new();
        env.insert("OPENAI_API_KEY".to_string(), "sk-test-secret".to_string());

        let redactor = LogRedactor::from_command_env(&env);

        assert_eq!(
            redactor.redact("token sk-test-secret was printed"),
            "token [REDACTED] was printed"
        );
    }

    #[test]
    fn log_redactor_masks_additional_secret_values_longest_first() {
        let redactor =
            LogRedactor::from_values(vec!["sk-test".to_string(), "sk-test-secret".to_string()]);

        assert_eq!(
            redactor.redact("token sk-test-secret and sk-test"),
            "token [REDACTED] and [REDACTED]"
        );
    }

    #[test]
    fn compress_log_file_keeps_small_logs_and_gzips_large_logs() {
        let root = temp_path("compression");
        fs::create_dir_all(&root).expect("create temp log directory");
        let small = root.join("small.log");
        let large = root.join("large.log");
        fs::write(&small, "ok").expect("write small log");
        fs::write(&large, "this log should be compressed").expect("write large log");

        assert_eq!(
            compress_log_file_if_large(&small, 10).expect("small log check"),
            small
        );

        let compressed = compress_log_file_if_large(&large, 1).expect("compress large test log");
        assert_eq!(compressed.file_name().unwrap(), "large.log.gz");
        assert!(compressed.exists());
        assert!(!large.exists());

        fs::remove_dir_all(root).expect("clean temp log directory");
    }
}
