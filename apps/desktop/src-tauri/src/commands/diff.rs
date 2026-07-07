use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::{
    core::error::{AppResult, CommandError},
    git::{self, GitError},
    storage::{
        ArtifactRepository, ManagedStorage, NewArtifact, NewArtifactFile, StorageError, TaskRecord,
        TaskRepository,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateTaskDiffRequest {
    pub task_id: String,
    pub base_ref: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedTaskDiff {
    pub task_id: String,
    pub base_ref: String,
    pub worktree_path: String,
    pub branch_name: String,
    pub artifact_id: String,
    pub diff_path: String,
    pub files: Vec<git::TaskDiffFile>,
    pub additions: u64,
    pub deletions: u64,
    pub patch: String,
}

#[tauri::command]
pub fn generate_task_diff(
    storage: State<'_, ManagedStorage>,
    request: GenerateTaskDiffRequest,
) -> AppResult<GeneratedTaskDiff> {
    generate_task_diff_inner(&storage, request)
}

pub(crate) fn generate_task_diff_inner(
    storage: &ManagedStorage,
    request: GenerateTaskDiffRequest,
) -> AppResult<GeneratedTaskDiff> {
    let task_id = request.task_id.trim().to_string();
    if task_id.is_empty() {
        return Err(CommandError::new(
            "diff.taskIdRequired",
            "Task id is required to generate a diff.",
        ));
    }

    let task = load_task(storage, &task_id)?;
    let worktree_path = task.worktree_path.clone().ok_or_else(|| {
        CommandError::new(
            "diff.worktreeMissing",
            format!("Task {task_id} does not have a saved worktree path."),
        )
    })?;
    let base_ref = match request.base_ref.as_deref().map(str::trim) {
        Some(base_ref) if !base_ref.is_empty() => base_ref.to_string(),
        _ => git::current_branch(&task.repository_path).map_err(diff_git_error)?,
    };

    let diff = git::task_diff(&task.id, &worktree_path, &base_ref).map_err(diff_git_error)?;
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(&task.id)
        .map_err(storage_error)?;
    fs::write(&paths.diff_path, &diff.patch).map_err(storage_error)?;

    let artifact_id = format!("diff-{}-{}", task.id, Uuid::new_v4());
    let artifact_file_id = format!("file-{artifact_id}");
    let diff_path = paths.diff_path.to_string_lossy().to_string();
    let changed_files = serde_json::to_string(&diff.files).map_err(json_error)?;
    let diff_size = file_size(&paths.diff_path).map_err(storage_error)?;

    {
        let store = storage.store.lock().map_err(|_| storage_lock_error())?;
        let artifacts = ArtifactRepository::new(store.connection());
        artifacts
            .record_artifact(NewArtifact {
                id: &artifact_id,
                task_id: &task.id,
                changed_files: &changed_files,
                diff_path: Some(&diff_path),
                test_report_path: None,
                screenshots: "[]",
                summary: "Generated task diff",
                commit_message: "",
            })
            .map_err(storage_error)?;
        artifacts
            .record_file(NewArtifactFile {
                id: &artifact_file_id,
                task_id: &task.id,
                artifact_id: Some(&artifact_id),
                file_type: "diff",
                path: &diff_path,
                size_bytes: diff_size as i64,
                compressed: false,
                retention_class: "permanent",
                expires_at: None,
            })
            .map_err(storage_error)?;
    }

    Ok(GeneratedTaskDiff {
        task_id: diff.task_id,
        base_ref: diff.base_ref,
        worktree_path: diff.worktree_path,
        branch_name: diff.branch_name,
        artifact_id,
        diff_path,
        files: diff.files,
        additions: diff.additions,
        deletions: diff.deletions,
        patch: diff.patch,
    })
}

fn load_task(storage: &ManagedStorage, task_id: &str) -> AppResult<TaskRecord> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    TaskRepository::new(store.connection())
        .get_required(task_id)
        .map_err(storage_error)
}

fn file_size(path: &Path) -> std::io::Result<u64> {
    Ok(fs::metadata(path)?.len())
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

fn diff_git_error(error: GitError) -> CommandError {
    match error {
        GitError::PathNotFound(path) => CommandError::new(
            "diff.pathNotFound",
            format!("Path does not exist: {}", path.to_string_lossy()),
        ),
        GitError::PathNotDirectory(path) => CommandError::new(
            "diff.pathNotDirectory",
            format!("Path is not a directory: {}", path.to_string_lossy()),
        ),
        GitError::Io(error) => {
            CommandError::new("diff.filesystemError", format!("Filesystem error: {error}"))
        }
        GitError::GitUnavailable(message) => CommandError::new(
            "diff.gitUnavailable",
            format!("Git is not available on this machine: {message}"),
        ),
        GitError::NotRepository { path, stderr } => CommandError::new(
            "diff.notGitRepository",
            format!(
                "Path is not a Git repository: {}{}",
                path.to_string_lossy(),
                format_stderr(&stderr)
            ),
        ),
        GitError::CommandFailed { path, args, stderr } => CommandError::new(
            "diff.gitCommandFailed",
            format!(
                "Git command failed in {}: git {}{}",
                path.to_string_lossy(),
                args,
                format_stderr(&stderr)
            ),
        ),
        GitError::InvalidTaskId(task_id) => CommandError::new(
            "diff.invalidTaskId",
            format!("Task id cannot produce a valid branch or worktree directory: {task_id}"),
        ),
        GitError::WorktreePathExists(path) => CommandError::new(
            "diff.worktreePathAlreadyExists",
            format!("Worktree path already exists: {}", path.to_string_lossy()),
        ),
        GitError::EmptyCommitMessage => CommandError::new(
            "diff.mergeCommitMessageRequired",
            "Merge commit message is required.",
        ),
        GitError::DirtyTarget(path) => CommandError::new(
            "diff.mergeTargetDirty",
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
            "diff.mergeTargetBranchChanged",
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
        "diff.invalidJson",
        format!("Unable to encode changed files for artifact storage: {error}"),
    )
}
