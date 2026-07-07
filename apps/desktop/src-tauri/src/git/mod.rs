use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;
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
}

pub type GitResult<T> = Result<T, GitError>;

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

fn repository_root(path: &Path) -> GitResult<PathBuf> {
    ensure_directory(path)?;

    let inside_work_tree = run_git(path, &["rev-parse", "--is-inside-work-tree"])?;
    if !inside_work_tree.status_success || inside_work_tree.stdout.trim() != "true" {
        return Err(GitError::NotRepository {
            path: path.to_path_buf(),
            stderr: inside_work_tree.stderr,
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
        return Ok(GeneratedPatch {
            patch: format!(
                "diff --git a/{display_path} b/{display_path}\nnew file mode {mode}\nindex 0000000..0000000\nBinary files /dev/null and b/{display_path} differ\n"
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
    fn validate_repository_rejects_non_git_directory() {
        let path = temp_path("non-git");
        fs::create_dir_all(&path).expect("create temp directory");

        let error = validate_repository(&path).expect_err("non-git directory should fail");

        assert!(matches!(error, GitError::NotRepository { .. }));
        fs::remove_dir_all(path).expect("clean temp directory");
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
}
