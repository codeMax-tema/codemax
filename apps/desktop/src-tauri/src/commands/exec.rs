use std::{
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::{
    core::error::{AppResult, CommandError},
    events,
    exec::{
        CommandExecutionError, CommandExecutionResult, CommandExecutor, CommandLogPaths,
        CommandOutputSink, CommandOutputStream, CommandRequest, CommandRunRegistry, LogRedactor,
    },
    safety::{self, RiskAssessment},
    storage::{
        ApprovalRecord, ApprovalRepository, CommandRunRecord, CommandRunRepository, ManagedStorage,
        NewApproval, NewCommandRun, StorageError, StoragePolicyRepository, TaskRecord,
        TaskRepository,
    },
};

use super::models::load_model_api_key_values;

const DEFAULT_LOG_PAGE_BYTES: u64 = 64 * 1024;
const MAX_LOG_PAGE_BYTES: u64 = 256 * 1024;
const ERROR_SUMMARY_TAIL_BYTES: usize = 64 * 1024;
const DEFAULT_ERROR_SUMMARY_LINES: usize = 20;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandCancelResult {
    pub run_id: String,
    pub cancelled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadCommandLogRequest {
    pub task_id: String,
    pub run_id: String,
    pub stream: CommandOutputStream,
    pub offset_bytes: Option<u64>,
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandLogPage {
    pub task_id: String,
    pub run_id: String,
    pub stream: CommandOutputStream,
    pub offset_bytes: u64,
    pub next_offset_bytes: u64,
    pub content: String,
    pub eof: bool,
    pub compressed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandLogSummaryRequest {
    pub task_id: String,
    pub run_id: String,
    pub max_lines: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandLogSummary {
    pub task_id: String,
    pub run_id: String,
    pub source_stream: CommandOutputStream,
    pub lines: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogCleanupResult {
    pub retention_days: i64,
    pub scanned_files: usize,
    pub deleted_files: usize,
    pub deleted_bytes: u64,
    pub cleanup_disabled: bool,
}

#[tauri::command]
pub async fn execute_task_command(
    app: AppHandle,
    storage: State<'_, ManagedStorage>,
    registry: State<'_, CommandRunRegistry>,
    request: CommandRequest,
) -> AppResult<CommandExecutionResult> {
    run_task_command(&app, storage.inner(), registry.inner(), request).await
}

pub(crate) async fn run_task_command(
    app: &AppHandle,
    storage: &ManagedStorage,
    registry: &CommandRunRegistry,
    request: CommandRequest,
) -> AppResult<CommandExecutionResult> {
    let task = load_task(storage, &request.task_id)?;
    let path_guard = validate_command_cwd(&task, &request.cwd)?;
    let additional_redaction_values = load_model_api_key_values(storage);
    let redactor =
        LogRedactor::from_command_env_and_values(&request.env, additional_redaction_values.clone());
    enforce_command_safety(storage, &task, &request, &path_guard, &redactor)?;

    let request = request.with_run_id();
    let run_id = request
        .run_id
        .as_deref()
        .expect("run id is assigned")
        .to_string();
    let log_paths = command_log_paths(storage, &request.task_id, &run_id)?;
    let sink_app = app.clone();
    let output_sink: CommandOutputSink = std::sync::Arc::new(move |event| {
        let _ = events::emit_command_output(&sink_app, event);
    });

    let result = CommandExecutor
        .run(
            request,
            log_paths,
            registry.clone(),
            output_sink,
            additional_redaction_values,
        )
        .await
        .map_err(command_execution_error)?;

    record_command_result(storage, &result)?;
    events::emit_command_finished(app, result.clone()).map_err(event_error)?;

    Ok(result)
}

#[tauri::command]
pub fn cancel_task_command(
    registry: State<'_, CommandRunRegistry>,
    run_id: String,
) -> AppResult<CommandCancelResult> {
    if run_id.trim().is_empty() {
        return Err(CommandError::new(
            "command.invalidRunId",
            "Command run id is required for cancellation.",
        ));
    }

    let cancelled = registry.cancel(&run_id).map_err(command_execution_error)?;

    Ok(CommandCancelResult { run_id, cancelled })
}

#[tauri::command]
pub fn read_task_command_log(
    storage: State<'_, ManagedStorage>,
    request: ReadCommandLogRequest,
) -> AppResult<CommandLogPage> {
    let run = load_command_run(&storage, &request.task_id, &request.run_id)?;
    let path = resolve_command_log_path(&storage, &run, request.stream)?;
    let offset = request.offset_bytes.unwrap_or(0);
    let max_bytes = clamp_log_page_size(request.max_bytes);
    let compressed = is_gzip_path(&path);
    let (bytes, eof) =
        read_log_window(&path, offset, max_bytes as usize).map_err(log_read_error)?;
    let redactor = stored_secret_redactor(storage.inner());
    let content = redactor.redact(&String::from_utf8_lossy(&bytes));

    Ok(CommandLogPage {
        task_id: request.task_id,
        run_id: request.run_id,
        stream: request.stream,
        offset_bytes: offset,
        next_offset_bytes: offset + bytes.len() as u64,
        content,
        eof,
        compressed,
    })
}

#[tauri::command]
pub fn summarize_task_command_log(
    storage: State<'_, ManagedStorage>,
    request: CommandLogSummaryRequest,
) -> AppResult<CommandLogSummary> {
    let run = load_command_run(&storage, &request.task_id, &request.run_id)?;
    let max_lines = request
        .max_lines
        .unwrap_or(DEFAULT_ERROR_SUMMARY_LINES)
        .clamp(1, 100);
    let stderr_path = resolve_command_log_path(&storage, &run, CommandOutputStream::Stderr)?;
    let stdout_path = resolve_command_log_path(&storage, &run, CommandOutputStream::Stdout)?;
    let (stderr_tail, stderr_truncated) =
        read_log_tail(&stderr_path, ERROR_SUMMARY_TAIL_BYTES).map_err(log_read_error)?;
    let (stdout_tail, stdout_truncated) =
        read_log_tail(&stdout_path, ERROR_SUMMARY_TAIL_BYTES).map_err(log_read_error)?;
    let redactor = stored_secret_redactor(storage.inner());
    let stderr_tail = redactor.redact(&stderr_tail);
    let stdout_tail = redactor.redact(&stdout_tail);

    let stderr_errors = extract_error_lines(&stderr_tail, max_lines);
    if !stderr_errors.is_empty() {
        return Ok(CommandLogSummary {
            task_id: request.task_id,
            run_id: request.run_id,
            source_stream: CommandOutputStream::Stderr,
            lines: stderr_errors,
            truncated: stderr_truncated || stdout_truncated,
        });
    }

    let stdout_errors = extract_error_lines(&stdout_tail, max_lines);
    if !stdout_errors.is_empty() {
        return Ok(CommandLogSummary {
            task_id: request.task_id,
            run_id: request.run_id,
            source_stream: CommandOutputStream::Stdout,
            lines: stdout_errors,
            truncated: stderr_truncated || stdout_truncated,
        });
    }

    let fallback = tail_lines(&stderr_tail, max_lines)
        .into_iter()
        .chain(tail_lines(&stdout_tail, max_lines))
        .take(max_lines)
        .collect();

    Ok(CommandLogSummary {
        task_id: request.task_id,
        run_id: request.run_id,
        source_stream: CommandOutputStream::Stderr,
        lines: fallback,
        truncated: stderr_truncated || stdout_truncated,
    })
}

#[tauri::command]
pub fn cleanup_expired_task_logs(
    storage: State<'_, ManagedStorage>,
) -> AppResult<LogCleanupResult> {
    let retention_days = {
        let store = storage.store.lock().map_err(|_| storage_lock_error())?;
        StoragePolicyRepository::new(store.connection())
            .default_policy()
            .map_err(storage_error)?
            .raw_log_retention_days
    };

    if retention_days < 0 {
        return Ok(LogCleanupResult {
            retention_days,
            scanned_files: 0,
            deleted_files: 0,
            deleted_bytes: 0,
            cleanup_disabled: true,
        });
    }

    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(retention_days as u64 * 24 * 60 * 60))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut result = LogCleanupResult {
        retention_days,
        scanned_files: 0,
        deleted_files: 0,
        deleted_bytes: 0,
        cleanup_disabled: false,
    };

    cleanup_logs_under_root(&storage.roots.artifact_root, cutoff, &mut result)
        .map_err(log_read_error)?;

    Ok(result)
}

fn load_task(storage: &ManagedStorage, task_id: &str) -> AppResult<TaskRecord> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    TaskRepository::new(store.connection())
        .get_required(task_id)
        .map_err(storage_error)
}

fn load_command_run(
    storage: &ManagedStorage,
    task_id: &str,
    run_id: &str,
) -> AppResult<CommandRunRecord> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    CommandRunRepository::new(store.connection())
        .get_for_task(task_id, run_id)
        .map_err(storage_error)
}

#[derive(Debug, Clone)]
struct CommandPathGuard {
    cwd: PathBuf,
    worktree: PathBuf,
}

fn validate_command_cwd(task: &TaskRecord, cwd: &str) -> AppResult<CommandPathGuard> {
    let worktree_path = task.worktree_path.as_deref().ok_or_else(|| {
        CommandError::new(
            "command.worktreeMissing",
            format!("Task {} does not have a saved worktree path.", task.id),
        )
    })?;

    let cwd = canonical_directory(cwd).map_err(|error| {
        CommandError::new(
            "command.cwdUnavailable",
            format!("Command working directory is unavailable: {error}"),
        )
    })?;
    let worktree = canonical_directory(worktree_path).map_err(|error| {
        CommandError::new(
            "command.worktreeUnavailable",
            format!("Task worktree is unavailable: {error}"),
        )
    })?;

    if !path_starts_with(&cwd, &worktree) {
        return Err(CommandError::new(
            "command.cwdOutsideWorktree",
            format!(
                "Command working directory must stay inside the task worktree: {}",
                worktree.to_string_lossy()
            ),
        ));
    }

    Ok(CommandPathGuard { cwd, worktree })
}

fn enforce_command_safety(
    storage: &ManagedStorage,
    task: &TaskRecord,
    request: &CommandRequest,
    path_guard: &CommandPathGuard,
    redactor: &LogRedactor,
) -> AppResult<()> {
    let assessment =
        safety::assess_command(&request.command, &path_guard.cwd, &path_guard.worktree);

    if assessment.denied {
        return Err(CommandError::new(
            "command.blockedBySafetyPolicy",
            format!(
                "Command was blocked before execution. {}",
                assessment.reason
            ),
        ));
    }

    if !assessment.requires_approval {
        return Ok(());
    }

    let content = approval_content(request, redactor);
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    let approvals = ApprovalRepository::new(connection);

    if let Some(existing) = approvals
        .find_for_content(&task.id, "command", &content)
        .map_err(storage_error)?
    {
        return match existing.decision.as_deref() {
            Some("approved") => Ok(()),
            Some("rejected") => Err(CommandError::new(
                "approval.rejected",
                format!(
                    "Command was rejected by the user and will not run: {}",
                    existing.comment.unwrap_or(existing.reason)
                ),
            )),
            Some("revise") => Err(CommandError::new(
                "approval.reviseRequested",
                format!(
                    "User requested a revised plan before this command can run: {}",
                    existing.comment.unwrap_or(existing.reason)
                ),
            )),
            _ => Err(CommandError::new(
                "approval.pending",
                format!(
                    "Command requires approval before execution. Approval id: {}",
                    existing.id
                ),
            )),
        };
    }

    let approval = create_command_approval(&approvals, &task.id, &content, &assessment)?;
    TaskRepository::new(connection)
        .update_status(&task.id, "waitingApproval", None)
        .map_err(storage_error)?;

    Err(CommandError::new(
        "approval.required",
        format!(
            "Command requires approval before execution. Approval id: {}",
            approval.id
        ),
    ))
}

fn create_command_approval(
    approvals: &ApprovalRepository<'_>,
    task_id: &str,
    content: &str,
    assessment: &RiskAssessment,
) -> AppResult<ApprovalRecord> {
    let id = format!("approval-{}", uuid::Uuid::new_v4());
    approvals
        .create(NewApproval {
            id: &id,
            task_id,
            approval_type: "command",
            risk_level: assessment.level.as_str(),
            content,
            reason: &approval_reason(assessment),
        })
        .map_err(storage_error)
}

fn approval_content(request: &CommandRequest, redactor: &LogRedactor) -> String {
    redactor.redact(&format!(
        "command: {}\ncwd: {}",
        request.command.trim(),
        request.cwd.trim()
    ))
}

fn approval_reason(assessment: &RiskAssessment) -> String {
    let operations = assessment
        .operations
        .iter()
        .map(|operation| operation.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let matched_rules = if assessment.matched_rule_ids.is_empty() {
        "none".to_string()
    } else {
        assessment.matched_rule_ids.join(", ")
    };

    format!(
        "{} Operations: [{}]. Matched rules: [{}].",
        assessment.reason, operations, matched_rules
    )
}

fn command_log_paths(
    storage: &ManagedStorage,
    task_id: &str,
    run_id: &str,
) -> AppResult<CommandLogPaths> {
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(task_id)
        .map_err(storage_error)?;
    let run_file_stem = safe_file_stem(run_id);

    Ok(CommandLogPaths {
        stdout_path: paths.logs_dir.join(format!("{run_file_stem}.stdout.log")),
        stderr_path: paths.logs_dir.join(format!("{run_file_stem}.stderr.log")),
    })
}

fn resolve_command_log_path(
    storage: &ManagedStorage,
    run: &CommandRunRecord,
    stream: CommandOutputStream,
) -> AppResult<PathBuf> {
    let saved_path = match stream {
        CommandOutputStream::Stdout => run.stdout_path.as_deref(),
        CommandOutputStream::Stderr => run.stderr_path.as_deref(),
    }
    .ok_or_else(|| {
        CommandError::new(
            "command.logPathMissing",
            format!(
                "Command run {} does not have a saved {} log path.",
                run.id,
                stream.as_str()
            ),
        )
    })?;
    let path = existing_log_path(PathBuf::from(saved_path)).ok_or_else(|| {
        CommandError::new(
            "command.logFileMissing",
            format!("Command log file is no longer available: {saved_path}"),
        )
    })?;
    let artifact_root = storage
        .roots
        .artifact_root
        .canonicalize()
        .map_err(|error| {
            CommandError::new(
                "storage.artifactRootUnavailable",
                format!("Task artifact root is unavailable: {error}"),
            )
        })?;
    let canonical = path.canonicalize().map_err(|error| {
        CommandError::new(
            "command.logFileUnavailable",
            format!("Command log file is unavailable: {error}"),
        )
    })?;

    if !path_starts_with(&canonical, &artifact_root) {
        return Err(CommandError::new(
            "command.logPathOutsideArtifacts",
            "Command log path must stay inside the task artifact directory.",
        ));
    }

    Ok(canonical)
}

fn existing_log_path(path: PathBuf) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path);
    }

    let file_name = path.file_name()?.to_str()?;
    let gz_path = path.with_file_name(format!("{file_name}.gz"));
    gz_path.is_file().then_some(gz_path)
}

fn record_command_result(
    storage: &ManagedStorage,
    result: &CommandExecutionResult,
) -> AppResult<()> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    CommandRunRepository::new(store.connection())
        .record(NewCommandRun {
            id: &result.run_id,
            task_id: &result.task_id,
            command: &result.command,
            cwd: &result.cwd,
            status: &result.status,
            stdout_path: Some(&result.stdout_path),
            stderr_path: Some(&result.stderr_path),
            exit_code: result.exit_code.map(i64::from),
            duration_ms: Some(result.duration_ms as i64),
        })
        .map_err(storage_error)?;
    Ok(())
}

fn stored_secret_redactor(storage: &ManagedStorage) -> LogRedactor {
    LogRedactor::from_values(load_model_api_key_values(storage))
}

fn clamp_log_page_size(max_bytes: Option<u64>) -> u64 {
    max_bytes
        .unwrap_or(DEFAULT_LOG_PAGE_BYTES)
        .clamp(1, MAX_LOG_PAGE_BYTES)
}

fn read_log_window(path: &Path, offset: u64, max_bytes: usize) -> io::Result<(Vec<u8>, bool)> {
    if is_gzip_path(path) {
        let file = fs::File::open(path)?;
        let mut decoder = GzDecoder::new(file);
        read_stream_window(&mut decoder, offset, max_bytes)
    } else {
        read_plain_window(path, offset, max_bytes)
    }
}

fn read_plain_window(path: &Path, offset: u64, max_bytes: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let offset = offset.min(len);
    file.seek(SeekFrom::Start(offset))?;

    let mut limited = file.take(max_bytes as u64);
    let mut bytes = Vec::with_capacity(max_bytes.min(8192));
    limited.read_to_end(&mut bytes)?;
    let eof = offset + bytes.len() as u64 >= len;

    Ok((bytes, eof))
}

fn read_stream_window<R: Read>(
    reader: &mut R,
    offset: u64,
    max_bytes: usize,
) -> io::Result<(Vec<u8>, bool)> {
    let mut buffer = [0_u8; 8192];
    let mut skipped = 0_u64;

    while skipped < offset {
        let remaining = (offset - skipped).min(buffer.len() as u64) as usize;
        let read = reader.read(&mut buffer[..remaining])?;
        if read == 0 {
            return Ok((Vec::new(), true));
        }
        skipped += read as u64;
    }

    let mut bytes = Vec::with_capacity(max_bytes.min(8192));
    while bytes.len() < max_bytes {
        let remaining = (max_bytes - bytes.len()).min(buffer.len());
        let read = reader.read(&mut buffer[..remaining])?;
        if read == 0 {
            return Ok((bytes, true));
        }
        bytes.extend_from_slice(&buffer[..read]);
    }

    let mut probe = [0_u8; 1];
    Ok((bytes, reader.read(&mut probe)? == 0))
}

fn read_log_tail(path: &Path, max_bytes: usize) -> io::Result<(String, bool)> {
    let (bytes, truncated) = if is_gzip_path(path) {
        let file = fs::File::open(path)?;
        let mut decoder = GzDecoder::new(file);
        read_stream_tail(&mut decoder, max_bytes)?
    } else {
        read_plain_tail(path, max_bytes)?
    };

    Ok((String::from_utf8_lossy(&bytes).to_string(), truncated))
}

fn read_plain_tail(path: &Path, max_bytes: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes as u64);
    file.seek(SeekFrom::Start(start))?;

    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;

    Ok((bytes, start > 0))
}

fn read_stream_tail<R: Read>(reader: &mut R, max_bytes: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut buffer = [0_u8; 8192];
    let mut tail = Vec::with_capacity(max_bytes.min(8192));
    let mut total = 0_usize;

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok((tail, total > max_bytes));
        }

        total += read;
        tail.extend_from_slice(&buffer[..read]);
        if tail.len() > max_bytes {
            let overflow = tail.len() - max_bytes;
            tail.drain(..overflow);
        }
    }
}

fn extract_error_lines(text: &str, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| is_error_line(line))
        .map(trim_log_line)
        .filter(|line| !line.is_empty())
        .collect();

    if lines.len() > max_lines {
        lines.drain(..lines.len() - max_lines);
    }

    lines
}

fn tail_lines(text: &str, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .map(trim_log_line)
        .filter(|line| !line.is_empty())
        .collect();

    if lines.len() > max_lines {
        lines.drain(..lines.len() - max_lines);
    }

    lines
}

fn trim_log_line(line: &str) -> String {
    line.trim().chars().take(500).collect()
}

fn is_error_line(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    [
        "error",
        "failed",
        "failure",
        "exception",
        "panic",
        "fatal",
        "traceback",
        "not found",
        "cannot find",
        "permission denied",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn cleanup_logs_under_root(
    artifact_root: &Path,
    cutoff: SystemTime,
    result: &mut LogCleanupResult,
) -> io::Result<()> {
    if !artifact_root.exists() {
        return Ok(());
    }

    for task_entry in fs::read_dir(artifact_root)? {
        let task_entry = task_entry?;
        if !task_entry.file_type()?.is_dir() {
            continue;
        }

        cleanup_log_dir(&task_entry.path().join("logs"), cutoff, result)?;
    }

    Ok(())
}

fn cleanup_log_dir(
    logs_dir: &Path,
    cutoff: SystemTime,
    result: &mut LogCleanupResult,
) -> io::Result<()> {
    if !logs_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(logs_dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            cleanup_log_dir(&path, cutoff, result)?;
            continue;
        }

        if !metadata.is_file() || !is_command_log_file(&path) {
            continue;
        }

        result.scanned_files += 1;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if modified > cutoff {
            continue;
        }

        let bytes = metadata.len();
        fs::remove_file(&path)?;
        result.deleted_files += 1;
        result.deleted_bytes += bytes;
    }

    Ok(())
}

fn is_command_log_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".log") || name.ends_with(".log.gz"))
}

fn is_gzip_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"))
}

fn canonical_directory(path: impl AsRef<Path>) -> Result<PathBuf, std::io::Error> {
    let path = path.as_ref();
    let canonical = path.canonicalize()?;
    if canonical.is_dir() {
        Ok(canonical)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("path is not a directory: {}", path.to_string_lossy()),
        ))
    }
}

fn path_starts_with(path: &Path, root: &Path) -> bool {
    #[cfg(windows)]
    {
        let path = path.to_string_lossy().to_ascii_lowercase();
        let root = root.to_string_lossy().to_ascii_lowercase();
        path == root || path.starts_with(&format!("{root}\\"))
    }

    #[cfg(not(windows))]
    {
        path.starts_with(root)
    }
}

fn safe_file_stem(value: &str) -> String {
    let stem: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let stem = stem.trim_matches('_');

    if stem.is_empty() {
        "command".to_string()
    } else {
        stem.chars().take(96).collect()
    }
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

fn log_read_error(error: std::io::Error) -> CommandError {
    CommandError::new(
        "command.logReadFailed",
        format!("Unable to read command log file: {error}"),
    )
}

fn command_execution_error(error: CommandExecutionError) -> CommandError {
    match error {
        CommandExecutionError::EmptyCommand => {
            CommandError::new("command.empty", "Command cannot be empty.")
        }
        CommandExecutionError::DuplicateRun(run_id) => CommandError::new(
            "command.duplicateRun",
            format!("Command run is already active: {run_id}"),
        ),
        CommandExecutionError::RegistryUnavailable => CommandError::new(
            "command.registryUnavailable",
            "Command execution registry is temporarily unavailable.",
        ),
        CommandExecutionError::LogFile(error) => CommandError::new(
            "command.logFileUnavailable",
            format!("Unable to write command log file: {error}"),
        ),
        CommandExecutionError::Spawn(message) => CommandError::new(
            "command.spawnFailed",
            format!("Unable to start command: {message}"),
        ),
        CommandExecutionError::Join(message) => CommandError::new(
            "command.outputCaptureFailed",
            format!("Unable to capture command output: {message}"),
        ),
    }
}

fn event_error(error: tauri::Error) -> CommandError {
    CommandError::new(
        "event.emitFailed",
        format!("Unable to notify the desktop UI: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        secrets::SecretStore,
        storage::{ModelConfigRepository, NewModelConfig, NewTask, SqliteStore, StorageRoots},
    };
    use std::collections::BTreeMap;

    #[test]
    fn safe_file_stem_removes_path_separators_and_limits_length() {
        let value = "run/with\\unsafe:*chars?".repeat(12);
        let stem = safe_file_stem(&value);

        assert!(!stem.contains('/'));
        assert!(!stem.contains('\\'));
        assert!(stem.len() <= 96);
    }

    #[test]
    fn path_starts_with_allows_nested_worktree_paths() {
        let root = PathBuf::from("D:/codemax/worktrees/task-001");
        let child = root.join("src");

        assert!(path_starts_with(&child, &root));
        assert!(path_starts_with(&root, &root));
    }

    #[test]
    fn path_starts_with_rejects_sibling_prefixes() {
        let root = PathBuf::from("D:/codemax/worktrees/task-001");
        let sibling = PathBuf::from("D:/codemax/worktrees/task-001-other");

        assert!(!path_starts_with(&sibling, &root));
    }

    #[test]
    fn reads_plain_log_pages_by_byte_offset() {
        let root = std::env::temp_dir().join(format!("codemax-log-page-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp dir");
        let path = root.join("run.stdout.log");
        std::fs::write(&path, "0123456789").expect("write log");

        let (first, first_eof) = read_log_window(&path, 0, 4).expect("read first page");
        let (second, second_eof) = read_log_window(&path, 4, 20).expect("read second page");

        assert_eq!(String::from_utf8_lossy(&first), "0123");
        assert!(!first_eof);
        assert_eq!(String::from_utf8_lossy(&second), "456789");
        assert!(second_eof);

        std::fs::remove_dir_all(root).expect("clean temp dir");
    }

    #[test]
    fn extracts_error_summary_from_tail() {
        let lines =
            extract_error_lines("ok\nTraceback: call failed\nERROR test failed\nall done", 1);

        assert_eq!(lines, vec!["ERROR test failed".to_string()]);
    }

    fn safety_storage() -> (ManagedStorage, TaskRecord, CommandPathGuard, CommandRequest) {
        let root =
            std::env::temp_dir().join(format!("codemax-safety-test-{}", uuid::Uuid::new_v4()));
        let worktree = root.join("worktrees").join("task-001");
        std::fs::create_dir_all(&worktree).expect("create worktree");

        let store = SqliteStore::open_in_memory().expect("open sqlite");
        store.migrate().expect("migrate sqlite");
        let task = TaskRepository::new(store.connection())
            .create(NewTask {
                id: "task-001",
                title: "Safety test",
                description: "Exercise S9 command approval",
                task_type: "custom",
                status: "validating",
                repository_path: root.to_string_lossy().as_ref(),
                worktree_path: Some(worktree.to_string_lossy().as_ref()),
                branch_name: Some("codex/task-001"),
                model_id: None,
            })
            .expect("create task");
        let storage = ManagedStorage {
            roots: StorageRoots::from_app_data_dir(root),
            store: std::sync::Mutex::new(store),
        };
        let guard = CommandPathGuard {
            cwd: worktree.clone(),
            worktree,
        };
        let request = CommandRequest {
            task_id: "task-001".to_string(),
            run_id: Some("run-001".to_string()),
            command: "npm install left-pad".to_string(),
            cwd: guard.cwd.to_string_lossy().to_string(),
            env: BTreeMap::new(),
            timeout_ms: Some(30_000),
        };

        (storage, task, guard, request)
    }

    fn test_redactor() -> LogRedactor {
        LogRedactor::from_values(Vec::new())
    }

    #[test]
    fn approval_content_redacts_secret_values() {
        let request = CommandRequest {
            task_id: "task-001".to_string(),
            run_id: Some("run-001".to_string()),
            command: "echo sk-test-secret-value".to_string(),
            cwd: "D:/repo/worktree".to_string(),
            env: BTreeMap::new(),
            timeout_ms: Some(30_000),
        };
        let redactor = LogRedactor::from_values(vec!["sk-test-secret-value".to_string()]);
        let content = approval_content(&request, &redactor);

        assert!(content.contains("[REDACTED]"));
        assert!(!content.contains("sk-test-secret-value"));
    }

    #[cfg(windows)]
    #[test]
    fn stored_secret_redactor_masks_saved_model_keys() {
        let (storage, root) = {
            let root = std::env::temp_dir().join(format!(
                "codemax-command-redaction-{}",
                uuid::Uuid::new_v4()
            ));
            let store = SqliteStore::open_in_memory().expect("open sqlite");
            store.migrate().expect("migrate sqlite");
            let storage = ManagedStorage {
                roots: StorageRoots::from_app_data_dir(&root),
                store: std::sync::Mutex::new(store),
            };
            (storage, root)
        };
        let secret = "sk-test-saved-key";
        let secret_ref = SecretStore::new(&storage.roots.app_data_dir)
            .put_model_api_key("model-default", secret)
            .expect("save secret");
        let store = storage.store.lock().expect("storage lock");
        ModelConfigRepository::new(store.connection())
            .save(NewModelConfig {
                id: "model-default",
                provider: "openai-compatible",
                base_url: "",
                model_name: "codemax-test",
                api_key_secret_ref: Some(&secret_ref),
            })
            .expect("save model config");
        drop(store);

        let redactor = stored_secret_redactor(&storage);

        assert_eq!(
            redactor.redact("printed sk-test-saved-key"),
            "printed [REDACTED]"
        );

        std::fs::remove_dir_all(root).expect("clean temp redaction storage");
    }

    #[test]
    fn high_risk_command_creates_pending_approval_and_suspends_task() {
        let (storage, task, guard, request) = safety_storage();

        let error = enforce_command_safety(&storage, &task, &request, &guard, &test_redactor())
            .expect_err("dependency install should require approval");

        assert_eq!(error.code, "approval.required");

        let store = storage.store.lock().expect("storage lock");
        let approvals = ApprovalRepository::new(store.connection())
            .list_for_task("task-001")
            .expect("list approvals");
        assert_eq!(approvals.len(), 1);
        assert_eq!(approvals[0].decision, None);
        assert_eq!(
            TaskRepository::new(store.connection())
                .get_required("task-001")
                .expect("load task")
                .status,
            "waitingApproval"
        );
    }

    #[test]
    fn rejected_approval_blocks_command_execution() {
        let (storage, task, guard, request) = safety_storage();
        let redactor = test_redactor();
        let _ = enforce_command_safety(&storage, &task, &request, &guard, &redactor);
        let store = storage.store.lock().expect("storage lock");
        let approval = ApprovalRepository::new(store.connection())
            .list_for_task("task-001")
            .expect("list approvals")
            .remove(0);
        ApprovalRepository::new(store.connection())
            .decide(
                &approval.id,
                "rejected",
                Some("Do not install new dependencies"),
            )
            .expect("reject approval");
        drop(store);

        let error = enforce_command_safety(&storage, &task, &request, &guard, &redactor)
            .expect_err("rejected command should stay blocked");

        assert_eq!(error.code, "approval.rejected");
    }

    #[test]
    fn approved_command_can_continue() {
        let (storage, task, guard, request) = safety_storage();
        let redactor = test_redactor();
        let _ = enforce_command_safety(&storage, &task, &request, &guard, &redactor);
        let store = storage.store.lock().expect("storage lock");
        let approval = ApprovalRepository::new(store.connection())
            .list_for_task("task-001")
            .expect("list approvals")
            .remove(0);
        ApprovalRepository::new(store.connection())
            .decide(
                &approval.id,
                "approved",
                Some("Run once in the task worktree"),
            )
            .expect("approve command");
        drop(store);

        enforce_command_safety(&storage, &task, &request, &guard, &redactor)
            .expect("approved command should be allowed");
    }
}
