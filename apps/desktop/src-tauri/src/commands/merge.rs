use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use fs2::FileExt;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::State;
use uuid::Uuid;

use crate::{
    core::error::{AppResult, CommandError},
    git::{self, GitError, MergeBaseline, TaskMergeStatus},
    storage::{
        AgentEventRepository, ArtifactRecord, ArtifactRepository, CommandRunRecord,
        CommandRunRepository, ManagedStorage, NewAgentEvent, NewArtifact, NewArtifactFile,
        StorageError, TaskRecord, TaskRepository,
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
    pub preview_id: String,
    pub baseline_digest: String,
    pub target_head: String,
    pub source_head: String,
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
    pub preview_id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskMergeRecord {
    task_id: String,
    preview_id: String,
    status: String,
    target_branch: String,
    source_branch: String,
    commit_sha: String,
    commit_message: String,
    conflict_files: Vec<String>,
    error_reason: Option<String>,
    baseline: MergeBaseline,
    recovery_suggestions: Vec<String>,
    started_at: String,
    recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedMergePreview {
    preview_id: String,
    task_id: String,
    status: String,
    baseline_digest: String,
    review_digest: String,
    #[serde(default)]
    preview_nonce: String,
    baseline: MergeBaseline,
    blockers: Vec<String>,
    can_merge: bool,
    preview_path: String,
    prepared_at: String,
    invalidated_at: Option<String>,
    invalidation_reason: Option<String>,
}

#[derive(Debug)]
struct StoredMergeAttempt {
    task_id: String,
    status: String,
    target_branch: String,
    source_branch: String,
    commit_sha: String,
    commit_message: String,
    conflict_files: Vec<String>,
    error_reason: Option<String>,
    record_path: Option<String>,
}

#[derive(Debug)]
struct RecordedMergeResult {
    record_path: String,
    warnings: Vec<String>,
}

struct MergeAttemptLock {
    file: File,
}

impl Drop for MergeAttemptLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
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
    let preview_id = request.preview_id.trim().to_string();
    if preview_id.is_empty() {
        return Err(CommandError::new(
            "merge.previewRequired",
            "A persisted merge preview is required. Reopen the merge preview and confirm again.",
        ));
    }
    let commit_message = request.commit_message.trim().to_string();
    let attempt_id = merge_attempt_id(&task_id, &preview_id);
    let _attempt_lock = acquire_merge_attempt_lock(storage, &task_id, &attempt_id)?;

    if let Some(existing) = load_merge_attempt(storage, &attempt_id)? {
        return existing_attempt_result(&task_id, existing);
    }

    let mut preview = load_latest_merge_preview(storage, &task_id)?.ok_or_else(|| {
        CommandError::new(
            "merge.previewRequired",
            "No persisted merge preview exists. Reopen the merge preview and confirm again.",
        )
    })?;
    if preview.preview_id != preview_id || preview.status != "prepared" {
        return Err(stale_preview_error(&[]));
    }

    let task = load_task(storage, &task_id)?;
    let worktree_path = match task.worktree_path.clone() {
        Some(path) => path,
        None => {
            let cause = CommandError::new(
                "merge.worktreeMissing",
                format!("Task {task_id} does not have a saved worktree path."),
            );
            invalidate_preview_and_review(
                storage,
                &mut preview,
                &["worktreePath".to_string()],
                "The task worktree binding is no longer available.",
            )?;
            set_task_status(storage, &task_id, "awaitingReview", None)?;
            return Err(stale_preview_revalidation_error(&cause));
        }
    };
    let target_branch = match frozen_target_branch(&task, request.target_branch.as_deref()) {
        Ok(branch) => branch,
        Err(error) => {
            invalidate_preview_and_review(
                storage,
                &mut preview,
                &["targetBranch".to_string()],
                "The task target branch binding changed after the preview was prepared.",
            )?;
            set_task_status(storage, &task_id, "awaitingReview", None)?;
            return Err(stale_preview_revalidation_error(&error));
        }
    };
    let (current_baseline, _) = match git::capture_merge_baseline(
        &task_id,
        &task.repository_path,
        &worktree_path,
        &target_branch,
    ) {
        Ok(value) => value,
        Err(error) => {
            let merge_error = merge_git_error(error);
            let changed_fields = vec!["baselineUnavailable".to_string()];
            invalidate_preview_and_review(
                storage,
                &mut preview,
                &changed_fields,
                "The saved merge baseline could not be revalidated.",
            )?;
            set_task_status(storage, &task_id, "awaitingReview", None)?;
            return Err(stale_preview_revalidation_error(&merge_error));
        }
    };
    let changed_fields = preview.baseline.changed_fields(&current_baseline);
    if !changed_fields.is_empty() {
        invalidate_preview_and_review(
            storage,
            &mut preview,
            &changed_fields,
            "Merge baseline changed after confirmation was shown.",
        )?;
        set_task_status(storage, &task_id, "awaitingReview", None)?;
        return Err(stale_preview_error(&changed_fields));
    }
    if task.branch_name.as_deref() != Some(current_baseline.source_branch.as_str()) {
        let changed_fields = vec!["taskBranchBinding".to_string()];
        invalidate_preview_and_review(
            storage,
            &mut preview,
            &changed_fields,
            "The saved task branch binding changed after the preview was prepared.",
        )?;
        set_task_status(storage, &task_id, "awaitingReview", None)?;
        return Err(stale_preview_error(&changed_fields));
    }

    let stored_baseline_digest = stable_json_digest(&preview.baseline)?;
    let review_digest = merge_review_digest(storage, &task_id)?;
    let current_preview_id = merge_preview_id(
        &task_id,
        &stored_baseline_digest,
        &review_digest,
        &preview.preview_nonce,
    );
    if stored_baseline_digest != preview.baseline_digest
        || review_digest != preview.review_digest
        || current_preview_id != preview_id
    {
        invalidate_preview_record(
            storage,
            &mut preview,
            "Quality Gate or approval state changed after this preview was prepared.",
        )?;
        set_task_status(storage, &task_id, "awaitingReview", None)?;
        return Err(stale_preview_error(&["deliveryReview".to_string()]));
    }

    let mut blockers = merge_blockers(current_baseline.target_dirty, &commit_message);
    blockers.extend(
        crate::commands::s12_evidence::delivery_review_blockers_for_task(storage, &task_id)?,
    );
    if !blockers.is_empty() {
        return Err(CommandError::new(
            "merge.precheckFailed",
            format!("Merge precheck failed: {}", blockers.join("; ")),
        ));
    }

    start_merge_attempt(storage, &attempt_id, &task_id, &preview, &commit_message)?;
    let _ = record_merge_event(
        storage,
        &task_id,
        "merge.started",
        "readyToMerge",
        "Local merge started from a persisted, revalidated preview.",
        json!({
            "preview_id": &preview.preview_id,
            "baseline_digest": &preview.baseline_digest,
            "target_branch": &preview.baseline.target_branch,
            "source_branch": &preview.baseline.source_branch,
            "target_head": &preview.baseline.target_head,
            "source_head": &preview.baseline.source_head,
            "worktree_path": &preview.baseline.worktree_path,
        }),
    );
    let _ = set_task_status(storage, &task_id, "readyToMerge", None);

    let merge_result = match git::merge_task_branch(
        &task_id,
        &task.repository_path,
        &worktree_path,
        &target_branch,
        &commit_message,
        &preview.baseline,
    ) {
        Ok(result) => result,
        Err(error) => {
            let merge_error = merge_git_error(error);
            finalize_merge_failure(
                storage,
                &attempt_id,
                &task_id,
                &preview,
                &commit_message,
                &merge_error.message,
            )?;
            let _ = update_preview_status(
                storage,
                &mut preview,
                "failed",
                Some("The merge attempt failed and requires a fresh preview before retry."),
            );
            set_task_status(storage, &task_id, "needsIntervention", None)?;
            let _ = record_merge_event(
                storage,
                &task_id,
                "merge.finished",
                "needsIntervention",
                "Local merge failed; CodeMax did not report success.",
                json!({
                    "preview_id": &preview.preview_id,
                    "target_branch": &preview.baseline.target_branch,
                    "source_branch": &preview.baseline.source_branch,
                    "error": &merge_error.message,
                }),
            );
            return Err(merge_error);
        }
    };

    let task_status = match merge_result.status {
        TaskMergeStatus::Merged => "merged",
        TaskMergeStatus::Conflicted => "needsIntervention",
    };
    let completed_at = matches!(merge_result.status, TaskMergeStatus::Merged).then(now_text);
    let recorded =
        match record_merge_result(storage, &attempt_id, &task_id, &preview, &merge_result) {
            Ok(recorded) => recorded,
            Err(error) => {
                let _ = set_task_status(storage, &task_id, task_status, completed_at.as_deref());
                return Err(merge_outcome_persistence_error(&merge_result.status, error));
            }
        };
    let mut persistence_warnings = recorded.warnings;
    if set_task_status(storage, &task_id, task_status, completed_at.as_deref()).is_err() {
        persistence_warnings.push(
            "The Git outcome is recorded, but the derived task status could not be updated."
                .to_string(),
        );
    }
    let preview_status = match merge_result.status {
        TaskMergeStatus::Merged => "merged",
        TaskMergeStatus::Conflicted => "conflicted",
    };
    if update_preview_status(storage, &mut preview, preview_status, None).is_err() {
        persistence_warnings.push(
            "The Git outcome is recorded, but the preview status could not be updated.".to_string(),
        );
    }
    let _ = record_merge_event(
        storage,
        &task_id,
        "merge.finished",
        task_status,
        match merge_result.status {
            TaskMergeStatus::Merged => "Local merge finished and was verified successfully.",
            TaskMergeStatus::Conflicted => {
                "Local merge conflicted, was aborted back to the clean target baseline, and was not marked successful."
            }
        },
        json!({
            "preview_id": &preview.preview_id,
            "status": &merge_result.status,
            "target_branch": &merge_result.target_branch,
            "source_branch": &merge_result.source_branch,
            "commit_sha": &merge_result.commit_sha,
            "conflict_files": &merge_result.conflict_files,
            "record_path": &recorded.record_path,
            "persistence_warnings": &persistence_warnings,
        }),
    );

    Ok(TaskMergeCommandResult {
        task_id,
        status: merge_result.status,
        target_branch: merge_result.target_branch,
        source_branch: merge_result.source_branch,
        commit_sha: merge_result.commit_sha,
        commit_message: merge_result.commit_message,
        conflict_files: merge_result.conflict_files,
        error_reason: merge_result.error_reason,
        merge_record_path: Some(recorded.record_path),
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
    let mut previous_preview = load_latest_merge_preview(storage, &task_id)?;
    let worktree_path = match task.worktree_path.clone() {
        Some(path) => path,
        None => {
            let cause = CommandError::new(
                "merge.worktreeMissing",
                format!("Task {task_id} does not have a saved worktree path."),
            );
            if let Some(previous) = previous_preview
                .as_mut()
                .filter(|preview| preview.status == "prepared")
            {
                invalidate_preview_and_review(
                    storage,
                    previous,
                    &["worktreePath".to_string()],
                    "The task worktree binding is no longer available.",
                )?;
                set_task_status(storage, &task_id, "awaitingReview", None)?;
                return Err(stale_preview_revalidation_error(&cause));
            }
            return Err(cause);
        }
    };
    let target_branch = match frozen_target_branch(&task, request.target_branch.as_deref()) {
        Ok(branch) => branch,
        Err(error) => {
            if let Some(previous) = previous_preview
                .as_mut()
                .filter(|preview| preview.status == "prepared")
            {
                invalidate_preview_and_review(
                    storage,
                    previous,
                    &["targetBranch".to_string()],
                    "The task target branch binding changed after the preview was prepared.",
                )?;
                set_task_status(storage, &task_id, "awaitingReview", None)?;
                return Err(stale_preview_revalidation_error(&error));
            }
            return Err(error);
        }
    };
    let (baseline, diff) = match git::capture_merge_baseline(
        &task.id,
        &task.repository_path,
        &worktree_path,
        &target_branch,
    ) {
        Ok(value) => value,
        Err(error) => {
            if let Some(previous) = previous_preview
                .as_mut()
                .filter(|preview| preview.status == "prepared")
            {
                invalidate_preview_and_review(
                    storage,
                    previous,
                    &["baselineUnavailable".to_string()],
                    "The saved merge baseline could not be revalidated.",
                )?;
                set_task_status(storage, &task_id, "awaitingReview", None)?;
            }
            return Err(merge_git_error(error));
        }
    };
    if let Some(previous) = previous_preview.as_mut() {
        let changed_fields = previous.baseline.changed_fields(&baseline);
        if !changed_fields.is_empty() && previous.status == "prepared" {
            invalidate_preview_and_review(
                storage,
                previous,
                &changed_fields,
                "Repository, branch, HEAD, worktree, or Diff changed after the previous preview.",
            )?;
            set_task_status(storage, &task_id, "awaitingReview", None)?;
        }
    }

    if task.branch_name.as_deref() != Some(baseline.source_branch.as_str()) {
        let error = CommandError::new(
            "merge.sourceBranchChanged",
            "The task worktree branch no longer matches the branch saved for this task. The old merge preview and confirmation cannot be reused.",
        );
        if let Some(previous) = previous_preview
            .as_mut()
            .filter(|preview| preview.status == "prepared")
        {
            invalidate_preview_and_review(
                storage,
                previous,
                &["taskBranchBinding".to_string()],
                "The saved task branch binding changed after the preview was prepared.",
            )?;
            set_task_status(storage, &task_id, "awaitingReview", None)?;
            return Err(stale_preview_revalidation_error(&error));
        }
        return Err(error);
    }

    let validation_runs = latest_validation_runs(&command_runs);
    let validation_status = validation_status(&validation_runs).to_string();
    let validation_summary = validation_summary(&validation_status, validation_runs.len());
    let diff_path =
        latest_diff_artifact(&artifacts).and_then(|artifact| artifact.diff_path.clone());
    let commit_message =
        latest_commit_message(&artifacts).unwrap_or_else(|| fallback_commit_message(&task));
    let mut blockers = merge_blockers(baseline.target_dirty, &commit_message);
    blockers.extend(
        crate::commands::s12_evidence::delivery_review_blockers_for_task(storage, &task_id)?,
    );

    let baseline_digest = stable_json_digest(&baseline)?;
    let review_digest = merge_review_digest(storage, &task_id)?;
    let preview_nonce = previous_preview
        .as_ref()
        .filter(|previous| {
            previous.status == "prepared"
                && previous.baseline_digest == baseline_digest
                && previous.review_digest == review_digest
        })
        .map(|previous| previous.preview_nonce.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let preview_id = merge_preview_id(&task_id, &baseline_digest, &review_digest, &preview_nonce);
    let prepared_at = now_text();
    let preview_path = preview_record_path(storage, &task_id, &preview_id)?;
    let preview = PersistedMergePreview {
        preview_id: preview_id.clone(),
        task_id: task.id.clone(),
        status: "prepared".to_string(),
        baseline_digest: baseline_digest.clone(),
        review_digest,
        preview_nonce,
        baseline: baseline.clone(),
        blockers: blockers.clone(),
        can_merge: blockers.is_empty(),
        preview_path: preview_path.to_string_lossy().to_string(),
        prepared_at,
        invalidated_at: None,
        invalidation_reason: None,
    };
    persist_merge_preview(storage, &preview)?;

    Ok(PreparedTaskMerge {
        task_id: task.id,
        target_branch: baseline.target_branch,
        source_branch: baseline.source_branch,
        worktree_path: baseline.worktree_path,
        preview_id,
        baseline_digest,
        target_head: baseline.target_head,
        source_head: baseline.source_head,
        target_dirty: baseline.target_dirty,
        worktree_dirty: baseline.worktree_dirty,
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

fn frozen_target_branch(task: &TaskRecord, requested: Option<&str>) -> AppResult<String> {
    let saved = task.target_branch.trim();
    let target_branch = if saved.is_empty() {
        git::current_branch(&task.repository_path).map_err(merge_git_error)?
    } else {
        saved.to_string()
    };

    if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        if requested != target_branch {
            return Err(CommandError::new(
                "merge.targetBranchChanged",
                "The requested target branch does not match the branch saved for this task.",
            ));
        }
    }

    Ok(target_branch)
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

    for run in runs.iter().filter(|run| run.purpose == "validation") {
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

fn merge_blockers(target_dirty: bool, commit_message: &str) -> Vec<String> {
    let mut blockers = Vec::new();

    if target_dirty {
        blockers.push("target branch has uncommitted changes".to_string());
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
    attempt_id: &str,
    task_id: &str,
    preview: &PersistedMergePreview,
    result: &git::TaskMergeResult,
) -> AppResult<RecordedMergeResult> {
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(task_id)
        .map_err(storage_error)?;
    let record_path = paths
        .artifacts_dir
        .join(format!("merge-record-{}.json", preview.preview_id));
    let latest_record_path = paths.artifacts_dir.join("merge-record.json");
    let status = match result.status {
        TaskMergeStatus::Merged => "merged",
        TaskMergeStatus::Conflicted => "conflicted",
    };
    let record = TaskMergeRecord {
        task_id: task_id.to_string(),
        preview_id: preview.preview_id.clone(),
        status: status.to_string(),
        target_branch: result.target_branch.clone(),
        source_branch: result.source_branch.clone(),
        commit_sha: result.commit_sha.clone(),
        commit_message: result.commit_message.clone(),
        conflict_files: result.conflict_files.clone(),
        error_reason: result.error_reason.clone(),
        baseline: preview.baseline.clone(),
        recovery_suggestions: merge_recovery_suggestions(status),
        started_at: now_text(),
        recorded_at: now_text(),
    };
    write_json_file(&record_path, &record)?;
    write_json_file(&latest_record_path, &record)?;

    let artifact_id = format!("merge-{task_id}-{}", Uuid::new_v4());
    let file_id = format!("file-{artifact_id}");
    let record_path_text = record_path.to_string_lossy().to_string();
    let mut warnings = Vec::new();
    let size_bytes = match file_size(&record_path) {
        Ok(size) => Some(size),
        Err(_) => {
            warnings.push(
                "The merge outcome is persisted, but its artifact size could not be indexed."
                    .to_string(),
            );
            None
        }
    };
    let changed_files = match changed_files_json(storage, task_id, result) {
        Ok(files) => files,
        Err(_) => {
            warnings.push(
                "The merge outcome is persisted, but changed-file metadata could not be indexed."
                    .to_string(),
            );
            "[]".to_string()
        }
    };
    let summary = match result.status {
        TaskMergeStatus::Merged => format!(
            "Merged {} into {} at {} from preview {}.",
            result.source_branch, result.target_branch, result.commit_sha, preview.preview_id
        ),
        TaskMergeStatus::Conflicted => format!(
            "Merge conflicted while merging {} into {} from preview {}. The target was restored and was not marked successful. {}",
            result.source_branch,
            result.target_branch,
            preview.preview_id,
            result.error_reason.as_deref().unwrap_or_default()
        ),
    };
    let conflict_files = serde_json::to_string(&result.conflict_files).map_err(json_error)?;

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let updated = store
        .connection()
        .execute(
            "UPDATE merge_records
             SET status = ?2, target_branch = ?3, source_branch = ?4, commit_sha = ?5,
                 commit_message = ?6, conflict_files = ?7, error_reason = ?8, record_path = ?9
             WHERE id = ?1 AND task_id = ?10 AND status = 'started'",
            rusqlite::params![
                attempt_id,
                status,
                result.target_branch,
                result.source_branch,
                result.commit_sha,
                result.commit_message,
                conflict_files,
                result.error_reason,
                record_path_text,
                task_id,
            ],
        )
        .map_err(storage_error)?;
    if updated != 1 {
        return Err(CommandError::new(
            "merge.attemptStateChanged",
            "The persisted merge attempt changed unexpectedly; CodeMax will not report success.",
        ));
    }

    if let Some(size_bytes) = size_bytes {
        let artifact_index_result: Result<(), StorageError> = (|| {
            let transaction = store.connection().unchecked_transaction()?;
            let artifacts = ArtifactRepository::new(&transaction);
            artifacts.record_artifact(NewArtifact {
                id: &artifact_id,
                task_id,
                changed_files: &changed_files,
                diff_path: None,
                test_report_path: Some(&record_path_text),
                screenshots: "[]",
                summary: &summary,
                commit_message: &result.commit_message,
            })?;
            artifacts.record_file(NewArtifactFile {
                id: &file_id,
                task_id,
                artifact_id: Some(&artifact_id),
                file_type: "merge_record",
                path: &record_path_text,
                size_bytes: size_bytes as i64,
                compressed: false,
                retention_class: "permanent",
                expires_at: None,
            })?;
            transaction.commit()?;
            Ok(())
        })();
        if artifact_index_result.is_err() {
            warnings.push(
                "The merge outcome is persisted, but the auxiliary artifact index could not be updated."
                    .to_string(),
            );
        }
    }

    Ok(RecordedMergeResult {
        record_path: record_path_text,
        warnings,
    })
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

fn preview_record_path(
    storage: &ManagedStorage,
    task_id: &str,
    preview_id: &str,
) -> AppResult<PathBuf> {
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(task_id)
        .map_err(storage_error)?;
    Ok(paths
        .artifacts_dir
        .join(format!("merge-preview-{preview_id}.json")))
}

fn latest_preview_path(storage: &ManagedStorage, task_id: &str) -> AppResult<PathBuf> {
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(task_id)
        .map_err(storage_error)?;
    Ok(paths.artifacts_dir.join("merge-preview-latest.json"))
}

fn persist_merge_preview(
    storage: &ManagedStorage,
    preview: &PersistedMergePreview,
) -> AppResult<()> {
    write_json_file(Path::new(&preview.preview_path), preview)?;
    write_json_file(&latest_preview_path(storage, &preview.task_id)?, preview)
}

fn load_latest_merge_preview(
    storage: &ManagedStorage,
    task_id: &str,
) -> AppResult<Option<PersistedMergePreview>> {
    let path = latest_preview_path(storage, task_id)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(storage_error)?;
    let preview = serde_json::from_slice::<PersistedMergePreview>(&bytes).map_err(json_error)?;
    if preview.task_id != task_id {
        return Err(CommandError::new(
            "merge.previewTaskMismatch",
            "The persisted merge preview belongs to a different task.",
        ));
    }
    Ok(Some(preview))
}

fn update_preview_status(
    storage: &ManagedStorage,
    preview: &mut PersistedMergePreview,
    status: &str,
    reason: Option<&str>,
) -> AppResult<()> {
    preview.status = status.to_string();
    if let Some(reason) = reason {
        preview.invalidation_reason = Some(reason.to_string());
    }
    persist_merge_preview(storage, preview)
}

fn invalidate_preview_record(
    storage: &ManagedStorage,
    preview: &mut PersistedMergePreview,
    reason: &str,
) -> AppResult<()> {
    preview.status = "invalidated".to_string();
    preview.can_merge = false;
    preview.invalidated_at = Some(now_text());
    preview.invalidation_reason = Some(reason.to_string());
    if !preview.blockers.iter().any(|blocker| blocker == reason) {
        preview.blockers.push(reason.to_string());
    }
    persist_merge_preview(storage, preview)
}

fn invalidate_preview_and_review(
    storage: &ManagedStorage,
    preview: &mut PersistedMergePreview,
    changed_fields: &[String],
    reason: &str,
) -> AppResult<()> {
    let (approval_count, gate_count) =
        invalidate_delivery_authorizations(storage, &preview.task_id, reason)?;
    invalidate_preview_record(storage, preview, reason)?;
    record_merge_event(
        storage,
        &preview.task_id,
        "merge.previewInvalidated",
        "awaitingReview",
        "The merge baseline changed. The old preview, confirmation, approvals, and Quality Gate results are no longer reusable.",
        json!({
            "preview_id": &preview.preview_id,
            "changed_fields": changed_fields,
            "invalidated_approvals": approval_count,
            "invalidated_quality_gates": gate_count,
            "data_changed_by_codemax": false,
        }),
    )
}

fn invalidate_delivery_authorizations(
    storage: &ManagedStorage,
    task_id: &str,
    reason: &str,
) -> AppResult<(usize, usize)> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    let invalidated_at = now_text();
    let approval_count = connection
        .execute(
            "UPDATE approvals
             SET invalidated_at = ?2, invalidation_reason = ?3
             WHERE task_id = ?1
               AND invalidated_at IS NULL
               AND consumed_at IS NULL
               AND (decision IS NULL OR decision = 'approved')",
            rusqlite::params![task_id, invalidated_at, reason],
        )
        .map_err(storage_error)?;

    let gate_types = {
        let mut statement = connection
            .prepare(
                "SELECT q.gate_type
                 FROM quality_gate_results q
                 WHERE q.task_id = ?1
                   AND q.rowid = (
                       SELECT MAX(latest.rowid)
                       FROM quality_gate_results latest
                       WHERE latest.task_id = q.task_id AND latest.gate_type = q.gate_type
                   )
                 ORDER BY q.gate_type",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map(rusqlite::params![task_id], |row| row.get::<_, String>(0))
            .map_err(storage_error)?;
        let mut values = Vec::new();
        for row in rows {
            values.push(row.map_err(storage_error)?);
        }
        values
    };

    let mut gate_count = 0usize;
    for gate_type in gate_types {
        connection
            .execute(
                "INSERT INTO quality_gate_results
                 (id, task_id, gate_type, status, message, evidence_path, override_reason, created_at)
                 VALUES (?1, ?2, ?3, 'invalidated', ?4, NULL, NULL, ?5)",
                rusqlite::params![
                    format!("gate-{task_id}-{gate_type}-{}", Uuid::new_v4()),
                    task_id,
                    gate_type,
                    reason,
                    now_text(),
                ],
            )
            .map_err(storage_error)?;
        gate_count += 1;
    }
    Ok((approval_count, gate_count))
}

fn merge_review_digest(storage: &ManagedStorage, task_id: &str) -> AppResult<String> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    let mut digest = Sha256::new();

    {
        let mut statement = connection
            .prepare(
                "SELECT id, type, COALESCE(decision, ''), COALESCE(decided_at, ''),
                        COALESCE(consumed_at, ''), COALESCE(invalidated_at, ''),
                        COALESCE(invalidation_reason, '')
                 FROM approvals WHERE task_id = ?1 ORDER BY rowid",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map(rusqlite::params![task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(storage_error)?;
        for row in rows {
            let row = row.map_err(storage_error)?;
            for value in [row.0, row.1, row.2, row.3, row.4, row.5, row.6] {
                digest.update(value.as_bytes());
                digest.update([0]);
            }
        }
    }
    digest.update([0xff]);
    {
        let mut statement = connection
            .prepare(
                "SELECT id, gate_type, status, message, COALESCE(evidence_path, ''),
                        COALESCE(override_reason, '')
                 FROM quality_gate_results WHERE task_id = ?1 ORDER BY rowid",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map(rusqlite::params![task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(storage_error)?;
        for row in rows {
            let row = row.map_err(storage_error)?;
            for value in [row.0, row.1, row.2, row.3, row.4, row.5] {
                digest.update(value.as_bytes());
                digest.update([0]);
            }
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn stable_json_digest<T: Serialize>(value: &T) -> AppResult<String> {
    let bytes = serde_json::to_vec(value).map_err(json_error)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn merge_preview_id(
    task_id: &str,
    baseline_digest: &str,
    review_digest: &str,
    preview_nonce: &str,
) -> String {
    let mut digest = Sha256::new();
    digest.update(task_id.as_bytes());
    digest.update([0]);
    digest.update(baseline_digest.as_bytes());
    digest.update([0]);
    digest.update(review_digest.as_bytes());
    if !preview_nonce.is_empty() {
        digest.update([0]);
        digest.update(preview_nonce.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn merge_attempt_id(task_id: &str, preview_id: &str) -> String {
    format!("merge-attempt-{task_id}-{preview_id}")
}

fn acquire_merge_attempt_lock(
    storage: &ManagedStorage,
    task_id: &str,
    attempt_id: &str,
) -> AppResult<MergeAttemptLock> {
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(task_id)
        .map_err(storage_error)?;
    let lock_digest = format!("{:x}", Sha256::digest(attempt_id.as_bytes()));
    let lock_path = paths
        .artifacts_dir
        .join(format!("merge-attempt-{lock_digest}.lock"));
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(storage_error)?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(MergeAttemptLock { file }),
        Err(error) if is_merge_lock_contention(&error) => Err(CommandError::new(
            "merge.inProgress",
            "This exact merge preview is already being processed. No second merge was started.",
        )),
        Err(error) => Err(storage_error(error)),
    }
}

fn is_merge_lock_contention(error: &std::io::Error) -> bool {
    if error.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }

    #[cfg(windows)]
    {
        // LockFileEx reports ERROR_LOCK_VIOLATION for a competing file lock. Depending on the
        // Rust version this is not normalized to WouldBlock, so preserve the product-level
        // in-progress result instead of surfacing a misleading filesystem failure.
        matches!(error.raw_os_error(), Some(33))
    }

    #[cfg(not(windows))]
    {
        false
    }
}

fn start_merge_attempt(
    storage: &ManagedStorage,
    attempt_id: &str,
    task_id: &str,
    preview: &PersistedMergePreview,
    commit_message: &str,
) -> AppResult<()> {
    let record_path = storage
        .roots
        .ensure_task_artifact_dirs(task_id)
        .map_err(storage_error)?
        .artifacts_dir
        .join(format!("merge-record-{}.json", preview.preview_id));
    let record = TaskMergeRecord {
        task_id: task_id.to_string(),
        preview_id: preview.preview_id.clone(),
        status: "started".to_string(),
        target_branch: preview.baseline.target_branch.clone(),
        source_branch: preview.baseline.source_branch.clone(),
        commit_sha: String::new(),
        commit_message: commit_message.to_string(),
        conflict_files: Vec::new(),
        error_reason: None,
        baseline: preview.baseline.clone(),
        recovery_suggestions: merge_recovery_suggestions("started"),
        started_at: now_text(),
        recorded_at: now_text(),
    };
    write_json_file(&record_path, &record)?;
    let record_path_text = record_path.to_string_lossy().to_string();

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let inserted = store
        .connection()
        .execute(
            "INSERT OR IGNORE INTO merge_records
             (id, task_id, status, target_branch, source_branch, commit_sha,
              commit_message, conflict_files, error_reason, record_path, created_at)
             VALUES (?1, ?2, 'started', ?3, ?4, '', ?5, '[]', NULL, ?6, ?7)",
            rusqlite::params![
                attempt_id,
                task_id,
                preview.baseline.target_branch,
                preview.baseline.source_branch,
                commit_message,
                record_path_text,
                now_text(),
            ],
        )
        .map_err(storage_error)?;
    if inserted != 1 {
        return Err(CommandError::new(
            "merge.inProgress",
            "This merge preview already has a persisted attempt. Wait for it to finish or reopen the task to recover it.",
        ));
    }
    Ok(())
}

fn load_merge_attempt(
    storage: &ManagedStorage,
    attempt_id: &str,
) -> AppResult<Option<StoredMergeAttempt>> {
    let mut attempt = {
        let store = storage.store.lock().map_err(|_| storage_lock_error())?;
        store
            .connection()
            .query_row(
                "SELECT task_id, status, target_branch, source_branch, commit_sha, commit_message,
                        conflict_files, error_reason, record_path
                 FROM merge_records WHERE id = ?1",
                rusqlite::params![attempt_id],
                |row| {
                    let conflict_files_json: String = row.get(6)?;
                    Ok(StoredMergeAttempt {
                        task_id: row.get(0)?,
                        status: row.get(1)?,
                        target_branch: row.get(2)?,
                        source_branch: row.get(3)?,
                        commit_sha: row.get(4)?,
                        commit_message: row.get(5)?,
                        conflict_files: serde_json::from_str(&conflict_files_json)
                            .unwrap_or_default(),
                        error_reason: row.get(7)?,
                        record_path: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(storage_error)?
    };

    if let Some(existing) = attempt
        .as_mut()
        .filter(|attempt| attempt.status == "started")
    {
        let _ = reconcile_interrupted_merge_attempt(storage, attempt_id, existing)?;
    }
    Ok(attempt)
}

fn reconcile_interrupted_merge_attempt(
    storage: &ManagedStorage,
    attempt_id: &str,
    attempt: &mut StoredMergeAttempt,
) -> AppResult<bool> {
    let Some(record_path) = attempt.record_path.as_deref() else {
        return Ok(false);
    };
    let Ok(bytes) = fs::read(record_path) else {
        return Ok(false);
    };
    let Ok(mut record) = serde_json::from_slice::<TaskMergeRecord>(&bytes) else {
        return Ok(false);
    };
    if record.task_id != attempt.task_id
        || merge_attempt_id(&record.task_id, &record.preview_id) != attempt_id
        || record.target_branch != attempt.target_branch
        || record.source_branch != attempt.source_branch
        || record.commit_message != attempt.commit_message
    {
        return Ok(false);
    }

    if record.status == "started" {
        match git::inspect_interrupted_merge_outcome(&record.baseline, &record.commit_message) {
            Ok(git::InterruptedMergeOutcome::Merged { commit_sha }) => {
                record.status = "merged".to_string();
                record.commit_sha = commit_sha;
                record.error_reason = None;
                record.recovery_suggestions = merge_recovery_suggestions("merged");
            }
            Ok(git::InterruptedMergeOutcome::NoTargetChange) => {
                record.status = "failed".to_string();
                record.commit_sha.clear();
                record.error_reason = Some(
                    "The application stopped before a final merge outcome was recorded. The target repository was verified at the original clean baseline; reopen the merge preview and confirm again.".to_string(),
                );
                record.recovery_suggestions = merge_recovery_suggestions("failed");
            }
            Err(_) => return Ok(false),
        }
        record.recorded_at = now_text();
        write_json_file(Path::new(record_path), &record)?;
    }

    let verified_status = match record.status.as_str() {
        "merged" => Some(TaskMergeStatus::Merged),
        "conflicted" => Some(TaskMergeStatus::Conflicted),
        "failed" => None,
        _ => return Ok(false),
    };
    if let Some(status) = verified_status.as_ref() {
        if git::verify_persisted_merge_outcome(&record.baseline, status, &record.commit_sha)
            .is_err()
        {
            return Ok(false);
        }
    }

    let conflict_files = serde_json::to_string(&record.conflict_files).map_err(json_error)?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let updated = store
        .connection()
        .execute(
            "UPDATE merge_records
             SET status = ?2, target_branch = ?3, source_branch = ?4, commit_sha = ?5,
                 commit_message = ?6, conflict_files = ?7, error_reason = ?8, record_path = ?9
             WHERE id = ?1 AND task_id = ?10 AND status = 'started'",
            rusqlite::params![
                attempt_id,
                record.status,
                record.target_branch,
                record.source_branch,
                record.commit_sha,
                record.commit_message,
                conflict_files,
                record.error_reason,
                record_path,
                record.task_id,
            ],
        )
        .map_err(storage_error)?;
    if updated == 1 {
        drop(store);
        let (task_status, completed_at, preview_status) = match record.status.as_str() {
            "merged" => ("merged", Some(now_text()), "merged"),
            "conflicted" => ("needsIntervention", None, "conflicted"),
            _ => ("needsIntervention", None, "failed"),
        };
        let _ = set_task_status(
            storage,
            &record.task_id,
            task_status,
            completed_at.as_deref(),
        );
        if let Ok(Some(mut preview)) = load_latest_merge_preview(storage, &record.task_id) {
            if preview.preview_id == record.preview_id {
                let _ = update_preview_status(storage, &mut preview, preview_status, None);
            }
        }
        attempt.status = record.status;
        attempt.target_branch = record.target_branch;
        attempt.source_branch = record.source_branch;
        attempt.commit_sha = record.commit_sha;
        attempt.commit_message = record.commit_message;
        attempt.conflict_files = record.conflict_files;
        attempt.error_reason = record.error_reason;
        return Ok(true);
    }
    Ok(false)
}

fn existing_attempt_result(
    task_id: &str,
    attempt: StoredMergeAttempt,
) -> AppResult<TaskMergeCommandResult> {
    match attempt.status.as_str() {
        "merged" | "conflicted" => {
            let status = if attempt.status == "merged" {
                TaskMergeStatus::Merged
            } else {
                TaskMergeStatus::Conflicted
            };
            let task_status = if attempt.status == "merged" {
                "merged"
            } else {
                "needsIntervention"
            };
            Ok(TaskMergeCommandResult {
                task_id: task_id.to_string(),
                status,
                target_branch: attempt.target_branch,
                source_branch: attempt.source_branch,
                commit_sha: attempt.commit_sha,
                commit_message: attempt.commit_message,
                conflict_files: attempt.conflict_files,
                error_reason: attempt.error_reason,
                merge_record_path: attempt.record_path,
                task_status: task_status.to_string(),
            })
        }
        "started" => Err(CommandError::new(
            "merge.inProgress",
            "This exact merge preview already has an in-progress or interrupted attempt. No second merge was started.",
        )),
        _ => Err(CommandError::new(
            "merge.previousAttemptFailed",
            attempt.error_reason.unwrap_or_else(|| {
                "The previous attempt failed. Inspect its record, recover the worktree if needed, then create a new preview.".to_string()
            }),
        )),
    }
}

fn finalize_merge_failure(
    storage: &ManagedStorage,
    attempt_id: &str,
    task_id: &str,
    preview: &PersistedMergePreview,
    commit_message: &str,
    error_reason: &str,
) -> AppResult<()> {
    let record_path = storage
        .roots
        .ensure_task_artifact_dirs(task_id)
        .map_err(storage_error)?
        .artifacts_dir
        .join(format!("merge-record-{}.json", preview.preview_id));
    let record = TaskMergeRecord {
        task_id: task_id.to_string(),
        preview_id: preview.preview_id.clone(),
        status: "failed".to_string(),
        target_branch: preview.baseline.target_branch.clone(),
        source_branch: preview.baseline.source_branch.clone(),
        commit_sha: String::new(),
        commit_message: commit_message.to_string(),
        conflict_files: Vec::new(),
        error_reason: Some(error_reason.to_string()),
        baseline: preview.baseline.clone(),
        recovery_suggestions: merge_recovery_suggestions("failed"),
        started_at: now_text(),
        recorded_at: now_text(),
    };
    write_json_file(&record_path, &record)?;
    let record_path_text = record_path.to_string_lossy().to_string();
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let updated = store
        .connection()
        .execute(
            "UPDATE merge_records
             SET status = 'failed', error_reason = ?2, record_path = ?3
             WHERE id = ?1 AND task_id = ?4 AND status = 'started'",
            rusqlite::params![attempt_id, error_reason, record_path_text, task_id],
        )
        .map_err(storage_error)?;
    if updated != 1 {
        return Err(CommandError::new(
            "merge.attemptStateChanged",
            "The persisted merge attempt changed unexpectedly; CodeMax will not overwrite its recorded outcome.",
        ));
    }
    Ok(())
}

fn stale_preview_revalidation_error(cause: &CommandError) -> CommandError {
    CommandError::new(
        "merge.previewStale",
        format!(
            "The merge baseline could not be revalidated ({}). The old preview, confirmation, approvals, and Quality Gate results were invalidated. CodeMax did not start a target merge.",
            cause.message
        ),
    )
}

fn merge_outcome_persistence_error(status: &TaskMergeStatus, _cause: CommandError) -> CommandError {
    let outcome = match status {
        TaskMergeStatus::Merged => "Git completed and verified the merge",
        TaskMergeStatus::Conflicted => {
            "Git reported conflicts and CodeMax restored the clean target baseline"
        }
    };
    CommandError::new(
        "merge.outcomePersistenceFailed",
        format!(
            "{outcome}, but CodeMax could not finish persisting the outcome. Do not retry blindly: inspect the target repository and the task's merge record, then reopen the task for recovery."
        ),
    )
}

fn stale_preview_error(changed_fields: &[String]) -> CommandError {
    let detail = if changed_fields.is_empty() {
        String::new()
    } else {
        format!(" Changed baseline fields: {}.", changed_fields.join(", "))
    };
    CommandError::new(
        "merge.previewStale",
        format!(
            "The merge preview is stale and its confirmation cannot be reused.{detail} CodeMax did not change the target repository. Reopen the preview, rerun invalidated gates or approvals, and confirm again."
        ),
    )
}

fn merge_recovery_suggestions(status: &str) -> Vec<String> {
    match status {
        "merged" => vec![
            "Verify the recorded target commit before removing the task worktree.".to_string(),
        ],
        "conflicted" => vec![
            "The target merge was aborted and restored; no success was recorded.".to_string(),
            "Resolve the listed files in the task worktree or update the target branch, then create a new preview.".to_string(),
        ],
        "started" => vec![
            "Do not click merge again while this persisted attempt is running.".to_string(),
            "After a crash, inspect repository status and this record before preparing a new preview.".to_string(),
        ],
        _ => vec![
            "Inspect both repository and task worktree status; CodeMax never runs stash, reset, clean, or force checkout automatically.".to_string(),
            "Preserve user changes, address the reported cause, and create a new merge preview.".to_string(),
        ],
    }
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    let parent = path.parent().ok_or_else(|| {
        CommandError::new(
            "merge.invalidRecordPath",
            "The merge record path does not have a writable parent directory.",
        )
    })?;
    fs::create_dir_all(parent).map_err(storage_error)?;
    let bytes = serde_json::to_vec_pretty(value).map_err(json_error)?;
    let temporary_path = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("merge-record"),
        Uuid::new_v4()
    ));
    fs::write(&temporary_path, bytes).map_err(storage_error)?;
    match replace_json_file(&temporary_path, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary_path);
            Err(storage_error(error))
        }
    }
}

#[cfg(windows)]
fn replace_json_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let moved = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_json_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
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
        GitError::WorktreeRepositoryMismatch { worktree } => CommandError::new(
            "merge.worktreeRepositoryMismatch",
            format!(
                "Task worktree belongs to a different Git repository: {}",
                worktree.to_string_lossy()
            ),
        ),
        GitError::MergeBaselineChanged { changed_fields } => CommandError::new(
            "merge.previewStale",
            format!(
                "The merge baseline changed ({}). CodeMax did not merge the target. Create a new preview and confirm again.",
                changed_fields.join(", ")
            ),
        ),
        GitError::WorktreeChangedDuringMerge(path) => CommandError::new(
            "merge.worktreeChangedDuringMerge",
            format!(
                "The task worktree changed during merge preparation: {}. The target branch was not merged; review the preserved task changes and create a new preview.",
                path.to_string_lossy()
            ),
        ),
        GitError::MergeVerificationFailed(message) => CommandError::new(
            "merge.verificationFailed",
            format!("Git merge result verification failed: {message}"),
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

fn record_merge_event(
    storage: &ManagedStorage,
    task_id: &str,
    event_type: &str,
    stage: &str,
    message: &str,
    payload: Value,
) -> AppResult<()> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let event_id = format!("event-{}", Uuid::new_v4());
    let payload = serde_json::to_string(&payload).map_err(json_error)?;
    AgentEventRepository::new(store.connection())
        .create(NewAgentEvent {
            event_id: &event_id,
            task_id,
            event_type,
            stage,
            message,
            payload: &payload,
        })
        .map_err(storage_error)?;
    Ok(())
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
            purpose: "validation".to_string(),
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
        let blockers = merge_blockers(true, "");

        assert!(blockers.iter().any(|blocker| blocker.contains("target")));
        assert!(blockers
            .iter()
            .any(|blocker| blocker.contains("commit message")));

        assert!(merge_blockers(false, "feat: merge task").is_empty());
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

    fn test_git_stdout(path: &Path, args: &[&str]) -> String {
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
        String::from_utf8_lossy(&output.stdout).trim().to_string()
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
        target_branch: &str,
    ) -> (ManagedStorage, PathBuf) {
        let root = temp_path("storage");
        let store = SqliteStore::open_in_memory().expect("open sqlite");
        store.migrate().expect("run migrations");
        {
            let connection = store.connection();
            let allowed_paths_json =
                serde_json::to_string(&vec![worktree_path.to_string_lossy().to_string()])
                    .expect("serialize allowed paths");
            let allowed_commands_json =
                serde_json::to_string(&vec!["cargo test"]).expect("serialize allowed commands");
            let contract_json = json!({
                "mode": "agent",
                "validationCommand": "cargo test",
                "allowedPaths": [worktree_path.to_string_lossy().to_string()],
                "allowedCommands": ["cargo test"],
            })
            .to_string();
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
                    target_branch,
                    workspace_kind: "git_worktree",
                    source_path: &repository_path.to_string_lossy(),
                    original_write_authorized: false,
                    workspace_estimated_bytes: 0,
                    model_id: None,
                })
                .expect("create merge task");
            crate::storage::RunContractRepository::new(connection)
                .upsert(crate::storage::NewRunContract {
                    id: "contract-merge-ready",
                    task_id: "task-merge-error",
                    profile_id: None,
                    mode: "agent",
                    model_id: Some("model-default"),
                    reasoning_effort: "balanced",
                    permission_level: "workspace-write",
                    network_policy: "restricted",
                    allowed_paths_json: &allowed_paths_json,
                    allowed_commands_json: &allowed_commands_json,
                    validation_command: Some("cargo test"),
                    token_budget_total: 4000,
                    token_budget_per_call: 1200,
                    output_language: "zh-CN",
                    memory_scope: "task",
                    budget_overflow_policy: "pause_for_approval",
                    contract_json: &contract_json,
                })
                .expect("record merge run contract");
            CommandRunRepository::new(connection)
                .record(NewCommandRun {
                    id: "run-merge-passed",
                    task_id: "task-merge-error",
                    purpose: "validation",
                    command: "cargo test",
                    cwd: &worktree_path.to_string_lossy(),
                    status: "passed",
                    stdout_path: None,
                    stderr_path: None,
                    exit_code: Some(0),
                    duration_ms: Some(1000),
                })
                .expect("record passing validation");
            ArtifactRepository::new(connection)
                .record_artifact(NewArtifact {
                    id: "artifact-merge-ready",
                    task_id: "task-merge-error",
                    changed_files: "[\"modified.txt\"]",
                    diff_path: Some("proof/task-merge-error/diff.patch"),
                    test_report_path: Some("proof/task-merge-error/report.json"),
                    screenshots: "[]",
                    summary: "Merge error fixture is ready for precheck.",
                    commit_message: "feat: merge task",
                })
                .expect("record merge artifact");
            connection
                .execute(
                    "INSERT INTO proof_packs (
                        id, task_id, summary, proof_dir, export_path, delivery_score, risk_level, created_at
                     ) VALUES (?1, ?2, ?3, ?4, NULL, 90, 'low', ?5)",
                    rusqlite::params![
                        "proof-pack-merge-ready",
                        "task-merge-error",
                        "Merge error fixture proof pack.",
                        "proof/task-merge-error",
                        "2026-07-04T12:00:00Z",
                    ],
                )
                .expect("record proof pack index");
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
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(worktree_path.join("modified.txt"), "after").expect("modify worktree file");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );

        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare merge before target branch changes");
        run_test_git(&repository, &["checkout", "release"]);

        let error = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: prepared.preview_id,
                confirmed: true,
            },
        )
        .expect_err("target branch mismatch should fail");

        assert_eq!(error.code, "merge.previewStale");
        let invalidated = load_latest_merge_preview(&storage, "task-merge-error")
            .expect("load invalidated preview")
            .expect("preview exists");
        assert_eq!(invalidated.status, "invalidated");
        assert!(invalidated.invalidated_at.is_some());
        let stored_task = load_task(&storage, "task-merge-error").expect("load task");
        assert_eq!(stored_task.status, "awaitingReview");
        assert_eq!(
            fs::read_to_string(worktree_path.join("modified.txt"))
                .expect("read preserved task change"),
            "after"
        );

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn merge_preview_rejects_repository_checkout_changes_without_switching_branches() {
        let repository = committed_repository("saved-target-branch");
        let worktree_root = temp_path("saved-target-worktree-root");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read saved target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(worktree_path.join("modified.txt"), "after").expect("modify worktree file");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        run_test_git(&repository, &["checkout", "release"]);
        let release_head = test_git_stdout(&repository, &["rev-parse", "HEAD"]);

        let error = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect_err("a checkout away from the saved target must invalidate merge preparation");

        assert_eq!(error.code, "merge.targetBranchChanged");
        assert_eq!(
            git::current_branch(&repository).expect("read unchanged repository branch"),
            "release"
        );
        assert_eq!(
            test_git_stdout(&repository, &["rev-parse", "HEAD"]),
            release_head
        );
        assert_eq!(
            fs::read_to_string(worktree_path.join("modified.txt"))
                .expect("read preserved task change"),
            "after"
        );

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn baseline_change_invalidates_preview_approval_and_latest_quality_gate() {
        let repository = committed_repository("baseline-invalidation");
        let worktree_root = temp_path("baseline-invalidation-worktrees");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(worktree_path.join("modified.txt"), "previewed").expect("write previewed change");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        {
            let store = storage.store.lock().expect("lock storage");
            store
                .connection()
                .execute(
                    "INSERT INTO approvals
                 (id, task_id, type, risk_level, content, reason, decision, created_at, decided_at)
                 VALUES ('approval-merge-ready', 'task-merge-error', 'merge', 'high',
                         'Approve merge preview', 'User reviewed merge', 'approved', '1', '2')",
                    [],
                )
                .expect("insert approved merge approval");
            store.connection().execute(
                "INSERT INTO quality_gate_results
                 (id, task_id, gate_type, status, message, evidence_path, override_reason, created_at)
                 VALUES ('gate-merge-ready', 'task-merge-error', 'mergeSafety', 'passed',
                         'Merge safety passed', NULL, NULL, '3')",
                [],
            ).expect("insert passing quality gate");
        }
        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare merge");
        fs::write(worktree_path.join("modified.txt"), "changed after preview")
            .expect("change task content after preview");

        let error = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: prepared.preview_id,
                confirmed: true,
            },
        )
        .expect_err("changed diff must invalidate confirmation");
        assert_eq!(error.code, "merge.previewStale");
        let invalidated = load_latest_merge_preview(&storage, "task-merge-error")
            .expect("load preview")
            .expect("preview exists");
        assert_eq!(invalidated.status, "invalidated");
        let store = storage.store.lock().expect("lock storage");
        let invalidated_at: Option<String> = store
            .connection()
            .query_row(
                "SELECT invalidated_at FROM approvals WHERE id = 'approval-merge-ready'",
                [],
                |row| row.get(0),
            )
            .expect("read invalidated approval");
        assert!(invalidated_at.is_some());
        let latest_gate: String = store
            .connection()
            .query_row(
                "SELECT status FROM quality_gate_results
             WHERE task_id = 'task-merge-error' AND gate_type = 'mergeSafety'
             ORDER BY rowid DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("read latest quality gate");
        assert_eq!(latest_gate, "invalidated");
        drop(store);
        assert_eq!(
            fs::read_to_string(worktree_path.join("modified.txt")).unwrap(),
            "changed after preview"
        );
        assert!(test_git_stdout(&repository, &["status", "--porcelain"]).is_empty());

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn cleared_worktree_binding_invalidates_preview_without_deleting_the_worktree() {
        let repository = committed_repository("cleared-worktree-binding");
        let worktree_root = temp_path("cleared-worktree-binding-root");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(
            worktree_path.join("modified.txt"),
            "preserve cleared binding change",
        )
        .expect("write task change");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare merge");
        let target_head_before = test_git_stdout(&repository, &["rev-parse", "HEAD"]);
        storage
            .store
            .lock()
            .expect("lock storage")
            .connection()
            .execute(
                "UPDATE tasks SET worktree_path = NULL WHERE id = ?1",
                rusqlite::params!["task-merge-error"],
            )
            .expect("clear saved worktree binding");

        let error = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: prepared.preview_id,
                confirmed: true,
            },
        )
        .expect_err("cleared worktree binding must invalidate the preview");

        assert_eq!(error.code, "merge.previewStale");
        let invalidated = load_latest_merge_preview(&storage, "task-merge-error")
            .expect("load invalidated preview")
            .expect("preview exists");
        assert_eq!(invalidated.status, "invalidated");
        assert_eq!(
            test_git_stdout(&repository, &["rev-parse", "HEAD"]),
            target_head_before
        );
        assert_eq!(
            fs::read_to_string(worktree_path.join("modified.txt"))
                .expect("read preserved task change"),
            "preserve cleared binding change"
        );

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn saved_task_branch_binding_change_invalidates_preview_without_merging() {
        let repository = committed_repository("saved-task-branch-binding");
        let worktree_root = temp_path("saved-task-branch-binding-root");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(
            worktree_path.join("modified.txt"),
            "preserve branch-bound change",
        )
        .expect("write task change");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare merge");
        let target_head_before = test_git_stdout(&repository, &["rev-parse", "HEAD"]);
        storage
            .store
            .lock()
            .expect("lock storage")
            .connection()
            .execute(
                "UPDATE tasks SET branch_name = ?2 WHERE id = ?1",
                rusqlite::params!["task-merge-error", "codemax/task-rebound"],
            )
            .expect("change saved task branch binding");

        let error = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: prepared.preview_id,
                confirmed: true,
            },
        )
        .expect_err("saved task branch change must invalidate the preview");

        assert_eq!(error.code, "merge.previewStale");
        let invalidated = load_latest_merge_preview(&storage, "task-merge-error")
            .expect("load invalidated preview")
            .expect("preview exists");
        assert_eq!(invalidated.status, "invalidated");
        assert_eq!(
            test_git_stdout(&repository, &["rev-parse", "HEAD"]),
            target_head_before
        );
        assert_eq!(
            fs::read_to_string(worktree_path.join("modified.txt"))
                .expect("read preserved task change"),
            "preserve branch-bound change"
        );

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn moved_worktree_invalidates_preview_without_deleting_user_changes() {
        let repository = committed_repository("moved-worktree-invalidation");
        let worktree_root = temp_path("moved-worktree-invalidation-root");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        let moved_worktree_path = worktree_root.join("moved-by-user");
        fs::write(
            worktree_path.join("modified.txt"),
            "preserve moved worktree change",
        )
        .expect("write task change");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare merge");
        let target_head = test_git_stdout(&repository, &["rev-parse", "HEAD"]);
        fs::rename(&worktree_path, &moved_worktree_path).expect("simulate moved worktree");

        let error = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: prepared.preview_id,
                confirmed: true,
            },
        )
        .expect_err("a moved worktree must invalidate the saved preview");

        assert_eq!(error.code, "merge.previewStale");
        let invalidated = load_latest_merge_preview(&storage, "task-merge-error")
            .expect("load invalidated preview")
            .expect("preview exists");
        assert_eq!(invalidated.status, "invalidated");
        assert!(invalidated.invalidated_at.is_some());
        assert_eq!(
            load_task(&storage, "task-merge-error")
                .expect("load task")
                .status,
            "awaitingReview"
        );
        assert_eq!(
            fs::read_to_string(moved_worktree_path.join("modified.txt"))
                .expect("read preserved moved worktree change"),
            "preserve moved worktree change"
        );
        assert_eq!(
            test_git_stdout(&repository, &["rev-parse", "HEAD"]),
            target_head
        );
        assert!(test_git_stdout(&repository, &["status", "--porcelain"]).is_empty());

        fs::remove_dir_all(moved_worktree_path).expect("clean moved worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn task_head_change_invalidates_preview_without_merging_or_rewriting_the_task_branch() {
        let repository = committed_repository("task-head-invalidation");
        let worktree_root = temp_path("task-head-invalidation-root");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(
            worktree_path.join("modified.txt"),
            "committed after preview",
        )
        .expect("write task change");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare merge");
        let target_head = test_git_stdout(&repository, &["rev-parse", "HEAD"]);
        run_test_git(&worktree_path, &["add", "modified.txt"]);
        run_test_git(
            &worktree_path,
            &["commit", "-m", "user commit after preview"],
        );
        let user_task_head = test_git_stdout(&worktree_path, &["rev-parse", "HEAD"]);

        let error = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: prepared.preview_id,
                confirmed: true,
            },
        )
        .expect_err("a changed task HEAD must invalidate the saved preview");

        assert_eq!(error.code, "merge.previewStale");
        assert_eq!(
            load_latest_merge_preview(&storage, "task-merge-error")
                .expect("load invalidated preview")
                .expect("preview exists")
                .status,
            "invalidated"
        );
        assert_eq!(
            test_git_stdout(&repository, &["rev-parse", "HEAD"]),
            target_head
        );
        assert_eq!(
            test_git_stdout(&worktree_path, &["rev-parse", "HEAD"]),
            user_task_head
        );
        assert_eq!(
            fs::read_to_string(worktree_path.join("modified.txt"))
                .expect("read preserved task content"),
            "committed after preview"
        );

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn concurrent_attempt_lock_blocks_a_second_merge_without_touching_user_state() {
        let repository = committed_repository("attempt-lock-command");
        let worktree_root = temp_path("attempt-lock-command-worktrees");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(worktree_path.join("modified.txt"), "protected while locked")
            .expect("write task change");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare merge");
        let target_head_before = test_git_stdout(&repository, &["rev-parse", "HEAD"]);
        let attempt_id = merge_attempt_id("task-merge-error", &prepared.preview_id);
        let attempt_lock = acquire_merge_attempt_lock(&storage, "task-merge-error", &attempt_id)
            .expect("hold merge attempt lock");

        let error = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: prepared.preview_id.clone(),
                confirmed: true,
            },
        )
        .expect_err("concurrent merge must be rejected while the attempt lock is held");

        assert_eq!(error.code, "merge.inProgress");
        assert_eq!(
            test_git_stdout(&repository, &["rev-parse", "HEAD"]),
            target_head_before
        );
        assert_eq!(
            fs::read_to_string(worktree_path.join("modified.txt"))
                .expect("read protected task change"),
            "protected while locked"
        );
        let attempt_count: i64 = storage
            .store
            .lock()
            .expect("lock storage")
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM merge_records WHERE task_id = 'task-merge-error'",
                [],
                |row| row.get(0),
            )
            .expect("count blocked attempts");
        assert_eq!(attempt_count, 0);

        drop(attempt_lock);
        let merged = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: prepared.preview_id,
                confirmed: true,
            },
        )
        .expect("merge succeeds after the attempt lock is released");
        assert_eq!(merged.status, TaskMergeStatus::Merged);

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn duplicate_merge_request_returns_one_persisted_outcome_without_second_commit() {
        let repository = committed_repository("idempotent-command");
        let worktree_root = temp_path("idempotent-command-worktrees");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(worktree_path.join("modified.txt"), "merged once").expect("write task change");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare merge");
        let preview_id = prepared.preview_id.clone();
        let first = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: preview_id.clone(),
                confirmed: true,
            },
        )
        .expect("first merge succeeds");
        let head_after_first = test_git_stdout(&repository, &["rev-parse", "HEAD"]);
        let second = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: preview_id.clone(),
                confirmed: true,
            },
        )
        .expect("duplicate request replays persisted result");
        assert_eq!(first.status, TaskMergeStatus::Merged);
        assert_eq!(second.status, TaskMergeStatus::Merged);
        assert_eq!(first.commit_sha, second.commit_sha);
        assert_eq!(
            test_git_stdout(&repository, &["rev-parse", "HEAD"]),
            head_after_first
        );
        let attempt_count: i64 = storage
            .store
            .lock()
            .expect("lock storage")
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM merge_records WHERE task_id = 'task-merge-error'",
                [],
                |row| row.get(0),
            )
            .expect("count attempts");
        assert_eq!(attempt_count, 1);

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn conflicted_command_is_persisted_as_conflicted_and_restores_target() {
        let repository = committed_repository("conflicted-command");
        let worktree_root = temp_path("conflicted-command-worktrees");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(repository.join("modified.txt"), "target version").expect("write target change");
        run_test_git(&repository, &["add", "."]);
        run_test_git(&repository, &["commit", "-m", "target conflict"]);
        fs::write(worktree_path.join("modified.txt"), "task version").expect("write task conflict");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare conflicting merge");
        let target_head = prepared.target_head.clone();
        let result = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: prepared.preview_id,
                confirmed: true,
            },
        )
        .expect("conflict is a persisted non-success result");
        assert_eq!(result.status, TaskMergeStatus::Conflicted);
        assert!(result.commit_sha.is_empty());
        assert_eq!(result.task_status, "needsIntervention");
        assert_eq!(
            test_git_stdout(&repository, &["rev-parse", "HEAD"]),
            target_head
        );
        assert!(test_git_stdout(&repository, &["status", "--porcelain"]).is_empty());
        let stored_status: String = storage
            .store
            .lock()
            .expect("lock storage")
            .connection()
            .query_row(
                "SELECT status FROM merge_records WHERE task_id = 'task-merge-error'",
                [],
                |row| row.get(0),
            )
            .expect("read merge status");
        assert_eq!(stored_status, "conflicted");

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn interrupted_started_attempt_at_clean_baseline_requires_a_fresh_preview_then_can_retry() {
        let repository = committed_repository("interrupted-started-clean");
        let worktree_root = temp_path("interrupted-started-clean-worktrees");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(
            worktree_path.join("modified.txt"),
            "retry after interrupted start",
        )
        .expect("write task change");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare merge");
        let repeated_preview = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("repeat unchanged preview");
        assert_eq!(repeated_preview.preview_id, prepared.preview_id);
        let preview = load_latest_merge_preview(&storage, "task-merge-error")
            .expect("load preview")
            .expect("preview exists");
        let attempt_id = merge_attempt_id("task-merge-error", &prepared.preview_id);
        start_merge_attempt(
            &storage,
            &attempt_id,
            "task-merge-error",
            &preview,
            "feat: interrupted start",
        )
        .expect("persist interrupted started attempt");
        let target_head = test_git_stdout(&repository, &["rev-parse", "HEAD"]);

        let error = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: interrupted start".to_string(),
                preview_id: prepared.preview_id.clone(),
                confirmed: true,
            },
        )
        .expect_err("interrupted start should be closed as failed before retry");
        assert_eq!(error.code, "merge.previousAttemptFailed");
        assert_eq!(
            test_git_stdout(&repository, &["rev-parse", "HEAD"]),
            target_head
        );
        assert_eq!(
            fs::read_to_string(worktree_path.join("modified.txt"))
                .expect("read preserved task change"),
            "retry after interrupted start"
        );

        let refreshed = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare a fresh retry preview");
        assert_ne!(refreshed.preview_id, prepared.preview_id);
        let merged = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: retry interrupted start".to_string(),
                preview_id: refreshed.preview_id,
                confirmed: true,
            },
        )
        .expect("fresh preview retries safely");
        assert_eq!(merged.status, TaskMergeStatus::Merged);

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn interrupted_started_attempt_recovers_exact_verified_git_merge_without_repeating_it() {
        let repository = committed_repository("interrupted-started-merged");
        let worktree_root = temp_path("interrupted-started-merged-worktrees");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(
            worktree_path.join("modified.txt"),
            "recover exact git merge",
        )
        .expect("write task change");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare merge");
        let preview = load_latest_merge_preview(&storage, "task-merge-error")
            .expect("load preview")
            .expect("preview exists");
        let attempt_id = merge_attempt_id("task-merge-error", &prepared.preview_id);
        start_merge_attempt(
            &storage,
            &attempt_id,
            "task-merge-error",
            &preview,
            "feat: interrupted exact merge",
        )
        .expect("persist started attempt");
        let git_result = git::merge_task_branch(
            "task-merge-error",
            &repository,
            &worktree_path,
            &target_branch,
            "feat: interrupted exact merge",
            &preview.baseline,
        )
        .expect("perform git merge before simulated crash");
        assert_eq!(git_result.status, TaskMergeStatus::Merged);
        let merged_head = test_git_stdout(&repository, &["rev-parse", "HEAD"]);

        let recovered = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: interrupted exact merge".to_string(),
                preview_id: prepared.preview_id,
                confirmed: true,
            },
        )
        .expect("reconcile exact merge without repeating git");
        assert_eq!(recovered.status, TaskMergeStatus::Merged);
        assert_eq!(recovered.commit_sha, merged_head);
        assert_eq!(
            test_git_stdout(&repository, &["rev-parse", "HEAD"]),
            merged_head
        );
        let record_count: i64 = storage
            .store
            .lock()
            .expect("lock storage")
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM merge_records WHERE task_id = 'task-merge-error'",
                [],
                |row| row.get(0),
            )
            .expect("count merge records");
        assert_eq!(record_count, 1);

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn interrupted_final_record_is_reconciled_without_repeating_git_merge() {
        let repository = committed_repository("reconcile-command");
        let worktree_root = temp_path("reconcile-command-worktrees");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(worktree_path.join("modified.txt"), "reconciled merge")
            .expect("write task change");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare merge");
        let preview = load_latest_merge_preview(&storage, "task-merge-error")
            .expect("load preview")
            .expect("preview exists");
        let attempt_id = merge_attempt_id("task-merge-error", &prepared.preview_id);
        start_merge_attempt(
            &storage,
            &attempt_id,
            "task-merge-error",
            &preview,
            "feat: merge task",
        )
        .expect("persist started attempt");
        let git_result = git::merge_task_branch(
            "task-merge-error",
            &repository,
            &worktree_path,
            &target_branch,
            "feat: merge task",
            &preview.baseline,
        )
        .expect("perform Git merge before simulated crash");
        let head_after_git = test_git_stdout(&repository, &["rev-parse", "HEAD"]);
        let record_path = storage
            .roots
            .ensure_task_artifact_dirs("task-merge-error")
            .expect("create artifact dirs")
            .artifacts_dir
            .join(format!("merge-record-{}.json", preview.preview_id));
        let final_record = TaskMergeRecord {
            task_id: "task-merge-error".to_string(),
            preview_id: preview.preview_id.clone(),
            status: "merged".to_string(),
            target_branch: git_result.target_branch.clone(),
            source_branch: git_result.source_branch.clone(),
            commit_sha: git_result.commit_sha.clone(),
            commit_message: git_result.commit_message.clone(),
            conflict_files: Vec::new(),
            error_reason: None,
            baseline: preview.baseline.clone(),
            recovery_suggestions: merge_recovery_suggestions("merged"),
            started_at: now_text(),
            recorded_at: now_text(),
        };
        write_json_file(&record_path, &final_record)
            .expect("persist final JSON before simulated DB crash");

        let replay = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: prepared.preview_id,
                confirmed: true,
            },
        )
        .expect("reconcile final file instead of repeating Git");
        assert_eq!(replay.status, TaskMergeStatus::Merged);
        assert_eq!(replay.commit_sha, git_result.commit_sha);
        assert_eq!(
            test_git_stdout(&repository, &["rev-parse", "HEAD"]),
            head_after_git
        );
        let (attempt_status, task_status): (String, String) = {
            let store = storage.store.lock().expect("lock storage");
            let attempt_status = store
                .connection()
                .query_row(
                    "SELECT status FROM merge_records WHERE id = ?1",
                    [&attempt_id],
                    |row| row.get(0),
                )
                .expect("read reconciled attempt");
            let task_status = store
                .connection()
                .query_row(
                    "SELECT status FROM tasks WHERE id = 'task-merge-error'",
                    [],
                    |row| row.get(0),
                )
                .expect("read reconciled task");
            (attempt_status, task_status)
        };
        assert_eq!(attempt_status, "merged");
        assert_eq!(task_status, "merged");

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }

    #[test]
    fn auxiliary_artifact_index_failure_does_not_misreport_verified_merge_as_failed() {
        let repository = committed_repository("artifact-index-warning");
        let worktree_root = temp_path("artifact-index-warning-worktrees");
        let worktree = git::create_task_worktree(&repository, &worktree_root, "task-merge-error")
            .expect("create task worktree");
        let target_branch = git::current_branch(&repository).expect("read target branch");
        let worktree_path = PathBuf::from(&worktree.worktree_path);
        fs::write(
            worktree_path.join("modified.txt"),
            "merge despite index warning",
        )
        .expect("write task change");
        let (storage, storage_root) = merge_test_storage(
            &repository,
            &worktree_path,
            &worktree.branch_name,
            &target_branch,
        );
        let prepared = prepare_task_merge_inner(
            &storage,
            PrepareTaskMergeRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
            },
        )
        .expect("prepare merge");
        storage
            .store
            .lock()
            .expect("lock storage")
            .connection()
            .execute_batch(
                "CREATE TRIGGER fail_merge_artifact_file
                 BEFORE INSERT ON artifact_files
                 WHEN NEW.type = 'merge_record'
                 BEGIN
                     SELECT RAISE(FAIL, 'simulated merge artifact index failure');
                 END;",
            )
            .expect("simulate auxiliary index failure");
        let result = merge_task_inner(
            &storage,
            MergeTaskRequest {
                task_id: "task-merge-error".to_string(),
                target_branch: None,
                commit_message: "feat: merge task".to_string(),
                preview_id: prepared.preview_id,
                confirmed: true,
            },
        )
        .expect("verified merge remains successful when auxiliary index fails");
        assert_eq!(result.status, TaskMergeStatus::Merged);
        let stored_status: String = storage
            .store
            .lock()
            .expect("lock storage")
            .connection()
            .query_row(
                "SELECT status FROM merge_records WHERE task_id = 'task-merge-error'",
                [],
                |row| row.get(0),
            )
            .expect("read core merge outcome");
        assert_eq!(stored_status, "merged");

        fs::remove_dir_all(worktree_path).expect("clean worktree");
        fs::remove_dir_all(worktree_root).expect("clean worktree root");
        fs::remove_dir_all(repository).expect("clean repository");
        if storage_root.exists() {
            fs::remove_dir_all(storage_root).expect("clean storage root");
        }
    }
}
