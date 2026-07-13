use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::storage::{ManagedStorage, StorageError, StorageResult};

const TERMINAL_TASK_STATUSES: &[&str] = &["completed", "failed", "cancelled", "merged"];
const DANGEROUS_TASK_STATUSES: &[&str] = &["merging", "delivering"];
const SAFE_RESUMABLE_TASK_STATUSES: &[&str] = &[
    "planning",
    "planned",
    "editing",
    "validating",
    "analyzingError",
    "repairing",
    "running",
];

pub fn recover_interrupted_runtime(storage: &ManagedStorage) -> StorageResult<()> {
    let mut store = storage.store.lock().map_err(|_| {
        StorageError::Io(std::io::Error::other(
            "storage lock unavailable during recovery",
        ))
    })?;
    let transaction = store.connection_mut().transaction()?;
    let now = now_text();

    let interrupted_commands = {
        let mut statement = transaction.prepare(
            "SELECT id, task_id, status FROM command_runs WHERE status IN ('running', 'cancelling')",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (run_id, task_id, previous_status) in interrupted_commands {
        transaction.execute(
            "UPDATE command_runs SET status = 'interrupted' WHERE id = ?1",
            [&run_id],
        )?;
        record_recovery(&transaction, &task_id, "command", Some(&run_id), &previous_status,
            "waiting_confirmation", "manual_confirmation",
            "The command process ended with the application. Its side effects are unknown and it will not be replayed automatically.", true, &now)?;
        mark_task_needs_intervention(&transaction, &task_id, &now)?;
    }

    let sessions = {
        let mut statement = transaction.prepare(
            "SELECT id, task_id, status, stage FROM agent_sessions WHERE status NOT IN ('completed', 'failed', 'cancelled', 'needs_intervention', 'waiting_approval')",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (session_id, task_id, previous_status, stage) in sessions {
        let dangerous = DANGEROUS_TASK_STATUSES.contains(&stage.as_str());
        let reason = if dangerous {
            "The Agent disconnected during a dangerous delivery or merge stage. Automatic replay is disabled."
        } else {
            "The Agent disconnected before reaching a terminal state. Resume is allowed only from the persisted checkpoint."
        };
        transaction.execute(
            "UPDATE agent_sessions SET status = 'needs_intervention', stage = 'needsIntervention', updated_at = ?2 WHERE id = ?1",
            params![session_id, now],
        )?;
        record_recovery(
            &transaction,
            &task_id,
            "agent_session",
            Some(&session_id),
            &previous_status,
            if dangerous {
                "waiting_confirmation"
            } else {
                "recovered"
            },
            if dangerous {
                "manual_confirmation"
            } else {
                "resume_from_checkpoint"
            },
            reason,
            dangerous,
            &now,
        )?;
        mark_task_needs_intervention(&transaction, &task_id, &now)?;
    }

    let tasks = {
        let mut statement = transaction.prepare("SELECT id, status FROM tasks")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (task_id, status) in tasks {
        if TERMINAL_TASK_STATUSES.contains(&status.as_str())
            || status == "awaitingApproval"
            || status == "needsIntervention"
        {
            continue;
        }
        let dangerous = DANGEROUS_TASK_STATUSES.contains(&status.as_str());
        let resumable = SAFE_RESUMABLE_TASK_STATUSES.contains(&status.as_str());
        if !dangerous && !resumable {
            continue;
        }
        let already_recorded = transaction.query_row(
            "SELECT 1 FROM recovery_actions WHERE task_id = ?1 AND resource_type IN ('command', 'agent_session') AND resolved_at IS NULL LIMIT 1",
            [&task_id], |row| row.get::<_, i64>(0)).optional()?.is_some();
        if already_recorded {
            continue;
        }
        record_recovery(
            &transaction,
            &task_id,
            "task",
            Some(&task_id),
            &status,
            if dangerous {
                "waiting_confirmation"
            } else {
                "recovered"
            },
            if dangerous {
                "manual_confirmation"
            } else {
                "resume_from_checkpoint"
            },
            if dangerous {
                "The application stopped during a dangerous task stage. The action will not be replayed automatically."
            } else {
                "The application stopped during an interruptible task stage. Persisted evidence remains available for explicit resume or termination."
            },
            dangerous,
            &now,
        )?;
        mark_task_needs_intervention(&transaction, &task_id, &now)?;
    }

    transaction.commit()?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_recovery(
    connection: &rusqlite::Connection,
    task_id: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    previous_status: &str,
    recovery_status: &str,
    strategy: &str,
    reason: &str,
    requires_confirmation: bool,
    now: &str,
) -> StorageResult<()> {
    connection.execute(
        "INSERT OR IGNORE INTO recovery_actions (id, task_id, resource_type, resource_id, previous_status, recovery_status, strategy, reason, requires_confirmation, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![format!("recovery-{}", Uuid::new_v4()), task_id, resource_type, resource_id, previous_status, recovery_status, strategy, reason, i64::from(requires_confirmation), now])?;
    connection.execute(
        "INSERT INTO agent_events (event_id, task_id, event_type, stage, message, created_at, payload) VALUES (?1, ?2, 'task.recovery.required', 'needsIntervention', ?3, ?4, json_object('resource_type', ?5, 'resource_id', ?6, 'previous_status', ?7, 'recovery_status', ?8, 'strategy', ?9, 'requires_confirmation', ?10))",
        params![format!("event-{}", Uuid::new_v4()), task_id, reason, now, resource_type, resource_id, previous_status, recovery_status, strategy, requires_confirmation])?;
    Ok(())
}

fn mark_task_needs_intervention(
    connection: &rusqlite::Connection,
    task_id: &str,
    now: &str,
) -> StorageResult<()> {
    connection.execute(
        "UPDATE tasks SET status = 'needsIntervention', updated_at = ?2 WHERE id = ?1 AND status NOT IN ('completed', 'failed', 'cancelled', 'merged', 'awaitingApproval')",
        params![task_id, now])?;
    Ok(())
}

fn now_text() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{NewAgentSession, NewCommandRun, NewTask};
    use std::path::PathBuf;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("codemax-recovery-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn storage() -> (TempDir, ManagedStorage) {
        let temp = TempDir::new();
        let storage = ManagedStorage::initialize(&temp.0).unwrap();
        (temp, storage)
    }
    fn seed_task(storage: &ManagedStorage, task_id: &str, status: &str) {
        let store = storage.store.lock().unwrap();
        crate::storage::TaskRepository::new(store.connection())
            .create(NewTask {
                id: task_id,
                title: task_id,
                description: "recovery",
                task_type: "coding",
                status,
                repository_path: "C:/repo",
                worktree_path: Some("C:/worktree"),
                branch_name: Some("codex/task"),
                target_branch: "main",
                workspace_kind: "git_worktree",
                source_path: "C:/repo",
                original_write_authorized: false,
                workspace_estimated_bytes: 0,
                model_id: None,
            })
            .unwrap();
    }
    #[test]
    fn interrupted_command_is_never_replayed_and_requires_confirmation() {
        let (_temp, storage) = storage();
        seed_task(&storage, "task-command", "validating");
        {
            let store = storage.store.lock().unwrap();
            crate::storage::CommandRunRepository::new(store.connection())
                .record(NewCommandRun {
                    id: "run-1",
                    task_id: "task-command",
                    purpose: "validation",
                    command: "danger.exe",
                    cwd: "C:/worktree",
                    status: "running",
                    stdout_path: None,
                    stderr_path: None,
                    exit_code: None,
                    duration_ms: None,
                })
                .unwrap();
        }
        recover_interrupted_runtime(&storage).unwrap();
        recover_interrupted_runtime(&storage).unwrap();
        let store = storage.store.lock().unwrap();
        let status: String = store
            .connection()
            .query_row(
                "SELECT status FROM command_runs WHERE id='run-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let count: i64 = store
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM recovery_actions WHERE resource_id='run-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "interrupted");
        assert_eq!(count, 1);
    }
    #[test]
    fn waiting_approval_survives_restart_without_new_action() {
        let (_temp, storage) = storage();
        seed_task(&storage, "task-approval", "awaitingApproval");
        recover_interrupted_runtime(&storage).unwrap();
        let store = storage.store.lock().unwrap();
        let status: String = store
            .connection()
            .query_row(
                "SELECT status FROM tasks WHERE id='task-approval'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let count: i64 = store
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM recovery_actions WHERE task_id='task-approval'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "awaitingApproval");
        assert_eq!(count, 0);
    }
    #[test]
    fn merge_stage_requires_manual_confirmation() {
        let (_temp, storage) = storage();
        seed_task(&storage, "task-merge", "merging");
        recover_interrupted_runtime(&storage).unwrap();
        let store = storage.store.lock().unwrap();
        let row: (String, i64) = store.connection().query_row("SELECT strategy, requires_confirmation FROM recovery_actions WHERE task_id='task-merge'", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!(row, ("manual_confirmation".into(), 1));
    }
    #[test]
    fn persisted_runtime_is_recovered_after_database_reopen() {
        let temp = TempDir::new();
        {
            let storage = ManagedStorage::initialize(&temp.0).unwrap();
            seed_task(&storage, "task-reopen", "delivering");
        }
        let restarted = ManagedStorage::initialize(&temp.0).unwrap();
        recover_interrupted_runtime(&restarted).unwrap();
        let store = restarted.store.lock().unwrap();
        let row: (String, String, i64) = store.connection().query_row(
            "SELECT t.status, r.strategy, r.requires_confirmation FROM tasks t JOIN recovery_actions r ON r.task_id=t.id WHERE t.id='task-reopen'",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap();
        assert_eq!(
            row,
            ("needsIntervention".into(), "manual_confirmation".into(), 1)
        );
    }

    #[test]
    fn critical_stage_matrix_has_explicit_recovery_strategy() {
        for (stage, expected_strategy, confirmation) in [
            ("planning", "resume_from_checkpoint", 0),
            ("editing", "resume_from_checkpoint", 0),
            ("validating", "resume_from_checkpoint", 0),
            ("repairing", "resume_from_checkpoint", 0),
            ("delivering", "manual_confirmation", 1),
            ("merging", "manual_confirmation", 1),
        ] {
            let (_temp, storage) = storage();
            let task_id = format!("task-{stage}");
            seed_task(&storage, &task_id, stage);
            recover_interrupted_runtime(&storage).unwrap();
            let store = storage.store.lock().unwrap();
            let row: (String, i64) = store
                .connection()
                .query_row(
                    "SELECT strategy, requires_confirmation FROM recovery_actions WHERE task_id=?1",
                    [&task_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(
                row,
                (expected_strategy.into(), confirmation),
                "stage {stage}"
            );
        }
    }

    #[test]
    fn editing_session_keeps_checkpoint_evidence_but_stops_running() {
        let (_temp, storage) = storage();
        seed_task(&storage, "task-agent", "editing");
        {
            let store = storage.store.lock().unwrap();
            crate::storage::AgentSessionRepository::new(store.connection())
                .create(NewAgentSession {
                    id: "session-1",
                    task_id: "task-agent",
                    status: "editing",
                    stage: "editing",
                    checkpoint_id: Some("checkpoint-7"),
                    iterations: 7,
                    repair_round: 1,
                    max_repair_rounds: 3,
                    validation_request_json: "{}",
                    validation_round: 0,
                })
                .unwrap();
        }
        recover_interrupted_runtime(&storage).unwrap();
        let store = storage.store.lock().unwrap();
        let row: (String, Option<String>) = store
            .connection()
            .query_row(
                "SELECT status, checkpoint_id FROM agent_sessions WHERE id='session-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            ("needs_intervention".into(), Some("checkpoint-7".into()))
        );
    }
}
