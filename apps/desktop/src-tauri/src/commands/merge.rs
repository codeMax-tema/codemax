use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

use crate::{
    core::error::{AppResult, CommandError},
    git::{self, GitError, TaskMergeStatus},
    storage::{
        ArtifactRecord, ArtifactRepository, CommandRunRecord, CommandRunRepository, ManagedStorage,
        NewArtifact, NewArtifactFile, StorageError, TaskRecord, TaskRepository,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareTaskMergeRequest {
    pub task_id: String,
    pub target_branch: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedTaskMerge {
    pub task_id: String,
    pub target_branch: String,
    pub source_branch: String,
    pub worktree_path: String,
    pub target_dirty: bool,
    pub worktree_dirty: bool,
    pub validation_status: String,
    pub validation_run_count: usize,
    pub validation_summary: String,
    pub diff_file_count: usize,
    pub additions: u64,
    pub deletions: u64,
    pub diff_path: Option<String>,
    pub commit_message: String,
    pub blockers: Vec<String>,
    pub can_merge: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeTaskRequest {
    pub task_id: String,
    pub target_branch: Option<String>,
    pub commit_message: String,
    pub confirmed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMergeCommandResult {
    pub task_id: String,
    pub status: TaskMergeStatus,
    pub target_branch: String,
    pub source_branch: String,
    pub commit_sha: String,
    pub commit_message: String,
    pub conflict_files: Vec<String>,
    pub error_reason: Option<String>,
    pub merge_record_path: Option<String>,
    pub task_status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskMergeRecord {
    task_id: String,
    status: TaskMergeStatus,
    target_branch: String,
    source_branch: String,
    commit_sha: String,
    commit_message: String,
    conflict_files: Vec<String>,
    error_reason: Option<String>,
    recorded_at: String,
}

#[tauri::command]
pub fn prepare_task_merge(
    storage: State<'_, ManagedStorage>,
    request: PrepareTaskMergeRequest,
) -> AppResult<PreparedTaskMerge> {
    prepare_task_merge_inner(&storage, request)
}

#[tauri::command]
pub fn merge_task(
    storage: State<'_, ManagedStorage>,
    request: MergeTaskRequest,
) -> AppResult<TaskMergeCommandResult> {
    merge_task_inner(&storage, request)
}

pub(crate) fn merge_task_inner(
    storage: &ManagedStorage,
    request: MergeTaskRequest,
) -> AppResult<TaskMergeCommandResult> {
    require_merge_confirmation(request.confirmed)?;

    let task_id = request.task_id.trim().to_string();
    if task_id.is_empty() {
        return Err(CommandError::new(
            "merge.taskIdRequired",
            "Task id is required to merge a task.",
        ));
    }

    let commit_message = request.commit_message.trim().to_string();
    let prepared = prepare_task_merge_inner(
        &storage,
        PrepareTaskMergeRequest {
            task_id: task_id.clone(),
            target_branch: request.target_branch,
        },
    )?;
    let blockers = merge_blockers(
        prepared.target_dirty,
        &prepared.validation_status,
        prepared.diff_file_count,
        &commit_message,
    );
    if !blockers.is_empty() {
        return Err(CommandError::new(
            "merge.precheckFailed",
            format!("Merge precheck failed: {}", blockers.join("; ")),
        ));
    }

    set_task_status(&storage, &task_id, "merging", None)?;
    let merge_result = match git::merge_task_branch(
        load_task(&storage, &task_id)?.repository_path,
        &prepared.worktree_path,
        &prepared.target_branch,
        &commit_message,
    ) {
        Ok(result) => result,
        Err(error) => {
            let merge_error = merge_git_error(error);
            if let Err(status_error) =
                set_task_status(&storage, &task_id, "needsIntervention", None)
            {
                return Err(CommandError::new(
                    "merge.statusUpdateFailed",
                    format!(
                        "{} Task status update also failed: {}",
                        merge_error.message, status_error.message
                    ),
                ));
            }
            return Err(merge_error);
        }
    };

    let task_status = match merge_result.status {
        TaskMergeStatus::Merged => "merged",
        TaskMergeStatus::Conflicted => "needsIntervention",
    };
    let completed_at = matches!(merge_result.status, TaskMergeStatus::Merged).then(|| now_text());
    let merge_record_path = match record_merge_result(&storage, &task_id, &merge_result) {
        Ok(path) => path,
        Err(error) => {
            if let Err(status_error) =
                set_task_status(&storage, &task_id, "needsIntervention", None)
            {
                return Err(CommandError::new(
                    "merge.statusUpdateFailed",
                    format!(
                        "{} Task status update also failed: {}",
                        error.message, status_error.message
                    ),
                ));
            }
            return Err(error);
        }
    };
    set_task_status(&storage, &task_id, task_status, completed_at.as_deref())?;

    Ok(TaskMergeCommandResult {
        task_id,
        status: merge_result.status,
        target_branch: merge_result.target_branch,
        source_branch: merge_result.source_branch,
        commit_sha: merge_result.commit_sha,
        commit_message: merge_result.commit_message,
        conflict_files: merge_result.conflict_files,
        error_reason: merge_result.error_reason,
        merge_record_path: Some(merge_record_path),
        task_status: task_status.to_string(),
    })
}

pub(crate) fn prepare_task_merge_inner(
    storage: &ManagedStorage,
    request: PrepareTaskMergeRequest,
) -> AppResult<PreparedTaskMerge> {
    let task_id = request.task_id.trim().to_string();
    if task_id.is_empty() {
        return Err(CommandError::new(
            "merge.taskIdRequired",
            "Task id is required to prepare a merge.",
        ));
    }

    let (task, command_runs, artifacts) = load_merge_inputs(storage, &task_id)?;
    let worktree_path = task.worktree_path.clone().ok_or_else(|| {
        CommandError::new(
            "merge.worktreeMissing",
            format!("Task {task_id} does not have a saved worktree path."),
        )
    })?;
    let target_branch = match request.target_branch.as_deref().map(str::trim) {
        Some(branch) if !branch.is_empty() => branch.to_string(),
        _ => git::current_branch(&task.repository_path).map_err(merge_git_error)?,
    };
    let source_branch = git::current_branch(&worktree_path).map_err(merge_git_error)?;
    let target_dirty =
        git::has_uncommitted_changes(&task.repository_path).map_err(merge_git_error)?;
    let worktree_dirty = git::has_uncommitted_changes(&worktree_path).map_err(merge_git_error)?;
    let diff = git::task_diff(&task.id, &worktree_path, &target_branch).map_err(merge_git_error)?;
    let validation_runs = latest_validation_runs(&command_runs);
    let validation_status = validation_status(&validation_runs).to_string();
    let validation_summary = validation_summary(&validation_status, validation_runs.len());
    let diff_path =
        latest_diff_artifact(&artifacts).and_then(|artifact| artifact.diff_path.clone());
    let commit_message =
        latest_commit_message(&artifacts).unwrap_or_else(|| fallback_commit_message(&task));
    let blockers = merge_blockers(
        target_dirty,
        &validation_status,
        diff.files.len(),
        &commit_message,
    );

    Ok(PreparedTaskMerge {
        task_id: task.id,
        target_branch,
        source_branch,
        worktree_path,
        target_dirty,
        worktree_dirty,
        validation_status,
        validation_run_count: validation_runs.len(),
        validation_summary,
        diff_file_count: diff.files.len(),
        additions: diff.additions,
        deletions: diff.deletions,
        diff_path,
        commit_message,
        can_merge: blockers.is_empty(),
        blockers,
    })
}

fn require_merge_confirmation(confirmed: bool) -> AppResult<()> {
    if confirmed {
        return Ok(());
    }

    Err(CommandError::new(
        "merge.confirmationRequired",
        "Local merge requires explicit user confirmation.",
    ))
}

fn latest_validation_runs(runs: &[CommandRunRecord]) -> Vec<&CommandRunRecord> {
    let mut latest: Vec<&CommandRunRecord> = Vec::new();

    for run in runs {
        let key = (run.command.as_str(), run.cwd.as_str());
        if let Some(existing) = latest
            .iter_mut()
            .find(|existing| (existing.command.as_str(), existing.cwd.as_str()) == key)
        {
            *existing = run;
        } else {
            latest.push(run);
        }
    }

    latest
}

fn validation_status(runs: &[&CommandRunRecord]) -> &'static str {
    if runs.is_empty() {
        return "notRun";
    }

    if runs
        .iter()
        .all(|run| run.status == "passed" && run.exit_code.unwrap_or(0) == 0)
    {
        "passed"
    } else {
        "failed"
    }
}

fn validation_summary(status: &str, run_count: usize) -> String {
    match status {
        "passed" => format!("{run_count} validation command(s) passed."),
        "failed" => {
            "At least one validation command failed, timed out, or was cancelled.".to_string()
        }
        _ => "No validation command has been recorded yet.".to_string(),
    }
}

fn merge_blockers(
    target_dirty: bool,
    validation_status: &str,
    diff_file_count: usize,
    commit_message: &str,
) -> Vec<String> {
    let mut blockers = Vec::new();

    if target_dirty {
        blockers.push("target branch has uncommitted changes".to_string());
    }
    if validation_status != "passed" {
        blockers.push("validation has not passed".to_string());
    }
    if diff_file_count == 0 {
        blockers.push("final diff is empty".to_string());
    }
    if commit_message.trim().is_empty() {
        blockers.push("commit message is required".to_string());
    }

    blockers
}

fn load_merge_inputs(
    storage: &ManagedStorage,
    task_id: &str,
) -> AppResult<(TaskRecord, Vec<CommandRunRecord>, Vec<ArtifactRecord>)> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    let task = TaskRepository::new(connection)
        .get_required(task_id)
        .map_err(storage_error)?;
    let command_runs = CommandRunRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?;
    let artifacts = ArtifactRepository::new(connection)
        .artifacts_for_task(task_id)
        .map_err(storage_error)?;

    Ok((task, command_runs, artifacts))
}

fn load_task(storage: &ManagedStorage, task_id: &str) -> AppResult<TaskRecord> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    TaskRepository::new(store.connection())
        .get_required(task_id)
        .map_err(storage_error)
}

fn set_task_status(
    storage: &ManagedStorage,
    task_id: &str,
    status: &str,
    completed_at: Option<&str>,
) -> AppResult<()> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    TaskRepository::new(store.connection())
        .update_status(task_id, status, completed_at)
        .map_err(storage_error)
}

fn latest_diff_artifact(artifacts: &[ArtifactRecord]) -> Option<&ArtifactRecord> {
    artifacts
        .iter()
        .rev()
        .find(|artifact| artifact.diff_path.is_some())
}

fn latest_commit_message(artifacts: &[ArtifactRecord]) -> Option<String> {
    artifacts
        .iter()
        .rev()
        .map(|artifact| artifact.commit_message.trim())
        .find(|message| !message.is_empty())
        .map(ToString::to_string)
}

fn fallback_commit_message(task: &TaskRecord) -> String {
    let commit_type = match task.task_type.as_str() {
        "bugfix" => "fix",
        "test" => "test",
        "refactor" => "refactor",
        "explain" => "docs",
        _ => "feat",
    };

    format!("{commit_type}: {}", task.title)
}

fn record_merge_result(
    storage: &ManagedStorage,
    task_id: &str,
    result: &git::TaskMergeResult,
) -> AppResult<String> {
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(task_id)
        .map_err(storage_error)?;
    let record_path = paths.artifacts_dir.join("merge-record.json");
    let record = TaskMergeRecord {
        task_id: task_id.to_string(),
        status: result.status.clone(),
        target_branch: result.target_branch.clone(),
        source_branch: result.source_branch.clone(),
        commit_sha: result.commit_sha.clone(),
        commit_message: result.commit_message.clone(),
        conflict_files: result.conflict_files.clone(),
        error_reason: result.error_reason.clone(),
        recorded_at: now_text(),
    };
    let record_json = serde_json::to_string_pretty(&record).map_err(json_error)?;
    fs::write(&record_path, record_json).map_err(storage_error)?;

    let artifact_id = format!("merge-{task_id}-{}", Uuid::new_v4());
    let file_id = format!("file-{artifact_id}");
    let record_path_text = record_path.to_string_lossy().to_string();
    let size_bytes = file_size(&record_path).map_err(storage_error)?;
    let changed_files = changed_files_json(storage, task_id, result)?;
    let summary = match result.status {
        TaskMergeStatus::Merged => format!(
            "Merged {} into {} at {}.",
            result.source_branch, result.target_branch, result.commit_sha
        ),
        TaskMergeStatus::Conflicted => format!(
            "Merge conflicted while merging {} into {}. {}",
            result.source_branch,
            result.target_branch,
            result.error_reason.as_deref().unwrap_or_default()
        ),
    };

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let artifacts = ArtifactRepository::new(store.connection());
    artifacts
        .record_artifact(NewArtifact {
            id: &artifact_id,
            task_id,
            changed_files: &changed_files,
            diff_path: None,
            test_report_path: Some(&record_path_text),
            screenshots: "[]",
            summary: &summary,
            commit_message: &result.commit_message,
        })
        .map_err(storage_error)?;
    artifacts
        .record_file(NewArtifactFile {
            id: &file_id,
            task_id,
            artifact_id: Some(&artifact_id),
            file_type: "merge_record",
            path: &record_path_text,
            size_bytes: size_bytes as i64,
            compressed: false,
            retention_class: "permanent",
            expires_at: None,
        })
        .map_err(storage_error)?;

    Ok(record_path_text)
}

fn changed_files_json(
    storage: &ManagedStorage,
    task_id: &str,
    result: &git::TaskMergeResult,
) -> AppResult<String> {
    if matches!(result.status, TaskMergeStatus::Conflicted) {
        return serde_json::to_string(&result.conflict_files).map_err(json_error);
    }

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let artifacts = ArtifactRepository::new(store.connection())
        .artifacts_for_task(task_id)
        .map_err(storage_error)?;
    let changed_files = latest_diff_artifact(&artifacts)
        .map(|artifact| artifact.changed_files.clone())
        .filter(|files| serde_json::from_str::<Value>(files).is_ok())
        .unwrap_or_else(|| "[]".to_string());

    Ok(changed_files)
}

fn file_size(path: &Path) -> std::io::Result<u64> {
    Ok(fs::metadata(path)?.len())
}

fn now_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
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

fn merge_git_error(error: GitError) -> CommandError {
    match error {
        GitError::PathNotFound(path) => CommandError::new(
            "merge.pathNotFound",
            format!("Path does not exist: {}", path.to_string_lossy()),
        ),
        GitError::PathNotDirectory(path) => CommandError::new(
            "merge.pathNotDirectory",
            format!("Path is not a directory: {}", path.to_string_lossy()),
        ),
        GitError::Io(error) => CommandError::new(
            "merge.filesystemError",
            format!("Filesystem error: {error}"),
        ),
        GitError::GitUnavailable(message) => CommandError::new(
            "merge.gitUnavailable",
            format!("Git is not available on this machine: {message}"),
        ),
        GitError::NotRepository { path, stderr } => CommandError::new(
            "merge.notGitRepository",
            format!(
                "Path is not a Git repository: {}{}",
                path.to_string_lossy(),
                format_stderr(&stderr)
            ),
        ),
        GitError::CommandFailed { path, args, stderr } => CommandError::new(
            "merge.gitCommandFailed",
            format!(
                "Git command failed in {}: git {}{}",
                path.to_string_lossy(),
                args,
                format_stderr(&stderr)
            ),
        ),
        GitError::InvalidTaskId(task_id) => CommandError::new(
            "merge.invalidTaskId",
            format!("Task id cannot produce a valid branch or worktree directory: {task_id}"),
        ),
        GitError::WorktreePathExists(path) => CommandError::new(
            "merge.worktreePathAlreadyExists",
            format!("Worktree path already exists: {}", path.to_string_lossy()),
        ),
        GitError::EmptyCommitMessage => CommandError::new(
            "merge.commitMessageRequired",
            "Merge commit message is required.",
        ),
        GitError::DirtyTarget(path) => CommandError::new(
            "merge.targetDirty",
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
            "merge.targetBranchChanged",
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
        "merge.invalidJson",
        format!("Unable to encode merge record: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        CommandRunRecord, CommandRunRepository, NewCommandRun, NewTask, SqliteStore, StorageRoots,
    };
    use std::{path::Path, path::PathBuf, process::Command, sync::Mutex};

    fn command_run(status: &str, exit_code: Option<i64>) -> CommandRunRecord {
        CommandRunRecord {
            id: format!("run-{status}"),
            task_id: "task-001".to_string(),
            command: "npm test".to_string(),
            cwd: "D:/codemax".to_string(),
            status: status.to_string(),
            stdout_path: None,
            stderr_path: None,
            exit_code,
            duration_ms: Some(1200),
            created_at: "1783372800".to_string(),
        }
    }

    #[test]
    fn merge_requires_explicit_confirmation() {
        let error = require_merge_confirmation(false).expect_err("merge requires confirmation");

        assert_eq!(error.code, "merge.confirmationRequired");
        require_merge_confirmation(true).expect("confirmed merge is allowed");
    }

    #[test]
    fn validation_status_requires_at_least_one_passing_run() {
        let failed = command_run("failed", Some(1));
        let passed = command_run("passed", Some(0));
        assert_eq!(validation_status(&[]), "notRun");
        assert_eq!(validation_status(&[&failed]), "failed");
        assert_eq!(validation_status(&[&passed]), "passed");
        assert_eq!(validation_status(&[&passed, &failed]), "failed");
    }

    #[test]
    fn validation_status_uses_latest_run_for_each_command() {
        let mut first_run = command_run("failed", Some(1));
        first_run.id = "run-1".to_string();
        first_run.command = "cargo test".to_string();
        first_run.cwd = "D:/codemax".to_string();
        let mut retry_run = command_run("passed", Some(0));
        retry_run.id = "run-2".to_string();
        retry_run.command = "cargo test".to_string();
        retry_run.cwd = "D:/codemax".to_string();

        let runs = [first_run, retry_run];
        let latest = latest_validation_runs(&runs);

        assert_eq!(latest.len(), 1);
        assert_eq!(validation_status(&latest), "passed");
    }

    #[test]
    fn merge_blockers_protect_dirty_targets_and_unverified_tasks() {
        let blockers = merge_blockers(true, "failed", 2, "feat: merge task");

        assert!(blockers.iter().any(|blocker| blocker.contains("target")));
        assert!(blockers
            .iter()
            .any(|blocker| blocker.contains("validation")));

        assert!(merge_blockers(false, "passed", 2, "feat: merge task").is_empty());
    }

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("codemax-merge-command-{label}-{}", Uuid::new_v4()))
    }

    fn run_test_git(path: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .expect("run test git command");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn committed_repository(label: &str) -> PathBuf {
        let path = temp_path(label);
        fs::create_dir_all(&path).expect("create temp repository directory");

        run_test_git(&path, &["init"]);
        run_test_git(&path, &["config", "user.email", "codemax@example.test"]);
        run_test_git(&path, &["config", "user.name", "Codemax Test"]);
        fs::write(path.join("modified.txt"), "before").expect("write fixture file");
        run_test_git(&path, &["add", "."]);
        run_test_git(&path, &["commit", "-m", "initial fixture"]);
        run_test_git(&path, &["branch", "release"]);

        path
    }

    fn merge_test_storage(
        repository_path: &Path,
        worktree_path: &Path,
        branch_name: &str,
    ) -> (ManagedStorage, PathBuf) {
        let root = temp_path("storage");
        let store = SqliteStore::open_in_memory().expect("open sqlite");
        store.migrate().expect("run migrations");
        {
            let connection = store.connection();
            TaskRepository::new(connection)
                .create(NewTask {
                    id: "task-merge-error",
                    title: "Merge error fixture",
                    description: "Task for merge error tests",
                    task_type: "custom",
                    status: "completed",
                    repository_path: &repository_path.to_string_lossy(),
                    worktree_path: Some(&worktree_path.to_string_lossy()),
                    branch_name: Some(branch_name),
                    model_id: None,
                })
                .expect("create merge task");
            CommandRunRepository::new(connection)
                .record(NewCommandRun {
                    id: "run-merge-passed",
                    task_id: "task-merge-error",
                    command: "cargo test",
                    cwd: &worktree_path.to_string_lossy(),
                    status: "passed",
                    stdout_path: None,
                    stderr_path: None,
                    exit_code: Some(0),
                    duration_ms: Some(1000),
                })
                .expect("record passing validation");
        }

        (
            ManagedStorage {
                roots: StorageRoots::from_app_data_dir(&root),
                store: Mutex::new(store),
            },
            root,
        )
    }

    #[test]
    fn merge_marks_task_needs_intervention_when_git_merge_errors() {
        let repository = committed_repository("target-branch-changed");
        let worktree_root = temp_path("worktree-root");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(worktree_path.join("modified.txt"), "after").expect("modify worktree file");
        let (storage, storage_root) =
            merge_test_storage(&repository, &worktree_path, &worktree.branch_name);

        let error = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: Some("release".to_string()),
                commit_message: "feat: merge task".to_string(),
                confirmed: true,
            },
        )
        .expect_err("target branch mismatch should fail");

        assert_eq!(error.code, "merge.targetBranchChanged");
        let stored_task = load_task(&storage, "task-merge-error").expect("load task");
        assert_eq!(stored_task.status, "needsIntervention");

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }
}
