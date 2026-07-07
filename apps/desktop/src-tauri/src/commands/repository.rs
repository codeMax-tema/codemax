use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, FilePath};

use crate::{
    core::error::{AppResult, CommandError},
    git::{self, GitError},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryPathSelection {
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryBranchInfo {
    pub branch: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDirtyStatus {
    pub dirty: bool,
}

#[tauri::command]
pub async fn select_repository_path(app: AppHandle) -> AppResult<Option<RepositoryPathSelection>> {
    let selected_path = app
        .dialog()
        .file()
        .set_title("Select repository folder")
        .blocking_pick_folder();

    let Some(selected_path) = selected_path else {
        return Ok(None);
    };

    let path = file_path_to_path_buf(selected_path)?;
    repository_path_selection(&path).map(Some)
}

#[tauri::command]
pub fn validate_repository_path(path: String) -> AppResult<git::RepositoryInfo> {
    git::validate_repository(path).map_err(repository_error)
}

#[tauri::command]
pub fn get_repository_current_branch(path: String) -> AppResult<RepositoryBranchInfo> {
    let branch = git::current_branch(path).map_err(repository_error)?;

    Ok(RepositoryBranchInfo { branch })
}

#[tauri::command]
pub fn get_repository_dirty_status(path: String) -> AppResult<RepositoryDirtyStatus> {
    let dirty = git::has_uncommitted_changes(path).map_err(repository_error)?;

    Ok(RepositoryDirtyStatus { dirty })
}

fn file_path_to_path_buf(path: FilePath) -> AppResult<PathBuf> {
    path.simplified().into_path().map_err(|error| {
        CommandError::new(
            "repository.unsupportedPath",
            format!("Selected repository path is not a local filesystem path: {error}"),
        )
    })
}

fn repository_error(error: GitError) -> CommandError {
    match error {
        GitError::PathNotFound(path) => CommandError::new(
            "repository.pathNotFound",
            format!("Repository path does not exist: {}", path.to_string_lossy()),
        ),
        GitError::PathNotDirectory(path) => CommandError::new(
            "repository.pathNotDirectory",
            format!(
                "Repository path is not a directory: {}",
                path.to_string_lossy()
            ),
        ),
        GitError::Io(error) => CommandError::new(
            "repository.filesystemError",
            format!("Filesystem error while reading repository path: {error}"),
        ),
        GitError::GitUnavailable(message) => CommandError::new(
            "repository.gitUnavailable",
            format!("Git is not available on this machine: {message}"),
        ),
        GitError::NotRepository { path, stderr } => CommandError::new(
            "repository.notGitRepository",
            format!(
                "Selected path is not a Git repository: {}{}",
                path.to_string_lossy(),
                format_stderr(&stderr)
            ),
        ),
        GitError::CommandFailed { path, args, stderr } => CommandError::new(
            "repository.gitCommandFailed",
            format!(
                "Git command failed in {}: git {}{}",
                path.to_string_lossy(),
                args,
                format_stderr(&stderr)
            ),
        ),
        GitError::InvalidTaskId(task_id) => CommandError::new(
            "repository.invalidTaskId",
            format!("Task id cannot produce a valid Git branch or worktree path: {task_id}"),
        ),
        GitError::WorktreePathExists(path) => CommandError::new(
            "repository.worktreePathExists",
            format!("Worktree path already exists: {}", path.to_string_lossy()),
        ),
        GitError::EmptyCommitMessage => CommandError::new(
            "repository.mergeCommitMessageRequired",
            "Merge commit message is required.",
        ),
        GitError::DirtyTarget(path) => CommandError::new(
            "repository.mergeTargetDirty",
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
            "repository.mergeTargetBranchChanged",
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

fn repository_path_selection(path: &Path) -> AppResult<RepositoryPathSelection> {
    if !path.exists() {
        return Err(CommandError::new(
            "repository.pathNotFound",
            "Selected repository path does not exist.",
        ));
    }

    if !path.is_dir() {
        return Err(CommandError::new(
            "repository.pathNotDirectory",
            "Selected repository path is not a directory.",
        ));
    }

    Ok(RepositoryPathSelection {
        path: path.to_string_lossy().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_path_selection_accepts_existing_directory() {
        let selection =
            repository_path_selection(&std::env::temp_dir()).expect("temp directory is selectable");

        assert!(!selection.path.is_empty());
    }

    #[test]
    fn repository_path_selection_rejects_missing_directory() {
        let missing_path =
            std::env::temp_dir().join(format!("codemax-missing-{}", uuid::Uuid::new_v4()));

        let error = repository_path_selection(&missing_path).expect_err("missing path should fail");

        assert_eq!(error.code, "repository.pathNotFound");
    }

    #[test]
    fn repository_error_maps_non_git_directory_to_clear_error_code() {
        let error = repository_error(GitError::NotRepository {
            path: PathBuf::from("D:/not-a-repo"),
            stderr: "fatal: not a git repository".to_string(),
        });

        assert_eq!(error.code, "repository.notGitRepository");
        assert!(error.message.contains("not a Git repository"));
    }
}
