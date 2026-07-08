use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;
use uuid::Uuid;

use crate::{
    core::error::{AppResult, CommandError},
    storage::{
        AgentEventRepository, ApprovalRecord, ApprovalRepository, ManagedStorage, NewAgentEvent,
        StorageError, TaskRepository,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTaskApprovalsRequest {
    pub task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecideApprovalRequest {
    pub approval_id: String,
    pub decision: String,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalResponse {
    pub id: String,
    pub task_id: String,
    pub approval_type: String,
    pub risk_level: String,
    pub content: String,
    pub reason: String,
    pub decision: Option<String>,
    pub comment: Option<String>,
    pub created_at: String,
    pub decided_at: Option<String>,
}

#[tauri::command]
pub fn list_pending_approvals(
    storage: State<'_, ManagedStorage>,
) -> AppResult<Vec<ApprovalResponse>> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    ApprovalRepository::new(store.connection())
        .list_pending()
        .map(|approvals| approvals.into_iter().map(ApprovalResponse::from).collect())
        .map_err(storage_error)
}

#[tauri::command]
pub fn list_task_approvals(
    storage: State<'_, ManagedStorage>,
    request: ListTaskApprovalsRequest,
) -> AppResult<Vec<ApprovalResponse>> {
    let task_id = request.task_id.trim();
    if task_id.is_empty() {
        return Err(CommandError::new(
            "approval.taskIdRequired",
            "Task id is required to list approvals.",
        ));
    }

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    ApprovalRepository::new(store.connection())
        .list_for_task(task_id)
        .map(|approvals| approvals.into_iter().map(ApprovalResponse::from).collect())
        .map_err(storage_error)
}

#[tauri::command]
pub fn decide_approval(
    storage: State<'_, ManagedStorage>,
    request: DecideApprovalRequest,
) -> AppResult<ApprovalResponse> {
    let approval_id = request.approval_id.trim();
    if approval_id.is_empty() {
        return Err(CommandError::new(
            "approval.idRequired",
            "Approval id is required.",
        ));
    }

    let decision = normalize_decision(&request.decision)?;
    let comment = request
        .comment
        .as_deref()
        .map(str::trim)
        .filter(|comment| !comment.is_empty());
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    let approvals = ApprovalRepository::new(connection);
    let decided = approvals
        .decide(approval_id, decision, comment)
        .map_err(storage_error)?;

    if matches!(decision, "rejected" | "revise") {
        TaskRepository::new(connection)
            .update_status(&decided.task_id, "needsIntervention", None)
            .map_err(storage_error)?;
    }
    record_agent_event_with_connection(
        connection,
        &decided.task_id,
        "approval.resolved",
        if decision == "approved" {
            "editing"
        } else {
            "needsIntervention"
        },
        "Approval decision was recorded.",
        json!({
            "approval_id": &decided.id,
            "decision": decision,
            "comment": &decided.comment,
        }),
    )?;

    Ok(ApprovalResponse::from(decided))
}

fn normalize_decision(decision: &str) -> AppResult<&'static str> {
    match decision.trim() {
        "approved" => Ok("approved"),
        "rejected" => Ok("rejected"),
        "revise" => Ok("revise"),
        other => Err(CommandError::new(
            "approval.invalidDecision",
            format!("Unsupported approval decision: {other}"),
        )),
    }
}

impl From<ApprovalRecord> for ApprovalResponse {
    fn from(record: ApprovalRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            approval_type: record.approval_type,
            risk_level: record.risk_level,
            content: record.content,
            reason: record.reason,
            decision: record.decision,
            comment: record.comment,
            created_at: record.created_at,
            decided_at: record.decided_at,
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
        StorageError::NotFound(message) => CommandError::new("approval.notFound", message),
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

fn record_agent_event_with_connection(
    connection: &rusqlite::Connection,
    task_id: &str,
    event_type: &str,
    stage: &str,
    message: &str,
    payload: Value,
) -> AppResult<()> {
    let event_id = format!("event-{}", Uuid::new_v4());
    let payload = serde_json::to_string(&payload).map_err(|error| {
        CommandError::new(
            "event.invalidPayload",
            format!("Unable to encode event payload: {error}"),
        )
    })?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_supported_decisions_are_accepted() {
        assert_eq!(normalize_decision("approved").unwrap(), "approved");
        assert_eq!(normalize_decision("rejected").unwrap(), "rejected");
        assert_eq!(normalize_decision("revise").unwrap(), "revise");
        assert!(normalize_decision("skip").is_err());
    }
}
