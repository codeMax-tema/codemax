use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryInfo {
    pub path: String,
    pub name: String,
    pub branch: String,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub path: String,
    pub name: String,
    pub is_git_repository: bool,
    pub branch: Option<String>,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskBranch {
    pub task_id: String,
    pub branch_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskWorktree {
    pub task_id: String,
    pub repository_path: String,
    pub worktree_path: String,
    pub branch_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeStatus {
    pub task_id: String,
    pub worktree_path: String,
    pub branch_name: String,
    pub dirty: bool,
    pub changes: Vec<WorktreeFileChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeFileChange {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDiff {
    pub task_id: String,
    pub base_ref: String,
    pub worktree_path: String,
    pub branch_name: String,
    pub patch: String,
    pub files: Vec<TaskDiffFile>,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDiffFile {
    pub path: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub patch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskMergeStatus {
    Merged,
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMergeResult {
    pub status: TaskMergeStatus,
    pub target_branch: String,
    pub source_branch: String,
    pub commit_sha: String,
    pub commit_message: String,
    pub conflict_files: Vec<String>,
    pub error_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterruptedMergeOutcome {
    NoTargetChange,
    Merged { commit_sha: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeBaseline {
    pub repository_root: String,
    pub repository_common_dir: String,
    pub worktree_path: String,
    pub target_branch: String,
    pub source_branch: String,
    pub target_head: String,
    pub source_head: String,
    pub diff_digest: String,
    pub diff_file_count: usize,
    pub target_dirty: bool,
    pub worktree_dirty: bool,
}

impl MergeBaseline {
    pub fn changed_fields(&self, current: &Self) -> Vec<String> {
        let mut fields = Vec::new();
        if self.repository_root != current.repository_root {
            fields.push("repository".to_string());
        }
        if self.repository_common_dir != current.repository_common_dir {
            fields.push("repositoryIdentity".to_string());
        }
        if self.worktree_path != current.worktree_path {
            fields.push("worktreePath".to_string());
        }
        if self.target_branch != current.target_branch {
            fields.push("targetBranch".to_string());
        }
        if self.source_branch != current.source_branch {
            fields.push("taskBranch".to_string());
        }
        if self.target_head != current.target_head {
            fields.push("targetHead".to_string());
        }
        if self.source_head != current.source_head {
            fields.push("taskHead".to_string());
        }
        if self.diff_digest != current.diff_digest
            || self.diff_file_count != current.diff_file_count
        {
            fields.push("diff".to_string());
        }
        if self.target_dirty != current.target_dirty {
            fields.push("targetWorktree".to_string());
        }
        if self.worktree_dirty != current.worktree_dirty {
            fields.push("taskWorktree".to_string());
        }
        fields
    }
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error("path does not exist: {0}")]
    PathNotFound(PathBuf),
    #[error("path is not a directory: {0}")]
    PathNotDirectory(PathBuf),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("git executable is unavailable: {0}")]
    GitUnavailable(String),
    #[error("path is not a git repository: {path}; {stderr}")]
    NotRepository { path: PathBuf, stderr: String },
    #[error("git command failed in {path}: git {args}; {stderr}")]
    CommandFailed {
        path: PathBuf,
        args: String,
        stderr: String,
    },
    #[error("task id cannot produce a valid git branch or worktree directory: {0}")]
    InvalidTaskId(String),
    #[error("worktree path already exists: {0}")]
    WorktreePathExists(PathBuf),
    #[error("merge commit message is required")]
    EmptyCommitMessage,
    #[error("target repository has uncommitted changes: {0}")]
    DirtyTarget(PathBuf),
    #[error("target branch changed in {path}: expected {expected}, got {actual}")]
    TargetBranchChanged {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    #[error("task worktree belongs to a different Git repository: {worktree}")]
    WorktreeRepositoryMismatch { worktree: PathBuf },
    #[error("merge preview baseline changed: {changed_fields:?}")]
    MergeBaselineChanged { changed_fields: Vec<String> },
    #[error("task worktree changed while CodeMax was preparing the merge: {0}")]
    WorktreeChangedDuringMerge(PathBuf),
    #[error("merge result verification failed: {0}")]
    MergeVerificationFailed(String),
}

pub type GitResult<T> = Result<T, GitError>;

pub fn inspect_project(path: impl AsRef<Path>) -> GitResult<ProjectInfo> {
    let selected_path = path.as_ref();
    ensure_directory(selected_path)?;
    let selected_path = selected_path
        .canonicalize()
        .unwrap_or_else(|_| selected_path.to_path_buf());
    let is_git_repository = is_inside_git_work_tree(&selected_path)?;
    let project_path = if is_git_repository {
        repository_root(&selected_path)?
    } else {
        selected_path
    };

    let (branch, dirty) = if is_git_repository {
        (
            Some(current_branch_in_repository(&project_path)?),
            is_dirty(&project_path)?,
        )
    } else {
        (None, false)
    };

    Ok(ProjectInfo {
        name: repository_name(&project_path),
        path: project_path.to_string_lossy().to_string(),
        is_git_repository,
        branch,
        dirty,
    })
}

pub fn initialize_repository_with_baseline(path: impl AsRef<Path>) -> GitResult<ProjectInfo> {
    let selected_path = path.as_ref();
    ensure_directory(selected_path)?;
    let selected_path = selected_path
        .canonicalize()
        .unwrap_or_else(|_| selected_path.to_path_buf());
    let git_metadata = selected_path.join(".git");
    if git_metadata.exists() {
        return Err(GitError::CommandFailed {
            path: selected_path,
            args: "initialize repository".to_string(),
            stderr: ".git already exists".to_string(),
        });
    }

    let initialize_result = run_required_git(&selected_path, &["init"]);
    if let Err(error) = initialize_result {
        let _ = remove_initialized_repository_metadata(&selected_path);
        return Err(error);
    }

    let baseline_result = (|| {
        run_required_git(&selected_path, &["add", "-A"])?;
        run_required_git(
            &selected_path,
            &[
                "-c",
                "user.name=CodeMax",
                "-c",
                "user.email=codemax@localhost",
                "commit",
                "--allow-empty",
                "-m",
                "chore: create CodeMax baseline",
            ],
        )?;
        inspect_project(&selected_path)
    })();

    if baseline_result.is_err() {
        let _ = remove_initialized_repository_metadata(&selected_path);
    }
    baseline_result
}

pub fn remove_initialized_repository_metadata(path: impl AsRef<Path>) -> GitResult<()> {
    let selected_path = path.as_ref();
    ensure_directory(selected_path)?;
    let git_metadata = selected_path.join(".git");
    let metadata = match fs::symlink_metadata(&git_metadata) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(GitError::Io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(GitError::CommandFailed {
            path: selected_path.to_path_buf(),
            args: "remove initialized Git metadata".to_string(),
            stderr: ".git is not a regular directory".to_string(),
        });
    }
    fs::remove_dir_all(git_metadata)?;
    Ok(())
}

pub fn validate_repository(path: impl AsRef<Path>) -> GitResult<RepositoryInfo> {
    let root = repository_root(path.as_ref())?;
    let branch = current_branch_in_repository(&root)?;
    let dirty = is_dirty(&root)?;

    Ok(RepositoryInfo {
        name: repository_name(&root),
        path: root.to_string_lossy().to_string(),
        branch,
        dirty,
    })
}

pub fn current_branch(path: impl AsRef<Path>) -> GitResult<String> {
    let root = repository_root(path.as_ref())?;
    current_branch_in_repository(&root)
}

pub fn has_uncommitted_changes(path: impl AsRef<Path>) -> GitResult<bool> {
    let root = repository_root(path.as_ref())?;
    is_dirty(&root)
}

pub fn task_branch_name(task_id: &str) -> GitResult<String> {
    Ok(format!("agent/{}", task_identity(task_id)?))
}

pub fn task_worktree_path(worktree_root: impl AsRef<Path>, task_id: &str) -> GitResult<PathBuf> {
    Ok(worktree_root.as_ref().join(task_identity(task_id)?))
}

pub fn create_task_branch(
    repository_path: impl AsRef<Path>,
    task_id: &str,
) -> GitResult<TaskBranch> {
    let root = repository_root(repository_path.as_ref())?;
    let branch_name = task_branch_name(task_id)?;

    if branch_exists(&root, &branch_name)? {
        return Ok(TaskBranch {
            task_id: task_id.to_string(),
            branch_name,
        });
    }

    let branch_output = run_git(&root, &["branch", branch_name.as_str()])?;
    if !branch_output.status_success {
        return Err(GitError::CommandFailed {
            path: root,
            args: format!("branch {branch_name}"),
            stderr: branch_output.stderr,
        });
    }

    Ok(TaskBranch {
        task_id: task_id.to_string(),
        branch_name,
    })
}

pub fn create_task_worktree(
    repository_path: impl AsRef<Path>,
    worktree_root: impl AsRef<Path>,
    task_id: &str,
) -> GitResult<TaskWorktree> {
    let repository_root = repository_root(repository_path.as_ref())?;
    let worktree_root = worktree_root.as_ref();
    fs::create_dir_all(worktree_root)?;

    let worktree_path = task_worktree_path(worktree_root, task_id)?;
    if worktree_path.exists() {
        return Err(GitError::WorktreePathExists(worktree_path));
    }

    let branch_name = task_branch_name(task_id)?;
    let worktree_path_arg = worktree_path.to_string_lossy().to_string();
    let branch_already_exists = branch_exists(&repository_root, &branch_name)?;
    let output = if branch_already_exists {
        run_git(
            &repository_root,
            &[
                "worktree",
                "add",
                worktree_path_arg.as_str(),
                branch_name.as_str(),
            ],
        )?
    } else {
        run_git(
            &repository_root,
            &[
                "worktree",
                "add",
                "-b",
                branch_name.as_str(),
                worktree_path_arg.as_str(),
            ],
        )?
    };

    if !output.status_success {
        let args = if branch_already_exists {
            format!("worktree add {worktree_path_arg} {branch_name}")
        } else {
            format!("worktree add -b {branch_name} {worktree_path_arg}")
        };
        return Err(GitError::CommandFailed {
            path: repository_root,
            args,
            stderr: output.stderr,
        });
    }

    Ok(TaskWorktree {
        task_id: task_id.to_string(),
        repository_path: repository_root.to_string_lossy().to_string(),
        worktree_path: worktree_path.to_string_lossy().to_string(),
        branch_name,
    })
}

pub fn worktree_status(
    task_id: &str,
    worktree_path: impl AsRef<Path>,
) -> GitResult<WorktreeStatus> {
    let worktree_root = repository_root(worktree_path.as_ref())?;
    let branch_name = current_branch_in_repository(&worktree_root)?;
    let changes = worktree_changes(&worktree_root)?;

    Ok(WorktreeStatus {
        task_id: task_id.to_string(),
        worktree_path: worktree_root.to_string_lossy().to_string(),
        branch_name,
        dirty: !changes.is_empty(),
        changes,
    })
}

pub fn task_diff(
    task_id: &str,
    worktree_path: impl AsRef<Path>,
    base_ref: &str,
) -> GitResult<TaskDiff> {
    let worktree_root = repository_root(worktree_path.as_ref())?;
    let base_ref = base_ref.trim();
    let branch_name = current_branch_in_repository(&worktree_root)?;
    let changes = worktree_changes(&worktree_root)?;
    let status_by_path = changes
        .iter()
        .map(|change| (change.path.clone(), change.status.clone()))
        .collect::<HashMap<_, _>>();
    let mut stat_by_path = diff_numstat(&worktree_root, base_ref)?;
    let tracked_patch = diff_patch(&worktree_root, base_ref)?;
    let untracked_patches = untracked_file_patches(&worktree_root, &mut stat_by_path)?;
    let patch = join_patch_sections(tracked_patch, untracked_patches);
    let files = split_patch_by_file(&patch, &status_by_path, &stat_by_path);
    let additions = files.iter().map(|file| file.additions).sum();
    let deletions = files.iter().map(|file| file.deletions).sum();

    Ok(TaskDiff {
        task_id: task_id.to_string(),
        base_ref: base_ref.to_string(),
        worktree_path: worktree_root.to_string_lossy().to_string(),
        branch_name,
        patch,
        files,
        additions,
        deletions,
    })
}

pub fn capture_merge_baseline(
    task_id: &str,
    repository_path: impl AsRef<Path>,
    worktree_path: impl AsRef<Path>,
    target_branch: &str,
) -> GitResult<(MergeBaseline, TaskDiff)> {
    let target_root = repository_root(repository_path.as_ref())?;
    let worktree_root = repository_root(worktree_path.as_ref())?;
    let target_branch = target_branch.trim();
    let current_target_branch = current_branch_in_repository(&target_root)?;
    if current_target_branch != target_branch {
        return Err(GitError::TargetBranchChanged {
            path: target_root,
            expected: target_branch.to_string(),
            actual: current_target_branch,
        });
    }

    let repository_common_dir = git_common_dir(&target_root)?;
    let worktree_common_dir = git_common_dir(&worktree_root)?;
    if repository_common_dir != worktree_common_dir {
        return Err(GitError::WorktreeRepositoryMismatch {
            worktree: worktree_root,
        });
    }

    let source_branch = current_branch_in_repository(&worktree_root)?;
    let target_head = head_oid(&target_root)?;
    let source_head = head_oid(&worktree_root)?;
    let target_dirty = is_dirty(&target_root)?;
    let worktree_dirty = is_dirty(&worktree_root)?;
    let diff = task_diff(task_id, &worktree_root, target_branch)?;
    let diff_digest = task_diff_digest(&diff);

    Ok((
        MergeBaseline {
            repository_root: target_root.to_string_lossy().to_string(),
            repository_common_dir: repository_common_dir.to_string_lossy().to_string(),
            worktree_path: worktree_root.to_string_lossy().to_string(),
            target_branch: target_branch.to_string(),
            source_branch,
            target_head,
            source_head,
            diff_digest,
            diff_file_count: diff.files.len(),
            target_dirty,
            worktree_dirty,
        },
        diff,
    ))
}

pub fn merge_task_branch(
    task_id: &str,
    repository_path: impl AsRef<Path>,
    worktree_path: impl AsRef<Path>,
    target_branch: &str,
    commit_message: &str,
    expected_baseline: &MergeBaseline,
) -> GitResult<TaskMergeResult> {
    let target_root = repository_root(repository_path.as_ref())?;
    let worktree_root = repository_root(worktree_path.as_ref())?;
    let target_branch = target_branch.trim();
    let commit_message = commit_message.trim();

    if commit_message.is_empty() {
        return Err(GitError::EmptyCommitMessage);
    }

    let (current_baseline, _) =
        capture_merge_baseline(task_id, &target_root, &worktree_root, target_branch)?;
    let changed_fields = expected_baseline.changed_fields(&current_baseline);
    if !changed_fields.is_empty() {
        return Err(GitError::MergeBaselineChanged { changed_fields });
    }
    if current_baseline.target_dirty {
        return Err(GitError::DirtyTarget(target_root));
    }

    let source_branch = current_baseline.source_branch.clone();
    commit_worktree_changes(&worktree_root, commit_message)?;
    if is_dirty(&worktree_root)? {
        return Err(GitError::WorktreeChangedDuringMerge(worktree_root));
    }
    let committed_diff = task_diff(task_id, &worktree_root, target_branch)?;
    if task_diff_digest(&committed_diff) != expected_baseline.diff_digest
        || committed_diff.files.len() != expected_baseline.diff_file_count
    {
        // A concurrent edit may have been safely committed to the task branch, but it was not part
        // of the user's confirmed preview. The target remains untouched and requires a new preview.
        return Err(GitError::WorktreeChangedDuringMerge(worktree_root));
    }

    // The source commit may legitimately change because the previewed task changes are committed.
    // Recheck the target immediately before the merge so an external checkout, commit, or edit
    // cannot silently reuse the earlier confirmation.
    let current_target_branch = current_branch_in_repository(&target_root)?;
    if current_target_branch != expected_baseline.target_branch {
        return Err(GitError::TargetBranchChanged {
            path: target_root,
            expected: expected_baseline.target_branch.clone(),
            actual: current_target_branch,
        });
    }
    if head_oid(&target_root)? != expected_baseline.target_head {
        return Err(GitError::MergeBaselineChanged {
            changed_fields: vec!["targetHead".to_string()],
        });
    }
    if is_dirty(&target_root)? {
        return Err(GitError::DirtyTarget(target_root));
    }
    if current_branch_in_repository(&worktree_root)? != source_branch {
        return Err(GitError::MergeBaselineChanged {
            changed_fields: vec!["taskBranch".to_string()],
        });
    }

    let source_head = head_oid(&worktree_root)?;
    if is_ancestor(&target_root, &source_head, &expected_baseline.target_head)? {
        return Ok(TaskMergeResult {
            status: TaskMergeStatus::Merged,
            target_branch: target_branch.to_string(),
            source_branch,
            commit_sha: expected_baseline.target_head.clone(),
            commit_message: commit_message.to_string(),
            conflict_files: Vec::new(),
            error_reason: None,
        });
    }

    let merge_output = run_git(
        &target_root,
        &[
            "merge",
            "--no-ff",
            source_branch.as_str(),
            "-m",
            commit_message,
        ],
    )?;

    if merge_output.status_success {
        let commit_sha = head_oid(&target_root)?;
        if is_dirty(&target_root)? || !is_ancestor(&target_root, &source_head, &commit_sha)? {
            return Err(GitError::MergeVerificationFailed(
                "Git returned success, but the target is dirty or does not contain the task commit."
                    .to_string(),
            ));
        }
        return Ok(TaskMergeResult {
            status: TaskMergeStatus::Merged,
            target_branch: target_branch.to_string(),
            source_branch,
            commit_sha,
            commit_message: commit_message.to_string(),
            conflict_files: Vec::new(),
            error_reason: None,
        });
    }

    let conflict_files = merge_conflict_files(&target_root)?;
    let merge_was_started = merge_in_progress(&target_root)?;
    if merge_was_started {
        abort_merge(&target_root)?;
    }

    if head_oid(&target_root)? != expected_baseline.target_head || is_dirty(&target_root)? {
        return Err(GitError::MergeVerificationFailed(
            "The failed merge could not be restored to its clean pre-merge target baseline."
                .to_string(),
        ));
    }

    if conflict_files.is_empty() {
        return Err(GitError::CommandFailed {
            path: target_root,
            args: format!("merge --no-ff {source_branch} -m <message>"),
            stderr: merge_output.stderr,
        });
    }

    let error_reason = git_output_summary(&merge_output)
        .filter(|message| !message.is_empty())
        .or_else(|| Some("Git reported merge conflicts.".to_string()));

    Ok(TaskMergeResult {
        status: TaskMergeStatus::Conflicted,
        target_branch: target_branch.to_string(),
        source_branch,
        commit_sha: String::new(),
        commit_message: commit_message.to_string(),
        conflict_files,
        error_reason,
    })
}

pub fn inspect_interrupted_merge_outcome(
    baseline: &MergeBaseline,
    commit_message: &str,
) -> GitResult<InterruptedMergeOutcome> {
    let target_root = repository_root(Path::new(&baseline.repository_root))?;
    let expected_common_dir = PathBuf::from(&baseline.repository_common_dir)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&baseline.repository_common_dir));
    if git_common_dir(&target_root)? != expected_common_dir {
        return Err(GitError::MergeVerificationFailed(
            "The interrupted merge no longer points at the same repository.".to_string(),
        ));
    }
    let branch = current_branch_in_repository(&target_root)?;
    if branch != baseline.target_branch {
        return Err(GitError::TargetBranchChanged {
            path: target_root,
            expected: baseline.target_branch.clone(),
            actual: branch,
        });
    }
    if merge_in_progress(&target_root)? || is_dirty(&target_root)? {
        return Err(GitError::MergeVerificationFailed(
            "The interrupted merge left an active or dirty target state; automatic recovery would risk overwriting user changes.".to_string(),
        ));
    }

    let current_head = head_oid(&target_root)?;
    if current_head == baseline.target_head {
        return Ok(InterruptedMergeOutcome::NoTargetChange);
    }

    let source_head = rev_parse_oid(&target_root, &baseline.source_branch)?;
    let parents_output = run_git(&target_root, &["rev-list", "--parents", "-n", "1", "HEAD"])?;
    if !parents_output.status_success {
        return Err(GitError::CommandFailed {
            path: target_root,
            args: "rev-list --parents -n 1 HEAD".to_string(),
            stderr: parents_output.stderr,
        });
    }
    let commit_parts = parents_output.stdout.split_whitespace().collect::<Vec<_>>();
    let message_output = run_git(&target_root, &["log", "-1", "--format=%B"])?;
    if commit_parts.len() == 3
        && commit_parts[0] == current_head
        && commit_parts[1] == baseline.target_head
        && commit_parts[2] == source_head
        && message_output.status_success
        && message_output.stdout.trim() == commit_message.trim()
    {
        return Ok(InterruptedMergeOutcome::Merged {
            commit_sha: current_head,
        });
    }

    Err(GitError::MergeVerificationFailed(
        "The target changed after the interrupted attempt, but it is not the exact verified merge commit owned by that attempt.".to_string(),
    ))
}

pub fn verify_persisted_merge_outcome(
    baseline: &MergeBaseline,
    status: &TaskMergeStatus,
    commit_sha: &str,
) -> GitResult<()> {
    let target_root = repository_root(Path::new(&baseline.repository_root))?;
    let expected_common_dir = PathBuf::from(&baseline.repository_common_dir)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&baseline.repository_common_dir));
    if git_common_dir(&target_root)? != expected_common_dir {
        return Err(GitError::MergeVerificationFailed(
            "The persisted merge record no longer points at the same repository.".to_string(),
        ));
    }
    let branch = current_branch_in_repository(&target_root)?;
    if branch != baseline.target_branch {
        return Err(GitError::TargetBranchChanged {
            path: target_root,
            expected: baseline.target_branch.clone(),
            actual: branch,
        });
    }
    if is_dirty(&target_root)? {
        return Err(GitError::MergeVerificationFailed(
            "The target repository is dirty, so an interrupted merge outcome cannot be reconciled automatically."
                .to_string(),
        ));
    }

    let current_head = head_oid(&target_root)?;
    match status {
        TaskMergeStatus::Merged => {
            if commit_sha.is_empty() || current_head != commit_sha {
                return Err(GitError::MergeVerificationFailed(
                    "The target HEAD does not match the persisted successful merge commit."
                        .to_string(),
                ));
            }
            let source_head = rev_parse_oid(&target_root, &baseline.source_branch)?;
            if !is_ancestor(&target_root, &source_head, &current_head)? {
                return Err(GitError::MergeVerificationFailed(
                    "The persisted target commit does not contain the current task branch commit."
                        .to_string(),
                ));
            }
        }
        TaskMergeStatus::Conflicted => {
            if !commit_sha.is_empty()
                || current_head != baseline.target_head
                || merge_in_progress(&target_root)?
            {
                return Err(GitError::MergeVerificationFailed(
                    "The conflicted merge was not restored to its clean preview baseline."
                        .to_string(),
                ));
            }
        }
    }
    Ok(())
}

pub fn remove_task_worktree(
    repository_path: impl AsRef<Path>,
    worktree_path: impl AsRef<Path>,
) -> GitResult<()> {
    let repository_root = repository_root(repository_path.as_ref())?;
    ensure_directory(worktree_path.as_ref())?;

    let worktree_path_arg = worktree_path.as_ref().to_string_lossy().to_string();
    let output = run_git(
        &repository_root,
        &["worktree", "remove", worktree_path_arg.as_str()],
    )?;
    if !output.status_success {
        return Err(GitError::CommandFailed {
            path: repository_root,
            args: format!("worktree remove {worktree_path_arg}"),
            stderr: output.stderr,
        });
    }

    Ok(())
}

pub fn delete_task_branch(
    repository_path: impl AsRef<Path>,
    task_id: &str,
    branch_name: &str,
) -> GitResult<()> {
    let repository_root = repository_root(repository_path.as_ref())?;
    let expected_branch = task_branch_name(task_id)?;
    if branch_name != expected_branch {
        return Err(GitError::CommandFailed {
            path: repository_root,
            args: "delete task branch".to_string(),
            stderr: "stored task branch does not match the task id".to_string(),
        });
    }

    if !branch_exists(&repository_root, &expected_branch)? {
        return Ok(());
    }
    let output = run_git(&repository_root, &["branch", "-D", &expected_branch])?;
    if !output.status_success {
        return Err(GitError::CommandFailed {
            path: repository_root,
            args: format!("branch -D {expected_branch}"),
            stderr: output.stderr,
        });
    }
    Ok(())
}

fn repository_root(path: &Path) -> GitResult<PathBuf> {
    ensure_directory(path)?;

    if !is_inside_git_work_tree_strict(path)? {
        return Err(GitError::NotRepository {
            path: path.to_path_buf(),
            stderr: "fatal: not a git repository".to_string(),
        });
    }

    let root_output = run_git(path, &["rev-parse", "--show-toplevel"])?;
    if !root_output.status_success {
        return Err(GitError::NotRepository {
            path: path.to_path_buf(),
            stderr: root_output.stderr,
        });
    }

    let raw_root = PathBuf::from(root_output.stdout.trim());
    Ok(raw_root.canonicalize().unwrap_or(raw_root))
}

fn is_inside_git_work_tree(path: &Path) -> GitResult<bool> {
    let inside_work_tree = match run_git(path, &["rev-parse", "--is-inside-work-tree"]) {
        Ok(output) => output,
        Err(GitError::GitUnavailable(_)) => return Ok(false),
        Err(error) => return Err(error),
    };

    Ok(inside_work_tree.status_success && inside_work_tree.stdout.trim() == "true")
}

fn is_inside_git_work_tree_strict(path: &Path) -> GitResult<bool> {
    let inside_work_tree = run_git(path, &["rev-parse", "--is-inside-work-tree"])?;
    Ok(inside_work_tree.status_success && inside_work_tree.stdout.trim() == "true")
}

fn ensure_directory(path: &Path) -> GitResult<()> {
    if !path.exists() {
        return Err(GitError::PathNotFound(path.to_path_buf()));
    }

    if !path.is_dir() {
        return Err(GitError::PathNotDirectory(path.to_path_buf()));
    }

    Ok(())
}

fn current_branch_in_repository(repository_path: &Path) -> GitResult<String> {
    let branch_output = run_git(repository_path, &["branch", "--show-current"])?;
    if !branch_output.status_success {
        return Err(GitError::CommandFailed {
            path: repository_path.to_path_buf(),
            args: "branch --show-current".to_string(),
            stderr: branch_output.stderr,
        });
    }

    let branch = branch_output.stdout.trim();
    if !branch.is_empty() {
        return Ok(branch.to_string());
    }

    let head_output = run_git(repository_path, &["rev-parse", "--short", "HEAD"])?;
    if head_output.status_success {
        let head = head_output.stdout.trim();
        if !head.is_empty() {
            return Ok(format!("HEAD ({head})"));
        }
    }

    Ok("HEAD".to_string())
}

fn is_dirty(repository_path: &Path) -> GitResult<bool> {
    let status_output = run_git(repository_path, &["status", "--porcelain"])?;
    if !status_output.status_success {
        return Err(GitError::CommandFailed {
            path: repository_path.to_path_buf(),
            args: "status --porcelain".to_string(),
            stderr: status_output.stderr,
        });
    }

    Ok(!status_output.stdout.trim().is_empty())
}

fn branch_exists(repository_path: &Path, branch_name: &str) -> GitResult<bool> {
    let ref_name = format!("refs/heads/{branch_name}");
    let output = run_git(
        repository_path,
        &["show-ref", "--verify", "--quiet", ref_name.as_str()],
    )?;

    Ok(output.status_success)
}

fn worktree_changes(worktree_path: &Path) -> GitResult<Vec<WorktreeFileChange>> {
    let status_output = run_git(worktree_path, &["status", "--porcelain=v1"])?;
    if !status_output.status_success {
        return Err(GitError::CommandFailed {
            path: worktree_path.to_path_buf(),
            args: "status --porcelain=v1".to_string(),
            stderr: status_output.stderr,
        });
    }

    Ok(status_output
        .stdout
        .lines()
        .filter_map(parse_porcelain_status_line)
        .collect())
}

fn diff_patch(worktree_path: &Path, base_ref: &str) -> GitResult<String> {
    let output = run_git(
        worktree_path,
        &["diff", "--binary", "--find-renames", base_ref, "--"],
    )?;
    if !output.status_success {
        return Err(GitError::CommandFailed {
            path: worktree_path.to_path_buf(),
            args: format!("diff --binary --find-renames {base_ref} --"),
            stderr: output.stderr,
        });
    }

    Ok(output.stdout)
}

fn diff_numstat(worktree_path: &Path, base_ref: &str) -> GitResult<HashMap<String, (u64, u64)>> {
    let output = run_git(worktree_path, &["diff", "--numstat", base_ref, "--"])?;
    if !output.status_success {
        return Err(GitError::CommandFailed {
            path: worktree_path.to_path_buf(),
            args: format!("diff --numstat {base_ref} --"),
            stderr: output.stderr,
        });
    }

    let mut stats = HashMap::new();
    for line in output.stdout.lines() {
        let mut fields = line.splitn(3, '\t');
        let additions = parse_numstat_count(fields.next());
        let deletions = parse_numstat_count(fields.next());
        let Some(path) = fields.next().map(parse_numstat_path) else {
            continue;
        };
        stats.insert(path, (additions, deletions));
    }

    Ok(stats)
}

fn commit_worktree_changes(worktree_path: &Path, commit_message: &str) -> GitResult<()> {
    if !is_dirty(worktree_path)? {
        return Ok(());
    }

    let add_output = run_git(worktree_path, &["add", "--all"])?;
    if !add_output.status_success {
        return Err(GitError::CommandFailed {
            path: worktree_path.to_path_buf(),
            args: "add --all".to_string(),
            stderr: add_output.stderr,
        });
    }

    let commit_output = run_git(worktree_path, &["commit", "-m", commit_message])?;
    if !commit_output.status_success {
        return Err(GitError::CommandFailed {
            path: worktree_path.to_path_buf(),
            args: "commit -m <message>".to_string(),
            stderr: commit_output.stderr,
        });
    }

    Ok(())
}

fn rev_parse_oid(repository_path: &Path, revision: &str) -> GitResult<String> {
    let output = run_git(repository_path, &["rev-parse", revision])?;
    if !output.status_success {
        return Err(GitError::CommandFailed {
            path: repository_path.to_path_buf(),
            args: format!("rev-parse {revision}"),
            stderr: output.stderr,
        });
    }
    Ok(output.stdout.trim().to_string())
}

fn head_oid(repository_path: &Path) -> GitResult<String> {
    let output = run_git(repository_path, &["rev-parse", "HEAD"])?;
    if !output.status_success {
        return Err(GitError::CommandFailed {
            path: repository_path.to_path_buf(),
            args: "rev-parse HEAD".to_string(),
            stderr: output.stderr,
        });
    }

    Ok(output.stdout.trim().to_string())
}

fn git_common_dir(repository_path: &Path) -> GitResult<PathBuf> {
    let output = run_git(repository_path, &["rev-parse", "--git-common-dir"])?;
    if !output.status_success {
        return Err(GitError::CommandFailed {
            path: repository_path.to_path_buf(),
            args: "rev-parse --git-common-dir".to_string(),
            stderr: output.stderr,
        });
    }

    let raw = PathBuf::from(output.stdout.trim());
    let path = if raw.is_absolute() {
        raw
    } else {
        repository_path.join(raw)
    };
    Ok(path.canonicalize().unwrap_or(path))
}

fn task_diff_digest(diff: &TaskDiff) -> String {
    let mut digest = Sha256::new();
    digest.update(diff.base_ref.as_bytes());
    digest.update([0]);
    digest.update(diff.branch_name.as_bytes());
    digest.update([0]);
    digest.update(diff.patch.as_bytes());
    for file in &diff.files {
        digest.update([0xff]);
        digest.update(file.path.as_bytes());
        digest.update([0]);
        digest.update(file.status.as_bytes());
        digest.update(file.additions.to_le_bytes());
        digest.update(file.deletions.to_le_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn is_ancestor(repository_path: &Path, ancestor: &str, descendant: &str) -> GitResult<bool> {
    let output = run_git(
        repository_path,
        &["merge-base", "--is-ancestor", ancestor, descendant],
    )?;
    Ok(output.status_success)
}

fn merge_in_progress(repository_path: &Path) -> GitResult<bool> {
    let output = run_git(
        repository_path,
        &["rev-parse", "--verify", "-q", "MERGE_HEAD"],
    )?;
    Ok(output.status_success)
}

fn merge_conflict_files(repository_path: &Path) -> GitResult<Vec<String>> {
    let output = run_git(repository_path, &["diff", "--name-only", "--diff-filter=U"])?;
    if !output.status_success {
        return Err(GitError::CommandFailed {
            path: repository_path.to_path_buf(),
            args: "diff --name-only --diff-filter=U".to_string(),
            stderr: output.stderr,
        });
    }

    Ok(output
        .stdout
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn abort_merge(repository_path: &Path) -> GitResult<()> {
    let output = run_git(repository_path, &["merge", "--abort"])?;
    if !output.status_success {
        return Err(GitError::CommandFailed {
            path: repository_path.to_path_buf(),
            args: "merge --abort".to_string(),
            stderr: output.stderr,
        });
    }

    Ok(())
}

fn git_output_summary(output: &GitCommandOutput) -> Option<String> {
    let stderr = output.stderr.trim();
    let stdout = output.stdout.trim();
    let message = match (stderr.is_empty(), stdout.is_empty()) {
        (false, false) => format!("{stderr}\n{stdout}"),
        (false, true) => stderr.to_string(),
        (true, false) => stdout.to_string(),
        (true, true) => String::new(),
    };

    if message.is_empty() {
        None
    } else {
        Some(message.chars().take(4000).collect())
    }
}

fn parse_numstat_count(value: Option<&str>) -> u64 {
    value
        .and_then(|count| count.parse::<u64>().ok())
        .unwrap_or(0)
}

fn parse_numstat_path(path: &str) -> String {
    path.split(" => ")
        .last()
        .unwrap_or(path)
        .trim_matches('"')
        .to_string()
}

fn untracked_file_patches(
    worktree_path: &Path,
    stat_by_path: &mut HashMap<String, (u64, u64)>,
) -> GitResult<Vec<String>> {
    let output = run_git(
        worktree_path,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    if !output.status_success {
        return Err(GitError::CommandFailed {
            path: worktree_path.to_path_buf(),
            args: "ls-files --others --exclude-standard -z".to_string(),
            stderr: output.stderr,
        });
    }

    let mut patches = Vec::new();
    for raw_path in output.stdout.split('\0').filter(|path| !path.is_empty()) {
        let patch = untracked_file_patch(worktree_path, raw_path)?;
        stat_by_path.insert(raw_path.to_string(), (patch.additions, 0));
        patches.push(patch.patch);
    }

    Ok(patches)
}

struct GeneratedPatch {
    patch: String,
    additions: u64,
}

fn untracked_file_patch(worktree_path: &Path, relative_path: &str) -> GitResult<GeneratedPatch> {
    let path = worktree_path.join(relative_path);
    let bytes = fs::read(&path)?;
    let mode = if is_executable(&path) {
        "100755"
    } else {
        "100644"
    };
    let display_path = normalize_diff_path(relative_path);

    if bytes.contains(&0) {
        let content_digest = format!("{:x}", Sha256::digest(&bytes));
        return Ok(GeneratedPatch {
            patch: format!(
                "diff --git a/{display_path} b/{display_path}\nnew file mode {mode}\nindex 0000000..0000000\ncodemax-sha256 {content_digest}\nBinary files /dev/null and b/{display_path} differ\n"
            ),
            additions: 0,
        });
    }

    let text = String::from_utf8_lossy(&bytes);
    let lines = text.lines().collect::<Vec<_>>();
    let additions = lines.len() as u64;
    let mut patch = format!(
        "diff --git a/{display_path} b/{display_path}\nnew file mode {mode}\nindex 0000000..0000000\n--- /dev/null\n+++ b/{display_path}\n"
    );

    if additions > 0 {
        patch.push_str(&format!("@@ -0,0 +1,{additions} @@\n"));
        for line in lines {
            patch.push('+');
            patch.push_str(line.trim_end_matches('\r'));
            patch.push('\n');
        }
    }

    Ok(GeneratedPatch { patch, additions })
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

fn normalize_diff_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn join_patch_sections(tracked_patch: String, untracked_patches: Vec<String>) -> String {
    let mut patch = tracked_patch;
    for section in untracked_patches {
        if !patch.is_empty() && !patch.ends_with('\n') {
            patch.push('\n');
        }
        patch.push_str(&section);
    }
    patch
}

fn split_patch_by_file(
    patch: &str,
    status_by_path: &HashMap<String, String>,
    stat_by_path: &HashMap<String, (u64, u64)>,
) -> Vec<TaskDiffFile> {
    let mut files = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_patch = String::new();

    for line in patch.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            push_diff_file(
                &mut files,
                current_path.take(),
                std::mem::take(&mut current_patch),
                status_by_path,
                stat_by_path,
            );
            current_path = parse_diff_git_path(line);
        }
        current_patch.push_str(line);
    }

    push_diff_file(
        &mut files,
        current_path,
        current_patch,
        status_by_path,
        stat_by_path,
    );

    files
}

fn push_diff_file(
    files: &mut Vec<TaskDiffFile>,
    path: Option<String>,
    patch: String,
    status_by_path: &HashMap<String, String>,
    stat_by_path: &HashMap<String, (u64, u64)>,
) {
    let Some(path) = path else {
        return;
    };
    if patch.trim().is_empty() {
        return;
    }

    let (additions, deletions) = stat_by_path.get(&path).copied().unwrap_or((0, 0));
    let status = status_by_path
        .get(&path)
        .cloned()
        .unwrap_or_else(|| infer_diff_status(&patch).to_string());

    files.push(TaskDiffFile {
        path,
        status,
        additions,
        deletions,
        patch,
    });
}

fn parse_diff_git_path(line: &str) -> Option<String> {
    let body = line.strip_prefix("diff --git ")?;
    if let Some(index) = body.rfind(" b/") {
        return Some(body[index + 3..].trim().to_string());
    }

    body.rfind("\"b/")
        .map(|index| body[index + 3..].trim().trim_end_matches('"').to_string())
}

fn infer_diff_status(patch: &str) -> &'static str {
    if patch.contains("\ndeleted file mode ") {
        "deleted"
    } else if patch.contains("\nnew file mode ") {
        "added"
    } else {
        "modified"
    }
}

fn parse_porcelain_status_line(line: &str) -> Option<WorktreeFileChange> {
    if line.len() < 4 {
        return None;
    }

    let code = &line[..2];
    let raw_path = &line[3..];
    let path = raw_path
        .split(" -> ")
        .last()
        .unwrap_or(raw_path)
        .trim_matches('"')
        .to_string();

    Some(WorktreeFileChange {
        path,
        status: porcelain_status_label(code).to_string(),
    })
}

fn porcelain_status_label(code: &str) -> &'static str {
    if code.contains('D') {
        "deleted"
    } else if code == "??" || code.contains('A') {
        "added"
    } else {
        "modified"
    }
}

fn repository_name(repository_path: &Path) -> String {
    repository_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| repository_path.to_string_lossy().to_string())
}

fn task_identity(task_id: &str) -> GitResult<String> {
    let slug = task_id_slug(task_id);
    if slug.is_empty() {
        return Err(GitError::InvalidTaskId(task_id.to_string()));
    }

    if slug == task_id {
        Ok(slug)
    } else {
        Ok(format!("{}-{:08x}", slug, stable_task_hash(task_id)))
    }
}

fn task_id_slug(task_id: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for character in task_id.chars() {
        let next = if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.') {
            previous_dash = false;
            character.to_ascii_lowercase()
        } else {
            if previous_dash {
                continue;
            }
            previous_dash = true;
            '-'
        };
        slug.push(next);
    }

    slug.trim_matches('-').chars().take(80).collect()
}

fn stable_task_hash(task_id: &str) -> u32 {
    let mut hash = 0x811c9dc5_u32;
    for byte in task_id.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitCommandOutput {
    stdout: String,
    stderr: String,
    status_success: bool,
}

fn run_git(path: &Path, args: &[&str]) -> GitResult<GitCommandOutput> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .map_err(|error| GitError::GitUnavailable(error.to_string()))?;

    Ok(GitCommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        status_success: output.status.success(),
    })
}

fn run_required_git(path: &Path, args: &[&str]) -> GitResult<GitCommandOutput> {
    let output = run_git(path, args)?;
    if output.status_success {
        Ok(output)
    } else {
        Err(GitError::CommandFailed {
            path: path.to_path_buf(),
            args: args.join(" "),
            stderr: output.stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
        fs::write(path.join("modified.txt"), "before").expect("write modified fixture");
        fs::write(path.join("deleted.txt"), "delete me").expect("write deleted fixture");
        run_test_git(&path, &["add", "."]);
        run_test_git(&path, &["commit", "-m", "initial fixture"]);

        path
    }

    #[test]
    fn inspect_project_resolves_a_repository_subdirectory_to_the_git_root() {
        let repository = committed_repository("inspect-project-subdirectory");
        let nested = repository.join("src/nested");
        fs::create_dir_all(&nested).expect("create nested directory");

        let project = inspect_project(&nested).expect("inspect nested project path");

        assert!(project.is_git_repository);
        assert_eq!(
            PathBuf::from(project.path)
                .canonicalize()
                .expect("canonical project path"),
            repository
                .canonicalize()
                .expect("canonical repository path")
        );
        assert!(project
            .branch
            .is_some_and(|branch| !branch.trim().is_empty()));

        fs::remove_dir_all(repository).expect("clean repository");
    }

    #[test]
    fn validate_repository_rejects_non_git_directory() {
        let path = temp_path("non-git");

        let error = validate_repository(&path).expect_err("missing directory should fail");

        assert!(matches!(error, GitError::PathNotFound(_)));
    }

    #[test]
    fn validate_repository_returns_repository_summary() {
        let path = temp_path("git-repo");
        fs::create_dir_all(&path).expect("create temp repository directory");
        let init = Command::new("git")
            .arg("-C")
            .arg(&path)
            .arg("init")
            .output()
            .expect("run git init");
        assert!(
            init.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );

        let repository = validate_repository(&path).expect("validate git repository");

        assert_eq!(repository.name, path.file_name().unwrap().to_string_lossy());
        assert_eq!(
            repository.path,
            path.canonicalize().unwrap().to_string_lossy()
        );
        assert!(!repository.branch.is_empty());
        assert!(!repository.dirty);

        fs::remove_dir_all(path).expect("clean temp repository");
    }

    #[test]
    fn current_branch_returns_named_branch() {
        let path = temp_path("git-branch");
        fs::create_dir_all(&path).expect("create temp repository directory");
        let init = Command::new("git")
            .arg("-C")
            .arg(&path)
            .arg("init")
            .output()
            .expect("run git init");
        assert!(
            init.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        let checkout = Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(["checkout", "-b", "codemax-test"])
            .output()
            .expect("create branch");
        assert!(
            checkout.status.success(),
            "git checkout failed: {}",
            String::from_utf8_lossy(&checkout.stderr)
        );

        let branch = current_branch(&path).expect("read current branch");

        assert_eq!(branch, "codemax-test");
        fs::remove_dir_all(path).expect("clean temp repository");
    }

    #[test]
    fn has_uncommitted_changes_detects_untracked_files() {
        let path = temp_path("git-dirty");
        fs::create_dir_all(&path).expect("create temp repository directory");
        let init = Command::new("git")
            .arg("-C")
            .arg(&path)
            .arg("init")
            .output()
            .expect("run git init");
        assert!(
            init.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );

        assert!(!has_uncommitted_changes(&path).expect("clean repository status"));

        fs::write(path.join("untracked.txt"), "pending change").expect("write untracked file");

        assert!(has_uncommitted_changes(&path).expect("dirty repository status"));
        fs::remove_dir_all(path).expect("clean temp repository");
    }

    #[test]
    fn task_worktree_rule_keeps_paths_unique_and_traceable() {
        let root = PathBuf::from("D:/codemax/app-data/worktrees");

        assert_eq!(
            task_branch_name("task-001").expect("branch name"),
            "agent/task-001"
        );
        assert_eq!(
            task_worktree_path(&root, "task-001").expect("worktree path"),
            root.join("task-001")
        );

        let branch = task_branch_name("Fix login #42").expect("branch name with hash");
        assert!(branch.starts_with("agent/fix-login-42-"));
    }

    #[test]
    fn create_task_branch_is_idempotent_and_includes_task_id() {
        let repository = committed_repository("git-branch-manager");

        let created = create_task_branch(&repository, "task-001").expect("create task branch");
        let repeated = create_task_branch(&repository, "task-001").expect("reuse task branch");

        assert_eq!(created.branch_name, "agent/task-001");
        assert_eq!(created, repeated);

        fs::remove_dir_all(repository).expect("clean temp repository");
    }

    #[test]
    fn create_task_worktree_creates_directory_and_branch() {
        let repository = committed_repository("git-worktree-create");
        let worktree_root = temp_path("worktrees");

        let worktree = create_task_worktree(&repository, &worktree_root, "task-001")
            .expect("create task worktree");

        assert_eq!(worktree.branch_name, "agent/task-001");
        assert!(PathBuf::from(&worktree.worktree_path).is_dir());
        assert_eq!(
            current_branch(&worktree.worktree_path).expect("read worktree branch"),
            "agent/task-001"
        );

        remove_task_worktree(&repository, &worktree.worktree_path).expect("remove worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean temp repository");
    }

    #[test]
    fn worktree_status_detects_added_modified_and_deleted_files() {
        let repository = committed_repository("git-worktree-status");
        let worktree_root = temp_path("worktree-status-root");
        let worktree = create_task_worktree(&repository, &worktree_root, "task-002")
            .expect("create task worktree");
        let worktree_path = PathBuf::from(&worktree.worktree_path);

        fs::write(worktree_path.join("added.txt"), "new").expect("write added file");
        fs::write(worktree_path.join("modified.txt"), "after").expect("modify tracked file");
        fs::remove_file(worktree_path.join("deleted.txt")).expect("delete tracked file");

        let status = worktree_status("task-002", &worktree_path).expect("read worktree status");
        let statuses = status
            .changes
            .iter()
            .map(|change| change.status.as_str())
            .collect::<Vec<_>>();

        assert!(status.dirty);
        assert!(statuses.contains(&"added"));
        assert!(statuses.contains(&"modified"));
        assert!(statuses.contains(&"deleted"));

        fs::remove_dir_all(worktree_path).expect("clean dirty worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean temp repository");
    }

    #[test]
    fn remove_task_worktree_removes_directory() {
        let repository = committed_repository("git-worktree-remove");
        let worktree_root = temp_path("worktree-remove-root");
        let worktree = create_task_worktree(&repository, &worktree_root, "task-003")
            .expect("create task worktree");
        let worktree_path = PathBuf::from(&worktree.worktree_path);

        remove_task_worktree(&repository, &worktree_path).expect("remove worktree");

        assert!(!worktree_path.exists());

        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean temp repository");
    }

    #[test]
    fn merge_task_branch_commits_dirty_worktree_and_merges_into_target() {
        let repository = committed_repository("git-merge-success");
        let target_branch = current_branch(&repository).expect("read target branch");
        let worktree_root = temp_path("merge-success-root");
        let worktree = create_task_worktree(&repository, &worktree_root, "task-merge-success")
            .expect("create task worktree");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(worktree_path.join("modified.txt"), "after").expect("modify worktree file");

        let (baseline, _) = capture_merge_baseline(
            "task-merge-success",
            &repository,
            &worktree_path,
            &target_branch,
        )
        .expect("capture merge baseline");
        let result = merge_task_branch(
            "task-merge-success",
            &repository,
            &worktree_path,
            &target_branch,
            "feat: merge task worktree",
            &baseline,
        )
        .expect("merge task branch");

        assert_eq!(result.status, TaskMergeStatus::Merged);
        assert_eq!(result.target_branch, target_branch);
        assert_eq!(result.source_branch, "agent/task-merge-success");
        assert!(result.conflict_files.is_empty());
        assert!(!result.commit_sha.is_empty());
        assert_eq!(
            fs::read_to_string(repository.join("modified.txt")).expect("read merged file"),
            "after"
        );
        assert!(!has_uncommitted_changes(&repository).expect("target repository remains clean"));

        fs::remove_dir_all(worktree_path).expect("clean dirty worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean temp repository");
    }

    #[test]
    fn merge_task_branch_reports_conflicts_and_aborts_target_merge() {
        let repository = committed_repository("git-merge-conflict");
        let target_branch = current_branch(&repository).expect("read target branch");
        let worktree_root = temp_path("merge-conflict-root");
        let worktree = create_task_worktree(&repository, &worktree_root, "task-merge-conflict")
            .expect("create task worktree");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(worktree_path.join("modified.txt"), "agent change")
            .expect("modify worktree file");
        fs::write(repository.join("modified.txt"), "target change").expect("modify target file");
        run_test_git(&repository, &["add", "modified.txt"]);
        run_test_git(&repository, &["commit", "-m", "target change"]);

        let (baseline, _) = capture_merge_baseline(
            "task-merge-conflict",
            &repository,
            &worktree_path,
            &target_branch,
        )
        .expect("capture conflict baseline");
        let result = merge_task_branch(
            "task-merge-conflict",
            &repository,
            &worktree_path,
            &target_branch,
            "feat: merge conflicting task worktree",
            &baseline,
        )
        .expect("conflict is returned as a merge result");

        assert_eq!(result.status, TaskMergeStatus::Conflicted);
        assert_eq!(result.conflict_files, vec!["modified.txt".to_string()]);
        assert!(result
            .error_reason
            .as_deref()
            .unwrap_or_default()
            .contains("modified.txt"));
        assert!(result.commit_sha.is_empty());
        assert_eq!(
            fs::read_to_string(repository.join("modified.txt")).expect("read target file"),
            "target change"
        );
        assert!(!has_uncommitted_changes(&repository).expect("merge abort leaves target clean"));

        fs::remove_dir_all(worktree_path).expect("clean dirty worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean temp repository");
    }

    #[test]
    fn merge_rejects_target_changes_after_preview_without_overwriting_user_file() {
        let repository = committed_repository("git-merge-target-baseline-change");
        let target_branch = current_branch(&repository).expect("read target branch");
        let worktree_root = temp_path("merge-target-baseline-root");
        let worktree = create_task_worktree(
            &repository,
            &worktree_root,
            "task-merge-target-baseline-change",
        )
        .expect("create task worktree");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(worktree_path.join("modified.txt"), "task change").expect("modify task worktree");
        let (baseline, _) = capture_merge_baseline(
            "task-merge-target-baseline-change",
            &repository,
            &worktree_path,
            &target_branch,
        )
        .expect("capture clean preview baseline");

        fs::write(repository.join("user-local.txt"), "preserve me")
            .expect("write target user change");
        let error = merge_task_branch(
            "task-merge-target-baseline-change",
            &repository,
            &worktree_path,
            &target_branch,
            "feat: must not overwrite target",
            &baseline,
        )
        .expect_err("dirty target must invalidate preview");

        assert!(matches!(
            error,
            GitError::MergeBaselineChanged { ref changed_fields }
                if changed_fields.contains(&"targetWorktree".to_string())
        ));
        assert_eq!(
            fs::read_to_string(repository.join("user-local.txt")).expect("read preserved file"),
            "preserve me"
        );
        assert_eq!(
            fs::read_to_string(repository.join("modified.txt")).expect("read unchanged target"),
            "before"
        );

        fs::remove_file(repository.join("user-local.txt")).expect("remove test user file");
        fs::remove_dir_all(worktree_path).expect("clean task worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
    }

    #[test]
    fn merge_rejects_task_diff_changes_after_preview_and_preserves_them() {
        let repository = committed_repository("git-merge-task-baseline-change");
        let target_branch = current_branch(&repository).expect("read target branch");
        let worktree_root = temp_path("merge-task-baseline-root");
        let worktree = create_task_worktree(
            &repository,
            &worktree_root,
            "task-merge-task-baseline-change",
        )
        .expect("create task worktree");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(worktree_path.join("modified.txt"), "previewed task change")
            .expect("write previewed task change");
        let (baseline, _) = capture_merge_baseline(
            "task-merge-task-baseline-change",
            &repository,
            &worktree_path,
            &target_branch,
        )
        .expect("capture preview baseline");
        fs::write(
            worktree_path.join("modified.txt"),
            "new unconfirmed task change",
        )
        .expect("change task diff after preview");

        let error = merge_task_branch(
            "task-merge-task-baseline-change",
            &repository,
            &worktree_path,
            &target_branch,
            "feat: reject stale task diff",
            &baseline,
        )
        .expect_err("task diff change must invalidate preview");

        assert!(matches!(
            error,
            GitError::MergeBaselineChanged { ref changed_fields }
                if changed_fields.contains(&"diff".to_string())
        ));
        assert_eq!(
            fs::read_to_string(worktree_path.join("modified.txt"))
                .expect("read preserved task change"),
            "new unconfirmed task change"
        );
        assert_eq!(
            fs::read_to_string(repository.join("modified.txt")).expect("read target file"),
            "before"
        );

        fs::remove_dir_all(worktree_path).expect("clean task worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
    }

    #[test]
    fn untracked_binary_content_changes_merge_diff_digest() {
        let repository = committed_repository("git-merge-binary-digest");
        let target_branch = current_branch(&repository).expect("read target branch");
        let worktree_root = temp_path("merge-binary-digest-root");
        let worktree =
            create_task_worktree(&repository, &worktree_root, "task-merge-binary-digest")
                .expect("create task worktree");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        let binary_path = worktree_path.join("asset.bin");
        fs::write(&binary_path, [0, 1, 2, 3]).expect("write first binary content");
        let (first, _) = capture_merge_baseline(
            "task-merge-binary-digest",
            &repository,
            &worktree_path,
            &target_branch,
        )
        .expect("capture first binary digest");
        fs::write(&binary_path, [0, 1, 2, 4]).expect("write changed binary content");
        let (second, _) = capture_merge_baseline(
            "task-merge-binary-digest",
            &repository,
            &worktree_path,
            &target_branch,
        )
        .expect("capture second binary digest");

        assert_ne!(first.diff_digest, second.diff_digest);
        assert!(first.changed_fields(&second).contains(&"diff".to_string()));

        fs::remove_dir_all(worktree_path).expect("clean task worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
    }

    #[test]
    fn capture_merge_baseline_rejects_worktree_from_another_repository() {
        let repository = committed_repository("git-merge-repository-a");
        let other_repository = committed_repository("git-merge-repository-b");
        let target_branch = current_branch(&repository).expect("read target branch");

        let error = capture_merge_baseline(
            "task-cross-repository",
            &repository,
            &other_repository,
            &target_branch,
        )
        .expect_err("cross-repository worktree must be rejected");
        assert!(matches!(error, GitError::WorktreeRepositoryMismatch { .. }));

        fs::remove_dir_all(other_repository).expect("clean other repository");
        fs::remove_dir_all(repository).expect("clean repository");
    }
}
