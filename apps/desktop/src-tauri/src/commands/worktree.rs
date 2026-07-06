use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::{
    core::error::{AppResult, CommandError},
    git::{self, GitError},
    storage::{ManagedStorage, StorageError, TaskRecord, TaskRepository},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCleanupResult {
    pub task_id: String,
    pub worktree_path: String,
    pub removed: bool,
}

#[tauri::command]
pub fn create_task_branch(repository_path: String, task_id: String) -> AppResult<git::TaskBranch> {
    git::create_task_branch(repository_path, &task_id).map_err(worktree_git_error)
}

#[tauri::command]
pub fn create_task_worktree(
    storage: State<'_, ManagedStorage>,
    task_id: String,
) -> AppResult<git::TaskWorktree> {
    let task = load_task(&storage, &task_id)?;
    let worktree_root = storage.roots.worktree_root.clone();

    let worktree = git::create_task_worktree(&task.repository_path, worktree_root, &task.id)
        .map_err(worktree_git_error)?;

    if let Err(error) = persist_worktree_metadata(&storage, &task.id, &worktree) {
        rollback_created_worktree(&task.repository_path, &worktree, error)?;
    }

    Ok(worktree)
}

#[tauri::command]
pub fn get_task_worktree_status(
    storage: State<'_, ManagedStorage>,
    task_id: String,
) -> AppResult<git::WorktreeStatus> {
    let task = load_task(&storage, &task_id)?;
    let worktree_path = task.worktree_path.ok_or_else(|| {
        CommandError::new(
            "worktree.metadataMissing",
            format!("Task {task_id} does not have a saved worktree path."),
        )
    })?;

    git::worktree_status(&task.id, worktree_path).map_err(worktree_git_error)
}

#[tauri::command]
pub fn cleanup_task_worktree(
    storage: State<'_, ManagedStorage>,
    task_id: String,
    confirmed: bool,
) -> AppResult<WorktreeCleanupResult> {
    require_cleanup_confirmation(confirmed)?;

    let task = load_task(&storage, &task_id)?;
    let worktree_path = task.worktree_path.clone().ok_or_else(|| {
        CommandError::new(
            "worktree.metadataMissing",
            format!("Task {task_id} does not have a saved worktree path."),
        )
    })?;
    let worktree = PathBuf::from(&worktree_path);

    if !worktree.exists() {
        clear_worktree_metadata(&storage, &task.id)?;
        return Ok(WorktreeCleanupResult {
            task_id,
            worktree_path,
            removed: false,
        });
    }

    git::remove_task_worktree(&task.repository_path, &worktree).map_err(worktree_git_error)?;
    clear_worktree_metadata(&storage, &task.id)?;

    Ok(WorktreeCleanupResult {
        task_id,
        worktree_path,
        removed: true,
    })
}

fn require_cleanup_confirmation(confirmed: bool) -> AppResult<()> {
    if confirmed {
        return Ok(());
    }

    Err(CommandError::new(
        "worktree.cleanupNotConfirmed",
        "Worktree cleanup requires explicit user confirmation.",
    ))
}

fn load_task(storage: &ManagedStorage, task_id: &str) -> AppResult<TaskRecord> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    TaskRepository::new(store.connection())
        .get_required(task_id)
        .map_err(storage_error)
}

fn persist_worktree_metadata(
    storage: &ManagedStorage,
    task_id: &str,
    worktree: &git::TaskWorktree,
) -> AppResult<()> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    TaskRepository::new(store.connection())
        .update_worktree_metadata(task_id, &worktree.worktree_path, &worktree.branch_name)
        .map_err(storage_error)?;
    Ok(())
}

fn clear_worktree_metadata(storage: &ManagedStorage, task_id: &str) -> AppResult<()> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    TaskRepository::new(store.connection())
        .clear_worktree_metadata(task_id)
        .map_err(storage_error)?;
    Ok(())
}

fn rollback_created_worktree(
    repository_path: &str,
    worktree: &git::TaskWorktree,
    original_error: CommandError,
) -> AppResult<()> {
    let worktree_path = PathBuf::from(&worktree.worktree_path);
    match git::remove_task_worktree(repository_path, &worktree_path) {
        Ok(()) => Err(original_error),
        Err(rollback_error) => {
            let rollback_error = worktree_git_error(rollback_error);
            Err(CommandError::new(
                "worktree.metadataPersistFailed",
                format!(
                    "{} Worktree rollback also failed: {}",
                    original_error.message, rollback_error.message
                ),
            ))
        }
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

fn worktree_git_error(error: GitError) -> CommandError {
    match error {
        GitError::PathNotFound(path) => CommandError::new(
            "worktree.pathNotFound",
            format!("Path does not exist: {}", path.to_string_lossy()),
        ),
        GitError::PathNotDirectory(path) => CommandError::new(
            "worktree.pathNotDirectory",
            format!("Path is not a directory: {}", path.to_string_lossy()),
        ),
        GitError::Io(error) => CommandError::new(
            "worktree.filesystemError",
            format!("Filesystem error: {error}"),
        ),
        GitError::GitUnavailable(message) => CommandError::new(
            "worktree.gitUnavailable",
            format!("Git is not available on this machine: {message}"),
        ),
        GitError::NotRepository { path, stderr } => CommandError::new(
            "worktree.notGitRepository",
            format!(
                "Path is not a Git repository: {}{}",
                path.to_string_lossy(),
                format_stderr(&stderr)
            ),
        ),
        GitError::CommandFailed { path, args, stderr } => CommandError::new(
            "worktree.gitCommandFailed",
            format!(
                "Git command failed in {}: git {}{}",
                path.to_string_lossy(),
                args,
                format_stderr(&stderr)
            ),
        ),
        GitError::InvalidTaskId(task_id) => CommandError::new(
            "worktree.invalidTaskId",
            format!("Task id cannot produce a valid branch or worktree directory: {task_id}"),
        ),
        GitError::WorktreePathExists(path) => CommandError::new(
            "worktree.pathAlreadyExists",
            format!("Worktree path already exists: {}", path.to_string_lossy()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path, process::Command};

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("codemax-{label}-{}", uuid::Uuid::new_v4()))
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
        fs::write(path.join("fixture.txt"), "before").expect("write fixture");
        run_test_git(&path, &["add", "."]);
        run_test_git(&path, &["commit", "-m", "initial fixture"]);

        path
    }

    #[test]
    fn cleanup_requires_explicit_confirmation() {
        let error = require_cleanup_confirmation(false).expect_err("confirmation is required");

        assert_eq!(error.code, "worktree.cleanupNotConfirmed");
        require_cleanup_confirmation(true).expect("confirmed cleanup is allowed");
    }

    #[test]
    fn git_error_maps_existing_worktree_path_to_clear_code() {
        let error = worktree_git_error(GitError::WorktreePathExists(PathBuf::from(
            "D:/codemax/app-data/worktrees/task-001",
        )));

        assert_eq!(error.code, "worktree.pathAlreadyExists");
    }

    #[test]
    fn rollback_created_worktree_removes_directory_and_preserves_original_error() {
        let repository = committed_repository("worktree-rollback");
        let worktree_root = temp_path("worktree-rollback-root");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-rollback")
            .expect("create task worktree");
        let worktree_path = PathBuf::from(&worktree.worktree_path);

        let error = rollback_created_worktree(
            repository.to_string_lossy().as_ref(),
            &worktree,
            CommandError::new("storage.sqliteError", "metadata save failed"),
        )
        .expect_err("rollback returns the original persistence error");

        assert_eq!(error.code, "storage.sqliteError");
        assert!(!worktree_path.exists());

        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean temp repository");
    }
}
