use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;
use uuid::Uuid;

use crate::{
    core::error::{AppResult, CommandError},
    storage::{
        AgentEventRepository, ArtifactRecord, ArtifactRepository, CommandRunRecord,
        CommandRunRepository, ManagedStorage, NewAgentEvent, NewArtifact, NewArtifactFile,
        StorageError, TaskRecord, TaskRepository,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateTaskDeliveryRequest {
    pub task_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedTaskDelivery {
    pub task_id: String,
    pub artifact_id: String,
    pub report_path: String,
    pub delivery_path: String,
    pub diff_path: Option<String>,
    pub summary: String,
    pub commit_message: String,
    pub report: TaskDeliveryReport,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDeliveryReport {
    pub task_id: String,
    pub artifact_id: String,
    pub task_title: String,
    pub generated_at: String,
    pub overall_status: String,
    pub summary: String,
    pub command_count: usize,
    pub passed_count: usize,
    pub failed_count: usize,
    pub changed_files: Vec<String>,
    pub diff_path: Option<String>,
    pub delivery_path: String,
    pub runs: Vec<TaskValidationRunSummary>,
    pub risk: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskValidationRunSummary {
    pub run_id: String,
    pub purpose: String,
    pub command: String,
    pub cwd: String,
    pub status: String,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub created_at: String,
}

#[tauri::command]
pub fn generate_task_delivery(
    storage: State<'_, ManagedStorage>,
    request: GenerateTaskDeliveryRequest,
) -> AppResult<GeneratedTaskDelivery> {
    generate_task_delivery_inner(&storage, request)
}

pub(crate) fn generate_task_delivery_inner(
    storage: &ManagedStorage,
    request: GenerateTaskDeliveryRequest,
) -> AppResult<GeneratedTaskDelivery> {
    let task_id = request.task_id.trim().to_string();
    if task_id.is_empty() {
        return Err(CommandError::new(
            "delivery.taskIdRequired",
            "Task id is required to generate a delivery report.",
        ));
    }

    let (task, command_runs, artifacts) = load_delivery_inputs(storage, &task_id)?;
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(&task.id)
        .map_err(storage_error)?;
    let artifact_id = format!("delivery-{}-{}", task.id, Uuid::new_v4());
    let generated_at = now_text();
    let latest_diff_artifact = latest_diff_artifact(&artifacts);
    let diff_path = latest_diff_artifact
        .and_then(|artifact| artifact.diff_path.clone())
        .or_else(|| {
            paths
                .diff_path
                .is_file()
                .then(|| paths.diff_path.to_string_lossy().to_string())
        });
    let changed_files = changed_files_from_artifacts(latest_diff_artifact);
    let runs = latest_validation_runs(command_runs)
        .into_iter()
        .map(validation_run_summary)
        .collect::<Vec<_>>();
    let command_count = runs.len();
    let passed_count = runs.iter().filter(|run| validation_run_passed(run)).count();
    let failed_count = command_count.saturating_sub(passed_count);
    let overall_status = overall_status(command_count, failed_count).to_string();
    let risk = risk_summary(&overall_status);
    let report_summary = report_summary(&overall_status, command_count, passed_count, failed_count);
    let summary = delivery_summary(&task, &changed_files, &runs, &report_summary, &risk);
    let commit_message = commit_message(&task, &report_summary, &risk);
    let report_path = paths.report_path.to_string_lossy().to_string();
    let delivery_path_buf = paths.artifacts_dir.join("delivery.md");
    let delivery_path = delivery_path_buf.to_string_lossy().to_string();

    let report = TaskDeliveryReport {
        task_id: task.id.clone(),
        artifact_id: artifact_id.clone(),
        task_title: task.title.clone(),
        generated_at,
        overall_status,
        summary: report_summary,
        command_count,
        passed_count,
        failed_count,
        changed_files: changed_files.clone(),
        diff_path: diff_path.clone(),
        delivery_path: delivery_path.clone(),
        runs,
        risk,
    };

    write_delivery_files(
        &paths.report_path,
        &delivery_path_buf,
        &report,
        &summary,
        &commit_message,
    )?;
    record_delivery_artifact(
        storage,
        &task.id,
        &artifact_id,
        &changed_files,
        diff_path.as_deref(),
        &report_path,
        &delivery_path,
        &summary,
        &commit_message,
        &report.overall_status,
    )?;

    Ok(GeneratedTaskDelivery {
        task_id: task.id,
        artifact_id,
        report_path,
        delivery_path,
        diff_path,
        summary,
        commit_message,
        report,
    })
}

fn load_delivery_inputs(
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

fn latest_diff_artifact(artifacts: &[ArtifactRecord]) -> Option<&ArtifactRecord> {
    artifacts
        .iter()
        .rev()
        .find(|artifact| artifact.diff_path.is_some())
}

fn changed_files_from_artifacts(artifact: Option<&ArtifactRecord>) -> Vec<String> {
    let Some(artifact) = artifact else {
        return Vec::new();
    };

    let mut files = BTreeSet::new();
    if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&artifact.changed_files) {
        for item in items {
            if let Some(path) = item
                .get("path")
                .and_then(Value::as_str)
                .or_else(|| item.as_str())
            {
                let path = path.trim();
                if !path.is_empty() {
                    files.insert(path.to_string());
                }
            }
        }
    }

    files.into_iter().collect()
}

fn latest_validation_runs(runs: Vec<CommandRunRecord>) -> Vec<CommandRunRecord> {
    let mut latest: Vec<CommandRunRecord> = Vec::new();

    for run in runs
        .into_iter()
        .filter(|run| run.purpose == "validation")
    {
        let command = run.command.clone();
        let cwd = run.cwd.clone();
        if let Some(index) = latest
            .iter()
            .position(|existing| existing.command == command && existing.cwd == cwd)
        {
            latest[index] = run;
        } else {
            latest.push(run);
        }
    }

    latest
}

fn validation_run_summary(run: CommandRunRecord) -> TaskValidationRunSummary {
    TaskValidationRunSummary {
        run_id: run.id,
        purpose: run.purpose,
        command: run.command,
        cwd: run.cwd,
        status: run.status,
        exit_code: run.exit_code,
        duration_ms: run.duration_ms,
        created_at: run.created_at,
    }
}

fn validation_run_passed(run: &TaskValidationRunSummary) -> bool {
    run.status == "passed" && run.exit_code.unwrap_or(0) == 0
}

fn overall_status(command_count: usize, failed_count: usize) -> &'static str {
    if command_count == 0 {
        "notRun"
    } else if failed_count == 0 {
        "passed"
    } else {
        "failed"
    }
}

fn report_summary(
    overall_status: &str,
    command_count: usize,
    passed_count: usize,
    failed_count: usize,
) -> String {
    match overall_status {
        "passed" => format!("验证通过：共 {command_count} 条命令，{passed_count} 条通过。"),
        "failed" => format!("验证未通过：共 {command_count} 条命令，{failed_count} 条失败或中断。"),
        _ => "尚未记录验证命令，交付前需要补充验证结果。".to_string(),
    }
}

fn risk_summary(overall_status: &str) -> String {
    match overall_status {
        "passed" => "未发现失败验证命令；合入前仍建议按项目规范复跑关键验证。".to_string(),
        "failed" => "存在失败、超时或取消的验证命令；默认不建议进入合入。".to_string(),
        _ => "缺少验证命令记录；当前报告只能说明交付物已生成，不能证明代码通过验证。".to_string(),
    }
}

fn delivery_summary(
    task: &TaskRecord,
    changed_files: &[String],
    runs: &[TaskValidationRunSummary],
    report_summary: &str,
    risk: &str,
) -> String {
    let files = if changed_files.is_empty() {
        "暂无已记录文件改动。".to_string()
    } else {
        changed_files
            .iter()
            .take(12)
            .map(|file| format!("- {file}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let verification = if runs.is_empty() {
        "- 尚未记录验证命令。".to_string()
    } else {
        runs.iter()
            .map(|run| {
                format!(
                    "- {}：{}，退出码 {}，耗时 {}",
                    run.command,
                    run.status,
                    run.exit_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "无".to_string()),
                    run.duration_ms
                        .map(|duration| format!("{duration}ms"))
                        .unwrap_or_else(|| "未记录".to_string())
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "## 问题\n{}\n\n## 修改点\n本次围绕任务 `{}` 生成可审查交付物，汇总 Diff 文件、验证命令结果、测试摘要和建议提交信息。\n\n## 文件\n{}\n\n## 验证\n{}\n{}\n\n## 风险\n{}",
        task.description.trim().is_empty().then(|| task.title.as_str()).unwrap_or(task.description.as_str()),
        task.title,
        files,
        report_summary,
        verification,
        risk
    )
}

fn commit_message(task: &TaskRecord, report_summary: &str, risk: &str) -> String {
    let commit_type = match task.task_type.as_str() {
        "bugfix" => "fix",
        "test" => "test",
        "refactor" => "refactor",
        "explain" => "docs",
        _ => "feat",
    };

    format!(
        "{commit_type}(desktop): add task delivery report\n\n- Generate S8-E02 validation summary and delivery artifact.\n- Verification: {report_summary}\n- Risk: {risk}"
    )
}

fn write_delivery_files(
    report_path: &Path,
    delivery_path: &Path,
    report: &TaskDeliveryReport,
    summary: &str,
    commit_message: &str,
) -> AppResult<()> {
    if let Some(parent) = delivery_path.parent() {
        fs::create_dir_all(parent).map_err(storage_error)?;
    }

    let report_json = serde_json::to_string_pretty(report).map_err(json_error)?;
    fs::write(report_path, report_json).map_err(storage_error)?;
    fs::write(
        delivery_path,
        format!("{summary}\n\n## 建议 Commit Message\n\n```text\n{commit_message}\n```\n"),
    )
    .map_err(storage_error)?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_delivery_artifact(
    storage: &ManagedStorage,
    task_id: &str,
    artifact_id: &str,
    changed_files: &[String],
    diff_path: Option<&str>,
    report_path: &str,
    delivery_path: &str,
    summary: &str,
    commit_message: &str,
    overall_status: &str,
) -> AppResult<()> {
    let changed_files = serde_json::to_string(changed_files).map_err(json_error)?;
    let report_size = file_size(Path::new(report_path)).map_err(storage_error)?;
    let delivery_size = file_size(Path::new(delivery_path)).map_err(storage_error)?;
    let report_file_id = format!("file-report-{artifact_id}");
    let delivery_file_id = format!("file-delivery-{artifact_id}");
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let artifacts = ArtifactRepository::new(store.connection());

    artifacts
        .record_artifact(NewArtifact {
            id: artifact_id,
            task_id,
            changed_files: &changed_files,
            diff_path,
            test_report_path: Some(report_path),
            screenshots: "[]",
            summary,
            commit_message,
        })
        .map_err(storage_error)?;
    artifacts
        .record_file(NewArtifactFile {
            id: &report_file_id,
            task_id,
            artifact_id: Some(artifact_id),
            file_type: "test_report",
            path: report_path,
            size_bytes: report_size as i64,
            compressed: false,
            retention_class: "permanent",
            expires_at: None,
        })
        .map_err(storage_error)?;
    artifacts
        .record_file(NewArtifactFile {
            id: &delivery_file_id,
            task_id,
            artifact_id: Some(artifact_id),
            file_type: "delivery_summary",
            path: delivery_path,
            size_bytes: delivery_size as i64,
            compressed: false,
            retention_class: "permanent",
            expires_at: None,
        })
        .map_err(storage_error)?;
    TaskRepository::new(store.connection())
        .update_status(
            task_id,
            if overall_status == "passed" {
                "readyToMerge"
            } else {
                "awaitingReview"
            },
            None,
        )
        .map_err(storage_error)?;
    record_agent_event_with_connection(
        store.connection(),
        task_id,
        "delivery.ready",
        if overall_status == "passed" {
            "readyToMerge"
        } else {
            "awaitingReview"
        },
        "Delivery report was generated from task evidence.",
        json!({
            "artifact_id": artifact_id,
            "report_path": report_path,
            "delivery_path": delivery_path,
            "overall_status": overall_status,
        }),
    )?;

    Ok(())
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

fn json_error(error: serde_json::Error) -> CommandError {
    CommandError::new(
        "delivery.invalidJson",
        format!("Unable to encode delivery report: {error}"),
    )
}

fn record_agent_event_with_connection(
    connection: &rusqlite::Connection,
    task_id: &str,
    event_type: &str,
    stage: &str,
    message: &str,
    payload: Value,
) -> AppResult<()> {
    let event_id = format!("event-{}", Uuid::new_v4());
    let payload = serde_json::to_string(&payload).map_err(json_error)?;
    AgentEventRepository::new(connection)
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
