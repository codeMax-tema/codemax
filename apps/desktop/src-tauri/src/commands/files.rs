use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;
use uuid::Uuid;

use crate::{
    core::error::{AppResult, CommandError},
    safe_fs::{self, SafeFileOperation, SafeFileOperationResult},
    storage::{
        AgentEventRepository, ApprovalAuthorization, ApprovalRepository, ManagedStorage,
        NewAgentEvent, NewBoundApproval, StorageError, TaskRepository,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteSafeFileOperationsRequest {
    pub task_id: String,
    pub request_id: String,
    pub operations: Vec<SafeFileOperation>,
    pub approval_id: Option<String>,
    pub diff_artifact_id: Option<String>,
    pub validation_round_id: Option<String>,
    pub proof_pack_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteSafeFileOperationsResponse {
    pub transaction_id: String,
    pub status: String,
    pub results: Vec<SafeFileOperationResult>,
}

#[derive(Debug)]
struct ExistingTransaction {
    transaction_id: String,
    request_digest: String,
    status: String,
    results_json: String,
}

#[tauri::command]
pub async fn execute_safe_file_operations(
    storage: State<'_, ManagedStorage>,
    request: ExecuteSafeFileOperationsRequest,
) -> AppResult<ExecuteSafeFileOperationsResponse> {
    execute_transaction(&storage, request)
}

pub(crate) fn execute_transaction(
    storage: &ManagedStorage,
    request: ExecuteSafeFileOperationsRequest,
) -> AppResult<ExecuteSafeFileOperationsResponse> {
    if request.request_id.trim().is_empty() {
        return Err(CommandError::new(
            "safeFile.requestIdRequired",
            "A stable request id is required.",
        ));
    }
    if request.operations.is_empty() {
        return Err(CommandError::new(
            "safeFile.emptyOperations",
            "At least one file operation is required.",
        ));
    }
    if request
        .operations
        .iter()
        .any(|operation| matches!(operation, SafeFileOperation::CreateDirectory { .. }))
    {
        return Err(CommandError::new(
            "safeFile.nonTransactionalOperation",
            "Directory creation is not accepted by the recoverable file-edit transaction.",
        ));
    }

    let operations_json = serde_json::to_string(&request.operations)
        .map_err(|error| CommandError::new("safeFile.invalidOperations", error.to_string()))?;
    let request_digest = digest(&operations_json);
    let transaction_id = Uuid::new_v4().to_string();

    let (workspace_path, inverse_operations) = {
        let store = storage.store.lock().map_err(|_| storage_busy())?;
        let task = TaskRepository::new(store.connection())
            .get_required(&request.task_id)
            .map_err(storage_error)?;
        let workspace_path = task.worktree_path.ok_or_else(|| {
            CommandError::new(
                "safeFile.worktreeUnavailable",
                "The task does not have an isolated worktree.",
            )
        })?;
        if let Some(existing) =
            load_existing(store.connection(), &request.task_id, &request.request_id)
                .map_err(storage_error)?
        {
            if existing.request_digest != request_digest {
                return Err(CommandError::new(
                    "safeFile.idempotencyConflict",
                    "The request id was already used with different operations.",
                ));
            }
            if existing.status == "committed" {
                let results = serde_json::from_str(&existing.results_json).map_err(|error| {
                    CommandError::new("safeFile.corruptTransaction", error.to_string())
                })?;
                return Ok(ExecuteSafeFileOperationsResponse {
                    transaction_id: existing.transaction_id,
                    status: existing.status,
                    results,
                });
            }
            return Err(CommandError::new(
                "safeFile.transactionInRecovery",
                "The prior request is incomplete and will be recovered before it can be retried.",
            ));
        }
        enforce_file_authorization(
            store.connection(),
            &task.id,
            &workspace_path,
            &request,
            &operations_json,
            &request_digest,
        )?;
        let inverse_operations = build_inverse_operations(&workspace_path, &request.operations)
            .map_err(|error| CommandError::new("safeFile.snapshotFailed", error.to_string()))?;
        let inverse_json = serde_json::to_string(&inverse_operations)
            .map_err(|error| CommandError::new("safeFile.snapshotFailed", error.to_string()))?;
        store.connection().execute(
            "INSERT INTO file_edit_transactions (transaction_id, task_id, request_id, request_digest, status, operations_json, inverse_operations_json, results_json, applied_count, approval_id, diff_artifact_id, validation_round_id, proof_pack_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'prepared', ?5, ?6, '[]', 0, ?7, ?8, ?9, ?10, datetime('now'), datetime('now'))",
            params![transaction_id, request.task_id, request.request_id, request_digest, operations_json, inverse_json, request.approval_id, request.diff_artifact_id, request.validation_round_id, request.proof_pack_id],
        ).map_err(StorageError::from).map_err(storage_error)?;
        (workspace_path, inverse_operations)
    };

    let mut results = Vec::with_capacity(request.operations.len());
    for (index, operation) in request.operations.iter().enumerate() {
        persist_progress(storage, &transaction_id, index + 1, &results)?;
        let operation_result =
            safe_fs::execute_operations(&workspace_path, std::slice::from_ref(operation));
        match operation_result {
            Ok(mut item) => {
                results.append(&mut item);
                persist_progress(storage, &transaction_id, index + 1, &results)?;
            }
            Err(error) => {
                rollback(
                    storage,
                    &transaction_id,
                    &workspace_path,
                    &request.operations,
                    &inverse_operations,
                    index,
                    Some(error.to_string()),
                )?;
                return Err(CommandError::new(
                    "safeFile.operationFailed",
                    "The edit failed and all applied operations were rolled back.",
                ));
            }
        }
    }

    let results_json = serde_json::to_string(&results).map_err(|error| {
        CommandError::new("safeFile.resultSerializationFailed", error.to_string())
    })?;
    let mut store = storage.store.lock().map_err(|_| storage_busy())?;
    let transaction = store
        .connection_mut()
        .transaction()
        .map_err(StorageError::from)
        .map_err(storage_error)?;
    transaction.execute("UPDATE file_edit_transactions SET status = 'committed', results_json = ?2, committed_at = datetime('now'), updated_at = datetime('now') WHERE transaction_id = ?1 AND status IN ('prepared', 'applying')", params![transaction_id, results_json]).map_err(StorageError::from).map_err(storage_error)?;
    AgentEventRepository::new(&transaction).create(NewAgentEvent {
        event_id: &Uuid::new_v4().to_string(), task_id: &request.task_id, event_type: "file_edit_transaction_committed", stage: "editing", message: "Recoverable file edit transaction committed.", payload: &serde_json::json!({"transactionId": transaction_id, "requestId": request.request_id, "resultCount": results.len(), "approvalId": request.approval_id, "diffArtifactId": request.diff_artifact_id, "validationRoundId": request.validation_round_id, "proofPackId": request.proof_pack_id}).to_string(),
    }).map_err(storage_error)?;
    transaction
        .commit()
        .map_err(StorageError::from)
        .map_err(storage_error)?;
    Ok(ExecuteSafeFileOperationsResponse {
        transaction_id,
        status: "committed".to_string(),
        results,
    })
}

pub(crate) fn recover_incomplete_file_transactions(
    storage: &ManagedStorage,
) -> Result<(), StorageError> {
    let pending = {
        let store = storage
            .store
            .lock()
            .map_err(|_| StorageError::NotFound("storage lock".into()))?;
        let mut statement = store.connection().prepare("SELECT f.transaction_id, f.task_id, f.operations_json, f.inverse_operations_json, f.applied_count, t.worktree_path FROM file_edit_transactions f JOIN tasks t ON t.id = f.task_id WHERE f.status IN ('prepared', 'applying', 'rolling_back') ORDER BY f.created_at")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, usize>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for (transaction_id, _task_id, operations_json, inverse_json, applied_count, workspace) in
        pending
    {
        let workspace = workspace.ok_or_else(|| {
            StorageError::NotFound(format!("worktree for transaction {transaction_id}"))
        })?;
        let operations: Vec<SafeFileOperation> = serde_json::from_str(&operations_json)
            .map_err(|error| StorageError::NotFound(error.to_string()))?;
        let inverse: Vec<SafeFileOperation> = serde_json::from_str(&inverse_json)
            .map_err(|error| StorageError::NotFound(error.to_string()))?;
        rollback(
            storage,
            &transaction_id,
            &workspace,
            &operations,
            &inverse,
            applied_count,
            None,
        )
        .map_err(|error| StorageError::NotFound(error.message))?;
    }
    Ok(())
}

fn build_inverse_operations(
    workspace: &str,
    operations: &[SafeFileOperation],
) -> std::io::Result<Vec<SafeFileOperation>> {
    let mut inverse = Vec::with_capacity(operations.len());
    for operation in operations {
        inverse.push(match operation {
            SafeFileOperation::Create { path, .. } => {
                SafeFileOperation::Delete { path: path.clone() }
            }
            SafeFileOperation::Update { path, .. } => SafeFileOperation::Update {
                path: path.clone(),
                content: safe_fs::read_utf8(workspace, path)?,
            },
            SafeFileOperation::Delete { path } => SafeFileOperation::Create {
                path: path.clone(),
                content: safe_fs::read_utf8(workspace, path)?,
            },
            SafeFileOperation::Rename { path, destination } => SafeFileOperation::Rename {
                path: destination.clone(),
                destination: path.clone(),
            },
            SafeFileOperation::CreateDirectory { .. } => unreachable!(),
        });
    }
    Ok(inverse)
}

fn persist_progress(
    storage: &ManagedStorage,
    transaction_id: &str,
    applied_count: usize,
    results: &[SafeFileOperationResult],
) -> AppResult<()> {
    let results_json = serde_json::to_string(results).map_err(|error| {
        CommandError::new("safeFile.resultSerializationFailed", error.to_string())
    })?;
    let store = storage.store.lock().map_err(|_| storage_busy())?;
    store.connection().execute("UPDATE file_edit_transactions SET status = 'applying', applied_count = ?2, results_json = ?3, updated_at = datetime('now') WHERE transaction_id = ?1", params![transaction_id, applied_count, results_json]).map_err(StorageError::from).map_err(storage_error)?;
    Ok(())
}

fn rollback(
    storage: &ManagedStorage,
    transaction_id: &str,
    workspace: &str,
    operations: &[SafeFileOperation],
    inverse: &[SafeFileOperation],
    applied_count: usize,
    reason: Option<String>,
) -> AppResult<()> {
    {
        let store = storage.store.lock().map_err(|_| storage_busy())?;
        store.connection().execute("UPDATE file_edit_transactions SET status = 'rolling_back', error_category = ?2, error_message = ?3, updated_at = datetime('now') WHERE transaction_id = ?1", params![transaction_id, reason.as_ref().map(|_| "operation_failed"), reason]).map_err(StorageError::from).map_err(storage_error)?;
    }
    for index in (0..applied_count).rev() {
        compensate_operation(workspace, &operations[index], &inverse[index])?;
    }
    let store = storage.store.lock().map_err(|_| storage_busy())?;
    store.connection().execute("UPDATE file_edit_transactions SET status = 'rolled_back', rolled_back_at = datetime('now'), updated_at = datetime('now') WHERE transaction_id = ?1", params![transaction_id]).map_err(StorageError::from).map_err(storage_error)?;
    Ok(())
}

fn compensate_operation(
    workspace: &str,
    forward: &SafeFileOperation,
    inverse: &SafeFileOperation,
) -> AppResult<()> {
    let read = |path: &str| match safe_fs::read_utf8(workspace, path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CommandError::new(
            "safeFile.rollbackReadFailed",
            error.to_string(),
        )),
    };
    let apply = |operation: &SafeFileOperation| {
        safe_fs::execute_operations(workspace, std::slice::from_ref(operation))
            .map(|_| ())
            .map_err(|error| CommandError::new("safeFile.rollbackFailed", error.to_string()))
    };
    match (forward, inverse) {
        (SafeFileOperation::Create { path, content }, SafeFileOperation::Delete { .. }) => {
            match read(path)? {
                None => Ok(()),
                Some(current) if current == *content => apply(inverse),
                Some(_) => Err(CommandError::new("safeFile.rollbackConflict", "A created path contains unexpected content; recovery stopped without deleting it.")),
            }
        }
        (SafeFileOperation::Update { path, content }, SafeFileOperation::Update { content: before, .. }) => {
            match read(path)? {
                Some(current) if current == *before => Ok(()),
                Some(current) if current == *content => apply(inverse),
                None => apply(&SafeFileOperation::Create { path: path.clone(), content: before.clone() }),
                Some(_) => Err(CommandError::new("safeFile.rollbackConflict", "An updated path changed again after the transaction; recovery stopped without overwriting it.")),
            }
        }
        (SafeFileOperation::Delete { path }, SafeFileOperation::Create { content: before, .. }) => {
            match read(path)? {
                None => apply(inverse),
                Some(current) if current == *before => Ok(()),
                Some(_) => Err(CommandError::new("safeFile.rollbackConflict", "A deleted path was recreated with different content; recovery stopped without overwriting it.")),
            }
        }
        (SafeFileOperation::Rename { path, destination }, SafeFileOperation::Rename { .. }) => {
            match (read(path)?, read(destination)?) {
                (Some(_), None) => Ok(()),
                (None, Some(_)) => apply(inverse),
                _ => Err(CommandError::new("safeFile.rollbackConflict", "Rename recovery found an ambiguous source/destination state.")),
            }
        }
        _ => Err(CommandError::new("safeFile.rollbackInvalid", "The persisted compensation operation does not match the forward operation.")),
    }
}

fn load_existing(
    connection: &rusqlite::Connection,
    task_id: &str,
    request_id: &str,
) -> Result<Option<ExistingTransaction>, StorageError> {
    connection.query_row("SELECT transaction_id, request_digest, status, results_json FROM file_edit_transactions WHERE task_id = ?1 AND request_id = ?2", params![task_id, request_id], |row| Ok(ExistingTransaction { transaction_id: row.get(0)?, request_digest: row.get(1)?, status: row.get(2)?, results_json: row.get(3)? })).optional().map_err(StorageError::from)
}

fn digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn enforce_file_authorization(
    connection: &rusqlite::Connection,
    task_id: &str,
    workspace: &str,
    request: &ExecuteSafeFileOperationsRequest,
    operations_json: &str,
    arguments_digest: &str,
) -> AppResult<()> {
    if !request.operations.iter().any(|operation| {
        matches!(
            operation,
            SafeFileOperation::Update { .. }
                | SafeFileOperation::Delete { .. }
                | SafeFileOperation::Rename { .. }
        )
    }) {
        return Ok(());
    }
    let target = request
        .operations
        .iter()
        .map(operation_target)
        .collect::<Vec<_>>()
        .join(" | ");
    let content_digest = file_content_digest(workspace, &request.operations)?;
    let contract_digest = digest(&format!("{}:{}", task_id, workspace));
    let approvals = ApprovalRepository::new(connection);
    if let Some(approval_id) = request
        .approval_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        approvals.consume_authorization(ApprovalAuthorization { approval_id, task_id, actor: "user", action: "file.mutate", target: &target, arguments_digest, content_digest: &content_digest, scope: "task_worktree", contract_digest: &contract_digest, call_id: &request.request_id })
            .map_err(|_| CommandError::new("approval.authorizationInvalid", "The approval is expired, consumed, changed, or does not match these file operations."))?;
        return Ok(());
    }
    if let Some(existing) = approvals
        .find_bound(task_id, "file.mutate", arguments_digest, &content_digest)
        .map_err(storage_error)?
    {
        return match existing.decision.as_deref() {
            Some("approved") => {
                approvals
                    .consume_authorization(ApprovalAuthorization {
                        approval_id: &existing.id,
                        task_id,
                        actor: "user",
                        action: "file.mutate",
                        target: &target,
                        arguments_digest,
                        content_digest: &content_digest,
                        scope: "task_worktree",
                        contract_digest: &contract_digest,
                        call_id: &request.request_id,
                    })
                    .map_err(|_| {
                        CommandError::new(
                            "approval.authorizationInvalid",
                            "The approval could not be consumed atomically.",
                        )
                    })?;
                Ok(())
            }
            Some("rejected") => Err(CommandError::new(
                "approval.rejected",
                existing.comment.unwrap_or(existing.reason),
            )),
            Some("revise") => Err(CommandError::new(
                "approval.reviseRequested",
                existing.comment.unwrap_or(existing.reason),
            )),
            _ => Err(CommandError::new(
                "approval.pending",
                format!(
                    "File operations require approval before execution. Approval id: {}",
                    existing.id
                ),
            )),
        };
    }
    let id = format!("approval-{}", Uuid::new_v4());
    let nonce = Uuid::new_v4().to_string();
    let expires_at = (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + 900)
        .to_string();
    let content = format!("action: file.mutate\ntarget: {target}\nscope: task_worktree\noperations: {operations_json}");
    approvals
        .create_bound(NewBoundApproval {
            id: &id,
            task_id,
            approval_type: "file_operation",
            risk_level: "high",
            content: &content,
            reason: "Existing task files will be overwritten, deleted, or renamed.",
            actor: "user",
            action: "file.mutate",
            target: &target,
            arguments_digest,
            content_digest: &content_digest,
            scope: "task_worktree",
            nonce: &nonce,
            contract_digest: &contract_digest,
            expires_at: &expires_at,
        })
        .map_err(storage_error)?;
    Err(CommandError::new(
        "approval.required",
        format!("File operations require approval before execution. Approval id: {id}"),
    ))
}

fn operation_target(operation: &SafeFileOperation) -> String {
    match operation {
        SafeFileOperation::Create { path, .. }
        | SafeFileOperation::Update { path, .. }
        | SafeFileOperation::Delete { path }
        | SafeFileOperation::CreateDirectory { path } => path.clone(),
        SafeFileOperation::Rename { path, destination } => format!("{path} -> {destination}"),
    }
}

fn file_content_digest(workspace: &str, operations: &[SafeFileOperation]) -> AppResult<String> {
    let root = Path::new(workspace);
    let mut state = Vec::new();
    for operation in operations {
        let relative = match operation {
            SafeFileOperation::Create { path, .. }
            | SafeFileOperation::Update { path, .. }
            | SafeFileOperation::Delete { path }
            | SafeFileOperation::CreateDirectory { path }
            | SafeFileOperation::Rename { path, .. } => path,
        };
        let path = root.join(relative);
        let value = match fs::read(&path) {
            Ok(bytes) => {
                let mut hasher = Sha256::new();
                hasher.update(bytes);
                format!("{:x}", hasher.finalize())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "missing".to_string(),
            Err(error) => {
                return Err(CommandError::new(
                    "approval.contentDigestFailed",
                    error.to_string(),
                ))
            }
        };
        state.push(format!("{}:{value}", path.to_string_lossy()));
    }
    Ok(digest(&state.join("\n")))
}
fn storage_busy() -> CommandError {
    CommandError::new("storage.lockUnavailable", "The local database is busy.")
}
fn storage_error(error: StorageError) -> CommandError {
    match error {
        StorageError::NotFound(message) => CommandError::new("task.notFound", message),
        other => CommandError::new("storage.operationFailed", other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "codemax-file-transaction-{label}-{}",
            Uuid::new_v4()
        ))
    }

    fn test_storage(label: &str) -> (ManagedStorage, PathBuf) {
        let root = temp_path(label);
        let workspace = root.join("workspace");
        fs::create_dir_all(&workspace).expect("create workspace");
        let storage = ManagedStorage::initialize(&root).expect("initialize storage");
        {
            let store = storage.store.lock().expect("lock storage");
            store.connection().execute(
                "INSERT INTO tasks (id, title, description, type, status, repository_path, worktree_path, source_path, created_at, updated_at) VALUES ('task-1', 'Transaction test', '', 'code', 'running', ?1, ?1, ?1, datetime('now'), datetime('now'))",
                params![workspace.to_string_lossy().as_ref()],
            ).expect("insert task");
        }
        (storage, workspace)
    }

    fn request(
        request_id: &str,
        operations: Vec<SafeFileOperation>,
    ) -> ExecuteSafeFileOperationsRequest {
        ExecuteSafeFileOperationsRequest {
            task_id: "task-1".to_string(),
            request_id: request_id.to_string(),
            operations,
            approval_id: None,
            diff_artifact_id: Some("diff-1".to_string()),
            validation_round_id: Some("validation-1".to_string()),
            proof_pack_id: Some("proof-1".to_string()),
        }
    }

    #[test]
    fn committed_request_is_idempotent_and_event_follows_commit() {
        let (storage, workspace) = test_storage("idempotent");
        let operations = vec![SafeFileOperation::Create {
            path: "created.txt".to_string(),
            content: "committed".to_string(),
        }];
        let first = execute_transaction(&storage, request("request-1", operations.clone()))
            .expect("commit transaction");
        let second = execute_transaction(&storage, request("request-1", operations))
            .expect("replay transaction");
        assert_eq!(first.transaction_id, second.transaction_id);
        assert_eq!(
            fs::read_to_string(workspace.join("created.txt")).unwrap(),
            "committed"
        );
        let store = storage.store.lock().unwrap();
        let status: String = store
            .connection()
            .query_row(
                "SELECT status FROM file_edit_transactions WHERE transaction_id = ?1",
                params![first.transaction_id],
                |row| row.get(0),
            )
            .unwrap();
        let event_count: i64 = store.connection().query_row("SELECT COUNT(*) FROM agent_events WHERE task_id = 'task-1' AND event_type = 'file_edit_transaction_committed'", [], |row| row.get(0)).unwrap();
        assert_eq!(status, "committed");
        assert_eq!(event_count, 1);
    }

    #[test]
    fn request_id_reuse_with_different_operations_is_rejected() {
        let (storage, _) = test_storage("conflict");
        execute_transaction(
            &storage,
            request(
                "request-1",
                vec![SafeFileOperation::Create {
                    path: "a.txt".to_string(),
                    content: "a".to_string(),
                }],
            ),
        )
        .unwrap();
        let error = execute_transaction(
            &storage,
            request(
                "request-1",
                vec![SafeFileOperation::Create {
                    path: "b.txt".to_string(),
                    content: "b".to_string(),
                }],
            ),
        )
        .unwrap_err();
        assert_eq!(error.code, "safeFile.idempotencyConflict");
    }

    #[test]
    fn partial_failure_rolls_back_all_applied_file_changes() {
        let (storage, workspace) = test_storage("rollback");
        fs::write(workspace.join("first.txt"), "before").unwrap();
        fs::write(workspace.join("exists.txt"), "preserve").unwrap();
        let operations = vec![
            SafeFileOperation::Update {
                path: "first.txt".to_string(),
                content: "after".to_string(),
            },
            SafeFileOperation::Create {
                path: "exists.txt".to_string(),
                content: "collision".to_string(),
            },
        ];
        let approval_error =
            execute_transaction(&storage, request("request-rollback", operations.clone()))
                .unwrap_err();
        assert_eq!(approval_error.code, "approval.required");
        let approval_id = {
            let store = storage.store.lock().unwrap();
            let approval = ApprovalRepository::new(store.connection())
                .list_pending()
                .unwrap()
                .into_iter()
                .next()
                .unwrap();
            ApprovalRepository::new(store.connection())
                .decide(&approval.id, "approved", None)
                .unwrap();
            approval.id
        };
        let mut approved_request = request("request-rollback", operations);
        approved_request.approval_id = Some(approval_id);
        let error = execute_transaction(&storage, approved_request).unwrap_err();
        assert_eq!(error.code, "safeFile.operationFailed");
        assert_eq!(
            fs::read_to_string(workspace.join("first.txt")).unwrap(),
            "before"
        );
        assert_eq!(
            fs::read_to_string(workspace.join("exists.txt")).unwrap(),
            "preserve"
        );
        let store = storage.store.lock().unwrap();
        let status: String = store
            .connection()
            .query_row(
                "SELECT status FROM file_edit_transactions WHERE request_id = 'request-rollback'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "rolled_back");
    }

    #[test]
    fn startup_recovery_rolls_back_interrupted_applying_transaction() {
        let (storage, workspace) = test_storage("recovery");
        fs::write(workspace.join("value.txt"), "before").unwrap();
        let operations = vec![SafeFileOperation::Update {
            path: "value.txt".to_string(),
            content: "after-crash".to_string(),
        }];
        let inverse = vec![SafeFileOperation::Update {
            path: "value.txt".to_string(),
            content: "before".to_string(),
        }];
        fs::write(workspace.join("value.txt"), "after-crash").unwrap();
        {
            let store = storage.store.lock().unwrap();
            store.connection().execute(
                "INSERT INTO file_edit_transactions (transaction_id, task_id, request_id, request_digest, status, operations_json, inverse_operations_json, applied_count, created_at, updated_at) VALUES ('tx-crash', 'task-1', 'request-crash', 'digest', 'applying', ?1, ?2, 1, datetime('now'), datetime('now'))",
                params![serde_json::to_string(&operations).unwrap(), serde_json::to_string(&inverse).unwrap()],
            ).unwrap();
        }
        let app_data_root = storage.roots.app_data_dir.clone();
        drop(storage);
        let restarted_storage = ManagedStorage::initialize(&app_data_root).expect("reopen storage");
        recover_incomplete_file_transactions(&restarted_storage).expect("recover transaction");
        assert_eq!(
            fs::read_to_string(workspace.join("value.txt")).unwrap(),
            "before"
        );
        let store = restarted_storage.store.lock().unwrap();
        let status: String = store
            .connection()
            .query_row(
                "SELECT status FROM file_edit_transactions WHERE transaction_id = 'tx-crash'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "rolled_back");
    }
}
