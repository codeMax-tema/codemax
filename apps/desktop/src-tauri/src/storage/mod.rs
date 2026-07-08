use rusqlite::{params, Connection, OptionalExtension};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

pub const DEFAULT_DATABASE_FILE: &str = "app.db";
const DEFAULT_STORAGE_POLICY_ID: &str = "default";
const DEFAULT_STORAGE_POLICY_SCOPE: &str = "global";
const INITIAL_MIGRATION_VERSION: &str = "0001_initial";
const INITIAL_MIGRATION: &str = include_str!("../../../../../database/migrations/0001_initial.sql");
const S12_EVIDENCE_MIGRATION_VERSION: &str = "0002_s12_evidence";
const S12_EVIDENCE_MIGRATION: &str =
    include_str!("../../../../../database/migrations/0002_s12_evidence.sql");
const A_TASK_CHAIN_MIGRATION_VERSION: &str = "0003_a_task_chain";
const A_TASK_CHAIN_MIGRATION: &str =
    include_str!("../../../../../database/migrations/0003_a_task_chain.sql");
const COMMAND_RUN_PURPOSE_MIGRATION_VERSION: &str = "0004_command_run_purpose";
const COMMAND_RUN_PURPOSE_MIGRATION: &str =
    include_str!("../../../../../database/migrations/0004_command_run_purpose.sql");
pub const DEFAULT_PROFILE_ID: &str = "profile-default";
const PRIVACY_CONTRACT_BUDGET_MIGRATION_VERSION: &str = "0005_privacy_contract_budget";
const PRIVACY_CONTRACT_BUDGET_MIGRATION: &str =
    include_str!("../../../../../database/migrations/0005_privacy_contract_budget.sql");

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0} not found")]
    NotFound(String),
    #[error("task {task_id} is not safe to clean: {reasons:?}")]
    UnsafeCleanup {
        task_id: String,
        reasons: Vec<String>,
    },
}

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug)]
pub struct SqliteStore {
    connection: Connection,
}

impl SqliteStore {
    pub fn open(database_path: impl AsRef<Path>) -> StorageResult<Self> {
        let database_path = database_path.as_ref();
        if let Some(parent) = database_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let connection = Connection::open(database_path)?;
        enable_foreign_keys(&connection)?;

        Ok(Self { connection })
    }

    pub fn open_in_memory() -> StorageResult<Self> {
        let connection = Connection::open_in_memory()?;
        enable_foreign_keys(&connection)?;

        Ok(Self { connection })
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }

    pub fn migrate(&self) -> StorageResult<()> {
        self.connection.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY,
                applied_at TEXT NOT NULL
            )",
            [],
        )?;

        self.apply_migration(INITIAL_MIGRATION_VERSION, INITIAL_MIGRATION)?;
        self.apply_migration(S12_EVIDENCE_MIGRATION_VERSION, S12_EVIDENCE_MIGRATION)?;
        self.apply_migration(A_TASK_CHAIN_MIGRATION_VERSION, A_TASK_CHAIN_MIGRATION)?;
        self.apply_migration(
            COMMAND_RUN_PURPOSE_MIGRATION_VERSION,
            COMMAND_RUN_PURPOSE_MIGRATION,
        )?;
        self.apply_migration(
            PRIVACY_CONTRACT_BUDGET_MIGRATION_VERSION,
            PRIVACY_CONTRACT_BUDGET_MIGRATION,
        )?;

        StoragePolicyRepository::new(&self.connection).ensure_default_policy()?;
        PersonalProfileRepository::new(&self.connection).ensure_default_active_profile()?;
        Ok(())
    }

    fn apply_migration(&self, version: &str, sql: &str) -> StorageResult<()> {
        let applied = self
            .connection
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version = ?1",
                params![version],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some();

        if applied {
            return Ok(());
        }

        self.connection.execute_batch(sql)?;
        self.connection.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![version, now_text()],
        )?;
        Ok(())
    }

    pub fn table_names(&self) -> StorageResult<Vec<String>> {
        let mut statement = self.connection.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut names = Vec::new();

        for row in rows {
            names.push(row?);
        }

        Ok(names)
    }
}

#[derive(Debug)]
pub struct ManagedStorage {
    pub roots: StorageRoots,
    pub store: Mutex<SqliteStore>,
}

impl ManagedStorage {
    pub fn initialize(app_data_dir: impl AsRef<Path>) -> StorageResult<Self> {
        let roots = StorageRoots::from_runtime(app_data_dir);
        roots.ensure_base_dirs()?;

        let store = SqliteStore::open(roots.database_path())?;
        store.migrate()?;

        Ok(Self {
            roots,
            store: Mutex::new(store),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRoots {
    pub app_data_dir: PathBuf,
    pub artifact_root: PathBuf,
    pub worktree_root: PathBuf,
    database_path: PathBuf,
}

impl StorageRoots {
    pub fn from_app_data_dir(app_data_dir: impl AsRef<Path>) -> Self {
        let app_data_dir = app_data_dir.as_ref().to_path_buf();
        let database_path = app_data_dir.join(DEFAULT_DATABASE_FILE);

        Self {
            artifact_root: app_data_dir.join("tasks"),
            worktree_root: app_data_dir.join("worktrees"),
            app_data_dir,
            database_path,
        }
    }

    pub fn from_runtime(app_data_dir: impl AsRef<Path>) -> Self {
        let app_data_dir =
            env_path("CODEMAX_APP_DATA_DIR").unwrap_or_else(|| app_data_dir.as_ref().to_path_buf());
        let artifact_root =
            env_path("CODEMAX_ARTIFACT_ROOT").unwrap_or_else(|| app_data_dir.join("tasks"));
        let worktree_root =
            env_path("CODEMAX_WORKTREE_ROOT").unwrap_or_else(|| app_data_dir.join("worktrees"));
        let database_path = env_database_path("CODEMAX_DATABASE_URL")
            .unwrap_or_else(|| app_data_dir.join(DEFAULT_DATABASE_FILE));

        Self {
            app_data_dir,
            artifact_root,
            worktree_root,
            database_path,
        }
    }

    pub fn database_path(&self) -> PathBuf {
        self.database_path.clone()
    }

    pub fn ensure_base_dirs(&self) -> StorageResult<()> {
        fs::create_dir_all(&self.app_data_dir)?;
        fs::create_dir_all(&self.artifact_root)?;
        fs::create_dir_all(&self.worktree_root)?;
        Ok(())
    }

    pub fn task_artifact_paths(&self, task_id: &str) -> TaskArtifactPaths {
        let root = self.artifact_root.join(task_id);

        TaskArtifactPaths {
            logs_dir: root.join("logs"),
            artifacts_dir: root.join("artifacts"),
            screenshots_dir: root.join("screenshots"),
            context_dir: root.join("context"),
            diff_path: root.join("diff.patch"),
            report_path: root.join("report.json"),
            root,
        }
    }

    pub fn ensure_task_artifact_dirs(&self, task_id: &str) -> StorageResult<TaskArtifactPaths> {
        let paths = self.task_artifact_paths(task_id);

        fs::create_dir_all(&paths.logs_dir)?;
        fs::create_dir_all(&paths.artifacts_dir)?;
        fs::create_dir_all(&paths.screenshots_dir)?;
        fs::create_dir_all(&paths.context_dir)?;

        Ok(paths)
    }

    pub fn task_storage_usage(&self, task_id: &str) -> StorageResult<StorageUsage> {
        let paths = self.task_artifact_paths(task_id);
        let worktree_path = self.worktree_root.join(task_id);

        StorageUsage::measure(&paths, Some(&worktree_path))
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn env_database_path(key: &str) -> Option<PathBuf> {
    let value = env::var(key).ok()?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if let Some(path) = value.strip_prefix("sqlite://") {
        if path == "app-data/app.db" {
            return None;
        }
        return (!path.is_empty()).then(|| PathBuf::from(path));
    }

    Some(PathBuf::from(value))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskArtifactPaths {
    pub root: PathBuf,
    pub logs_dir: PathBuf,
    pub artifacts_dir: PathBuf,
    pub screenshots_dir: PathBuf,
    pub context_dir: PathBuf,
    pub diff_path: PathBuf,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageUsage {
    pub worktree_bytes: u64,
    pub logs_bytes: u64,
    pub screenshots_bytes: u64,
    pub context_bytes: u64,
    pub artifact_bytes: u64,
    pub total_bytes: u64,
}

impl StorageUsage {
    pub fn measure(
        task_paths: &TaskArtifactPaths,
        worktree_path: Option<&Path>,
    ) -> StorageResult<Self> {
        let worktree_bytes = worktree_path.map(directory_size).transpose()?.unwrap_or(0);
        let logs_bytes = directory_size(&task_paths.logs_dir)?;
        let screenshots_bytes = directory_size(&task_paths.screenshots_dir)?;
        let context_bytes = directory_size(&task_paths.context_dir)?;
        let artifact_bytes = directory_size(&task_paths.artifacts_dir)?
            + file_size_if_present(&task_paths.diff_path)?
            + file_size_if_present(&task_paths.report_path)?;
        let total_bytes =
            worktree_bytes + logs_bytes + screenshots_bytes + context_bytes + artifact_bytes;

        Ok(Self {
            worktree_bytes,
            logs_bytes,
            screenshots_bytes,
            context_bytes,
            artifact_bytes,
            total_bytes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub description: String,
    pub task_type: String,
    pub status: String,
    pub repository_path: String,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    pub model_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct NewTask<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub task_type: &'a str,
    pub status: &'a str,
    pub repository_path: &'a str,
    pub worktree_path: Option<&'a str>,
    pub branch_name: Option<&'a str>,
    pub model_id: Option<&'a str>,
}

pub struct TaskRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> TaskRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn create(&self, task: NewTask<'_>) -> StorageResult<TaskRecord> {
        let now = now_text();
        self.connection.execute(
            "INSERT INTO tasks (
                id, title, description, type, status, repository_path, worktree_path,
                branch_name, model_id, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            params![
                task.id,
                task.title,
                task.description,
                task.task_type,
                task.status,
                task.repository_path,
                task.worktree_path,
                task.branch_name,
                task.model_id,
                now,
            ],
        )?;

        self.get_required(task.id)
    }

    pub fn get(&self, task_id: &str) -> StorageResult<Option<TaskRecord>> {
        self.connection
            .query_row(
                "SELECT id, title, description, type, status, repository_path, worktree_path,
                    branch_name, model_id, created_at, updated_at, completed_at
                 FROM tasks WHERE id = ?1",
                params![task_id],
                map_task_record,
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn get_required(&self, task_id: &str) -> StorageResult<TaskRecord> {
        self.get(task_id)?
            .ok_or_else(|| StorageError::NotFound(format!("task {task_id}")))
    }

    pub fn list_recent(
        &self,
        repository_path: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> StorageResult<Vec<TaskRecord>> {
        let limit = limit.clamp(1, 200) as i64;

        match (repository_path, status) {
            (Some(repository_path), Some(status)) => {
                let mut statement = self.connection.prepare(
                    "SELECT id, title, description, type, status, repository_path, worktree_path,
                        branch_name, model_id, created_at, updated_at, completed_at
                     FROM tasks
                     WHERE repository_path = ?1 AND status = ?2
                     ORDER BY updated_at DESC, created_at DESC
                     LIMIT ?3",
                )?;
                let rows = statement.query(params![repository_path, status, limit])?;
                collect_task_rows(rows)
            }
            (Some(repository_path), None) => {
                let mut statement = self.connection.prepare(
                    "SELECT id, title, description, type, status, repository_path, worktree_path,
                        branch_name, model_id, created_at, updated_at, completed_at
                     FROM tasks
                     WHERE repository_path = ?1
                     ORDER BY updated_at DESC, created_at DESC
                     LIMIT ?2",
                )?;
                let rows = statement.query(params![repository_path, limit])?;
                collect_task_rows(rows)
            }
            (None, Some(status)) => {
                let mut statement = self.connection.prepare(
                    "SELECT id, title, description, type, status, repository_path, worktree_path,
                        branch_name, model_id, created_at, updated_at, completed_at
                     FROM tasks
                     WHERE status = ?1
                     ORDER BY updated_at DESC, created_at DESC
                     LIMIT ?2",
                )?;
                let rows = statement.query(params![status, limit])?;
                collect_task_rows(rows)
            }
            (None, None) => {
                let mut statement = self.connection.prepare(
                    "SELECT id, title, description, type, status, repository_path, worktree_path,
                        branch_name, model_id, created_at, updated_at, completed_at
                     FROM tasks
                     ORDER BY updated_at DESC, created_at DESC
                     LIMIT ?1",
                )?;
                let rows = statement.query(params![limit])?;
                collect_task_rows(rows)
            }
        }
    }

    pub fn update_status(
        &self,
        task_id: &str,
        status: &str,
        completed_at: Option<&str>,
    ) -> StorageResult<()> {
        self.connection.execute(
            "UPDATE tasks
             SET status = ?2, completed_at = ?3, updated_at = ?4
             WHERE id = ?1",
            params![task_id, status, completed_at, now_text()],
        )?;
        Ok(())
    }

    pub fn update_worktree_metadata(
        &self,
        task_id: &str,
        worktree_path: &str,
        branch_name: &str,
    ) -> StorageResult<TaskRecord> {
        let updated = self.connection.execute(
            "UPDATE tasks
             SET worktree_path = ?2, branch_name = ?3, updated_at = ?4
             WHERE id = ?1",
            params![task_id, worktree_path, branch_name, now_text()],
        )?;

        if updated == 0 {
            return Err(StorageError::NotFound(format!("task {task_id}")));
        }

        self.get_required(task_id)
    }

    pub fn clear_worktree_metadata(&self, task_id: &str) -> StorageResult<TaskRecord> {
        let updated = self.connection.execute(
            "UPDATE tasks
             SET worktree_path = NULL, branch_name = NULL, updated_at = ?2
             WHERE id = ?1",
            params![task_id, now_text()],
        )?;

        if updated == 0 {
            return Err(StorageError::NotFound(format!("task {task_id}")));
        }

        self.get_required(task_id)
    }
}

fn collect_task_rows(mut rows: rusqlite::Rows<'_>) -> StorageResult<Vec<TaskRecord>> {
    let mut tasks = Vec::new();
    while let Some(row) = rows.next()? {
        tasks.push(map_task_record(row)?);
    }
    Ok(tasks)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoRecord {
    pub id: String,
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct NewTodo<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub status: &'a str,
}

pub struct TodoRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> TodoRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn create(&self, todo: NewTodo<'_>) -> StorageResult<TodoRecord> {
        self.connection.execute(
            "INSERT INTO todos (id, task_id, title, description, status)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                todo.id,
                todo.task_id,
                todo.title,
                todo.description,
                todo.status,
            ],
        )?;

        self.get_required(todo.id)
    }

    pub fn upsert(&self, todo: NewTodo<'_>) -> StorageResult<TodoRecord> {
        let timestamp = now_text();
        self.connection.execute(
            "INSERT INTO todos (
                id, task_id, title, description, status, started_at, completed_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                CASE WHEN ?5 = 'in_progress' THEN ?6 ELSE NULL END,
                CASE WHEN ?5 IN ('completed', 'failed', 'skipped') THEN ?6 ELSE NULL END
             )
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                description = excluded.description,
                status = excluded.status,
                started_at = CASE
                    WHEN excluded.status = 'in_progress' AND todos.started_at IS NULL THEN ?6
                    ELSE todos.started_at
                END,
                completed_at = CASE
                    WHEN excluded.status IN ('completed', 'failed', 'skipped') THEN ?6
                    ELSE todos.completed_at
                END,
                error_message = NULL",
            params![
                todo.id,
                todo.task_id,
                todo.title,
                todo.description,
                todo.status,
                timestamp,
            ],
        )?;

        self.get_required(todo.id)
    }

    pub fn update_status(
        &self,
        todo_id: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> StorageResult<()> {
        let timestamp = now_text();
        self.connection.execute(
            "UPDATE todos
             SET status = ?2,
                 started_at = CASE WHEN ?2 = 'in_progress' THEN ?4 ELSE started_at END,
                 completed_at = CASE WHEN ?2 IN ('completed', 'failed') THEN ?4 ELSE completed_at END,
                 error_message = ?3
             WHERE id = ?1",
            params![todo_id, status, error_message, timestamp],
        )?;
        Ok(())
    }

    pub fn list_for_task(&self, task_id: &str) -> StorageResult<Vec<TodoRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, title, description, status, started_at, completed_at, error_message
             FROM todos
             WHERE task_id = ?1
             ORDER BY id",
        )?;
        let rows = statement.query_map(params![task_id], map_todo_record)?;
        let mut todos = Vec::new();

        for row in rows {
            todos.push(row?);
        }

        Ok(todos)
    }

    fn get_required(&self, todo_id: &str) -> StorageResult<TodoRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, title, description, status, started_at, completed_at, error_message
                 FROM todos WHERE id = ?1",
                params![todo_id],
                map_todo_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("todo {todo_id}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRunRecord {
    pub id: String,
    pub task_id: String,
    pub purpose: String,
    pub command: String,
    pub cwd: String,
    pub status: String,
    pub stdout_path: Option<String>,
    pub stderr_path: Option<String>,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewCommandRun<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub purpose: &'a str,
    pub command: &'a str,
    pub cwd: &'a str,
    pub status: &'a str,
    pub stdout_path: Option<&'a str>,
    pub stderr_path: Option<&'a str>,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
}

pub struct CommandRunRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> CommandRunRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn record(&self, run: NewCommandRun<'_>) -> StorageResult<CommandRunRecord> {
        self.connection.execute(
            "INSERT INTO command_runs (
                id, task_id, purpose, command, cwd, status, stdout_path, stderr_path,
                exit_code, duration_ms, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                run.id,
                run.task_id,
                run.purpose,
                run.command,
                run.cwd,
                run.status,
                run.stdout_path,
                run.stderr_path,
                run.exit_code,
                run.duration_ms,
                now_text(),
            ],
        )?;

        self.get_required(run.id)
    }

    pub fn list_for_task(&self, task_id: &str) -> StorageResult<Vec<CommandRunRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, command, cwd, status, stdout_path, stderr_path,
                exit_code, duration_ms, created_at, purpose
             FROM command_runs
             WHERE task_id = ?1
             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map(params![task_id], map_command_run_record)?;
        let mut runs = Vec::new();

        for row in rows {
            runs.push(row?);
        }

        Ok(runs)
    }

    pub fn get_for_task(&self, task_id: &str, run_id: &str) -> StorageResult<CommandRunRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, command, cwd, status, stdout_path, stderr_path,
                    exit_code, duration_ms, created_at, purpose
                 FROM command_runs WHERE task_id = ?1 AND id = ?2",
                params![task_id, run_id],
                map_command_run_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("command run {run_id}")))
    }

    fn get_required(&self, run_id: &str) -> StorageResult<CommandRunRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, command, cwd, status, stdout_path, stderr_path,
                    exit_code, duration_ms, created_at, purpose
                 FROM command_runs WHERE id = ?1",
                params![run_id],
                map_command_run_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("command run {run_id}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRecord {
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

#[derive(Debug, Clone, Copy)]
pub struct NewApproval<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub approval_type: &'a str,
    pub risk_level: &'a str,
    pub content: &'a str,
    pub reason: &'a str,
}

pub struct ApprovalRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> ApprovalRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn create(&self, approval: NewApproval<'_>) -> StorageResult<ApprovalRecord> {
        self.connection.execute(
            "INSERT INTO approvals (
                id, task_id, type, risk_level, content, reason, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                approval.id,
                approval.task_id,
                approval.approval_type,
                approval.risk_level,
                approval.content,
                approval.reason,
                now_text(),
            ],
        )?;

        self.get_required(approval.id)
    }

    pub fn decide(
        &self,
        approval_id: &str,
        decision: &str,
        comment: Option<&str>,
    ) -> StorageResult<ApprovalRecord> {
        let updated = self.connection.execute(
            "UPDATE approvals
             SET decision = ?2, comment = ?3, decided_at = ?4
             WHERE id = ?1",
            params![approval_id, decision, comment, now_text()],
        )?;

        if updated == 0 {
            return Err(StorageError::NotFound(format!("approval {approval_id}")));
        }

        self.get_required(approval_id)
    }

    pub fn list_for_task(&self, task_id: &str) -> StorageResult<Vec<ApprovalRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, type, risk_level, content, reason, decision,
                comment, created_at, decided_at
             FROM approvals
             WHERE task_id = ?1
             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map(params![task_id], map_approval_record)?;
        let mut approvals = Vec::new();

        for row in rows {
            approvals.push(row?);
        }

        Ok(approvals)
    }

    pub fn list_pending(&self) -> StorageResult<Vec<ApprovalRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, type, risk_level, content, reason, decision,
                comment, created_at, decided_at
             FROM approvals
             WHERE decision IS NULL
             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], map_approval_record)?;
        let mut approvals = Vec::new();

        for row in rows {
            approvals.push(row?);
        }

        Ok(approvals)
    }

    pub fn find_for_content(
        &self,
        task_id: &str,
        approval_type: &str,
        content: &str,
    ) -> StorageResult<Option<ApprovalRecord>> {
        self.connection
            .query_row(
                "SELECT id, task_id, type, risk_level, content, reason, decision,
                    comment, created_at, decided_at
                 FROM approvals
                 WHERE task_id = ?1 AND type = ?2 AND content = ?3
                 ORDER BY created_at DESC, id DESC
                 LIMIT 1",
                params![task_id, approval_type, content],
                map_approval_record,
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn get_required(&self, approval_id: &str) -> StorageResult<ApprovalRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, type, risk_level, content, reason, decision,
                    comment, created_at, decided_at
                 FROM approvals WHERE id = ?1",
                params![approval_id],
                map_approval_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("approval {approval_id}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub id: String,
    pub task_id: String,
    pub changed_files: String,
    pub diff_path: Option<String>,
    pub test_report_path: Option<String>,
    pub screenshots: String,
    pub summary: String,
    pub commit_message: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewArtifact<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub changed_files: &'a str,
    pub diff_path: Option<&'a str>,
    pub test_report_path: Option<&'a str>,
    pub screenshots: &'a str,
    pub summary: &'a str,
    pub commit_message: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelConfigRecord {
    pub id: String,
    pub provider: String,
    pub base_url: String,
    pub model_name: String,
    pub api_key_secret_ref: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewModelConfig<'a> {
    pub id: &'a str,
    pub provider: &'a str,
    pub base_url: &'a str,
    pub model_name: &'a str,
    pub api_key_secret_ref: Option<&'a str>,
}

pub struct ModelConfigRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> ModelConfigRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn save(&self, config: NewModelConfig<'_>) -> StorageResult<ModelConfigRecord> {
        let now = now_text();
        self.connection.execute(
            "INSERT INTO model_configs (
                id, provider, base_url, model_name, api_key_secret_ref, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                base_url = excluded.base_url,
                model_name = excluded.model_name,
                api_key_secret_ref = excluded.api_key_secret_ref,
                updated_at = excluded.updated_at",
            params![
                config.id,
                config.provider,
                config.base_url,
                config.model_name,
                config.api_key_secret_ref,
                now,
            ],
        )?;

        self.get(config.id)?
            .ok_or_else(|| StorageError::NotFound(format!("model config {}", config.id)))
    }

    pub fn get(&self, config_id: &str) -> StorageResult<Option<ModelConfigRecord>> {
        self.connection
            .query_row(
                "SELECT id, provider, base_url, model_name, api_key_secret_ref, created_at, updated_at
                 FROM model_configs WHERE id = ?1",
                params![config_id],
                map_model_config_record,
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn list(&self) -> StorageResult<Vec<ModelConfigRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, provider, base_url, model_name, api_key_secret_ref, created_at, updated_at
             FROM model_configs
             ORDER BY updated_at DESC, id",
        )?;
        let rows = statement.query_map([], map_model_config_record)?;
        let mut configs = Vec::new();

        for row in rows {
            configs.push(row?);
        }

        Ok(configs)
    }
}

pub struct AppSettingsRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> AppSettingsRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn set(&self, key: &str, value: &str) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO app_settings (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at",
            params![key, value, now_text()],
        )?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> StorageResult<Option<String>> {
        self.connection
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(StorageError::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoragePolicy {
    pub id: String,
    pub scope: String,
    pub keep_recent_messages: i64,
    pub raw_log_retention_days: i64,
    pub screenshot_retention_days: i64,
    pub temporary_context_retention_days: i64,
    pub auto_cleanup_worktree_after_merge: bool,
    pub keep_final_diff_forever: bool,
    pub keep_approval_records_forever: bool,
    pub updated_at: String,
}

pub struct StoragePolicyRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> StoragePolicyRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn ensure_default_policy(&self) -> StorageResult<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO storage_policies (
                id, scope, keep_recent_messages, raw_log_retention_days,
                screenshot_retention_days, temporary_context_retention_days,
                auto_cleanup_worktree_after_merge, keep_final_diff_forever,
                keep_approval_records_forever, updated_at
            ) VALUES (?1, ?2, 50, 30, 30, 7, 0, 1, 1, ?3)",
            params![
                DEFAULT_STORAGE_POLICY_ID,
                DEFAULT_STORAGE_POLICY_SCOPE,
                now_text(),
            ],
        )?;
        Ok(())
    }

    pub fn default_policy(&self) -> StorageResult<StoragePolicy> {
        self.connection
            .query_row(
                "SELECT id, scope, keep_recent_messages, raw_log_retention_days,
                    screenshot_retention_days, temporary_context_retention_days,
                    auto_cleanup_worktree_after_merge, keep_final_diff_forever,
                    keep_approval_records_forever, updated_at
                 FROM storage_policies WHERE id = ?1",
                params![DEFAULT_STORAGE_POLICY_ID],
                map_storage_policy,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound("default storage policy".to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationRecord {
    pub id: String,
    pub task_id: Option<String>,
    pub repository_path: Option<String>,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_message_id: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct NewConversation<'a> {
    pub id: &'a str,
    pub task_id: Option<&'a str>,
    pub repository_path: Option<&'a str>,
    pub title: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRecord {
    pub id: String,
    pub conversation_id: String,
    pub task_id: Option<String>,
    pub role: String,
    pub content: String,
    pub token_count: i64,
    pub is_pinned: bool,
    pub retention_class: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewMessage<'a> {
    pub id: &'a str,
    pub conversation_id: &'a str,
    pub task_id: Option<&'a str>,
    pub role: &'a str,
    pub content: &'a str,
    pub token_count: i64,
    pub is_pinned: bool,
    pub retention_class: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryItemRecord {
    pub id: String,
    pub scope: String,
    pub scope_id: Option<String>,
    pub key: String,
    pub value: String,
    pub confidence: f64,
    pub source: String,
    pub is_user_editable: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewMemoryItem<'a> {
    pub id: &'a str,
    pub scope: &'a str,
    pub scope_id: Option<&'a str>,
    pub key: &'a str,
    pub value: &'a str,
    pub confidence: f64,
    pub source: &'a str,
    pub is_user_editable: bool,
}

pub struct MemoryRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> MemoryRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn create_conversation(
        &self,
        conversation: NewConversation<'_>,
    ) -> StorageResult<ConversationRecord> {
        let now = now_text();
        self.connection.execute(
            "INSERT INTO conversations (
                id, task_id, repository_path, title, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![
                conversation.id,
                conversation.task_id,
                conversation.repository_path,
                conversation.title,
                now,
            ],
        )?;

        self.conversation_required(conversation.id)
    }

    pub fn add_message(&self, message: NewMessage<'_>) -> StorageResult<MessageRecord> {
        let now = now_text();
        self.connection.execute(
            "INSERT INTO messages (
                id, conversation_id, task_id, role, content, token_count,
                is_pinned, retention_class, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                message.id,
                message.conversation_id,
                message.task_id,
                message.role,
                message.content,
                message.token_count,
                bool_to_i64(message.is_pinned),
                message.retention_class,
                now,
            ],
        )?;
        self.connection.execute(
            "UPDATE conversations
             SET last_message_id = ?2, updated_at = ?3
             WHERE id = ?1",
            params![message.conversation_id, message.id, now_text()],
        )?;

        self.message_required(message.id)
    }

    pub fn recent_messages(
        &self,
        conversation_id: &str,
        limit: i64,
    ) -> StorageResult<Vec<MessageRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, conversation_id, task_id, role, content, token_count,
                is_pinned, retention_class, created_at
             FROM messages
             WHERE conversation_id = ?1
             ORDER BY created_at DESC, id DESC
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![conversation_id, limit], map_message_record)?;
        let mut messages = Vec::new();

        for row in rows {
            messages.push(row?);
        }

        messages.reverse();
        Ok(messages)
    }

    pub fn upsert_memory_item(&self, memory: NewMemoryItem<'_>) -> StorageResult<MemoryItemRecord> {
        let now = now_text();
        self.connection.execute(
            "INSERT INTO memory_items (
                id, scope, scope_id, key, value, confidence, source,
                is_user_editable, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
            ON CONFLICT(id) DO UPDATE SET
                scope = excluded.scope,
                scope_id = excluded.scope_id,
                key = excluded.key,
                value = excluded.value,
                confidence = excluded.confidence,
                source = excluded.source,
                is_user_editable = excluded.is_user_editable,
                updated_at = excluded.updated_at",
            params![
                memory.id,
                memory.scope,
                memory.scope_id,
                memory.key,
                memory.value,
                memory.confidence,
                memory.source,
                bool_to_i64(memory.is_user_editable),
                now,
            ],
        )?;

        self.memory_item(memory.id)?
            .ok_or_else(|| StorageError::NotFound(format!("memory item {}", memory.id)))
    }

    pub fn memory_item(&self, memory_id: &str) -> StorageResult<Option<MemoryItemRecord>> {
        self.connection
            .query_row(
                "SELECT id, scope, scope_id, key, value, confidence, source,
                    is_user_editable, created_at, updated_at
                 FROM memory_items WHERE id = ?1",
                params![memory_id],
                map_memory_item_record,
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn delete_memory_item(&self, memory_id: &str) -> StorageResult<()> {
        self.connection
            .execute("DELETE FROM memory_items WHERE id = ?1", params![memory_id])?;
        Ok(())
    }

    fn conversation_required(&self, conversation_id: &str) -> StorageResult<ConversationRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, repository_path, title, created_at, updated_at, last_message_id
                 FROM conversations WHERE id = ?1",
                params![conversation_id],
                map_conversation_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("conversation {conversation_id}")))
    }

    fn message_required(&self, message_id: &str) -> StorageResult<MessageRecord> {
        self.connection
            .query_row(
                "SELECT id, conversation_id, task_id, role, content, token_count,
                    is_pinned, retention_class, created_at
                 FROM messages WHERE id = ?1",
                params![message_id],
                map_message_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("message {message_id}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactFileRecord {
    pub id: String,
    pub task_id: String,
    pub artifact_id: Option<String>,
    pub file_type: String,
    pub path: String,
    pub size_bytes: i64,
    pub compressed: bool,
    pub retention_class: String,
    pub created_at: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct NewArtifactFile<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub artifact_id: Option<&'a str>,
    pub file_type: &'a str,
    pub path: &'a str,
    pub size_bytes: i64,
    pub compressed: bool,
    pub retention_class: &'a str,
    pub expires_at: Option<&'a str>,
}

pub struct ArtifactRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> ArtifactRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn record_artifact(&self, artifact: NewArtifact<'_>) -> StorageResult<ArtifactRecord> {
        self.connection.execute(
            "INSERT INTO artifacts (
                id, task_id, changed_files, diff_path, test_report_path,
                screenshots, summary, commit_message
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                artifact.id,
                artifact.task_id,
                artifact.changed_files,
                artifact.diff_path,
                artifact.test_report_path,
                artifact.screenshots,
                artifact.summary,
                artifact.commit_message,
            ],
        )?;

        self.artifact_required(artifact.id)
    }

    pub fn artifacts_for_task(&self, task_id: &str) -> StorageResult<Vec<ArtifactRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, changed_files, diff_path, test_report_path,
                screenshots, summary, commit_message
             FROM artifacts
             WHERE task_id = ?1
             ORDER BY id",
        )?;
        let rows = statement.query_map(params![task_id], map_artifact_record)?;
        let mut artifacts = Vec::new();

        for row in rows {
            artifacts.push(row?);
        }

        Ok(artifacts)
    }

    pub fn record_file(&self, file: NewArtifactFile<'_>) -> StorageResult<ArtifactFileRecord> {
        let now = now_text();
        self.connection.execute(
            "INSERT INTO artifact_files (
                id, task_id, artifact_id, type, path, size_bytes, compressed,
                retention_class, created_at, expires_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                file.id,
                file.task_id,
                file.artifact_id,
                file.file_type,
                file.path,
                file.size_bytes,
                bool_to_i64(file.compressed),
                file.retention_class,
                now,
                file.expires_at,
            ],
        )?;

        self.file_required(file.id)
    }

    pub fn files_for_task(&self, task_id: &str) -> StorageResult<Vec<ArtifactFileRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, artifact_id, type, path, size_bytes, compressed,
                retention_class, created_at, expires_at
             FROM artifact_files
             WHERE task_id = ?1
             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map(params![task_id], map_artifact_file_record)?;
        let mut files = Vec::new();

        for row in rows {
            files.push(row?);
        }

        Ok(files)
    }

    fn file_required(&self, file_id: &str) -> StorageResult<ArtifactFileRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, artifact_id, type, path, size_bytes, compressed,
                    retention_class, created_at, expires_at
                 FROM artifact_files WHERE id = ?1",
                params![file_id],
                map_artifact_file_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("artifact file {file_id}")))
    }

    fn artifact_required(&self, artifact_id: &str) -> StorageResult<ArtifactRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, changed_files, diff_path, test_report_path,
                    screenshots, summary, commit_message
                 FROM artifacts WHERE id = ?1",
                params![artifact_id],
                map_artifact_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("artifact {artifact_id}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionRecord {
    pub id: String,
    pub task_id: String,
    pub status: String,
    pub stage: String,
    pub checkpoint_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewAgentSession<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub status: &'a str,
    pub stage: &'a str,
    pub checkpoint_id: Option<&'a str>,
}

pub struct AgentSessionRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> AgentSessionRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn create(&self, session: NewAgentSession<'_>) -> StorageResult<AgentSessionRecord> {
        let now = now_text();
        self.connection.execute(
            "INSERT INTO agent_sessions (
                id, task_id, status, stage, checkpoint_id, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![
                session.id,
                session.task_id,
                session.status,
                session.stage,
                session.checkpoint_id,
                now,
            ],
        )?;

        self.get_required(session.id)
    }

    pub fn upsert(&self, session: NewAgentSession<'_>) -> StorageResult<AgentSessionRecord> {
        let now = now_text();
        self.connection.execute(
            "INSERT INTO agent_sessions (
                id, task_id, status, stage, checkpoint_id, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET
                status = excluded.status,
                stage = excluded.stage,
                checkpoint_id = excluded.checkpoint_id,
                updated_at = ?6",
            params![
                session.id,
                session.task_id,
                session.status,
                session.stage,
                session.checkpoint_id,
                now,
            ],
        )?;

        self.get_required(session.id)
    }

    pub fn get_for_task(&self, task_id: &str) -> StorageResult<Option<AgentSessionRecord>> {
        self.connection
            .query_row(
                "SELECT id, task_id, status, stage, checkpoint_id, created_at, updated_at
                 FROM agent_sessions
                 WHERE task_id = ?1
                 ORDER BY updated_at DESC, id DESC
                 LIMIT 1",
                params![task_id],
                map_agent_session_record,
            )
            .optional()
            .map_err(StorageError::from)
    }

    fn get_required(&self, session_id: &str) -> StorageResult<AgentSessionRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, status, stage, checkpoint_id, created_at, updated_at
                 FROM agent_sessions WHERE id = ?1",
                params![session_id],
                map_agent_session_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("agent session {session_id}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEventRecord {
    pub event_id: String,
    pub task_id: String,
    pub event_type: String,
    pub stage: String,
    pub message: String,
    pub created_at: String,
    pub payload: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewAgentEvent<'a> {
    pub event_id: &'a str,
    pub task_id: &'a str,
    pub event_type: &'a str,
    pub stage: &'a str,
    pub message: &'a str,
    pub payload: &'a str,
}

pub struct AgentEventRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> AgentEventRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn create(&self, event: NewAgentEvent<'_>) -> StorageResult<AgentEventRecord> {
        self.connection.execute(
            "INSERT INTO agent_events (
                event_id, task_id, event_type, stage, message, created_at, payload
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.event_id,
                event.task_id,
                event.event_type,
                event.stage,
                event.message,
                now_text(),
                event.payload,
            ],
        )?;

        self.get_required(event.event_id)
    }

    pub fn list_for_task(&self, task_id: &str) -> StorageResult<Vec<AgentEventRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT event_id, task_id, event_type, stage, message, created_at, payload
             FROM agent_events
             WHERE task_id = ?1
             ORDER BY created_at, event_id",
        )?;
        let rows = statement.query_map(params![task_id], map_agent_event_record)?;
        let mut events = Vec::new();

        for row in rows {
            events.push(row?);
        }

        Ok(events)
    }

    fn get_required(&self, event_id: &str) -> StorageResult<AgentEventRecord> {
        self.connection
            .query_row(
                "SELECT event_id, task_id, event_type, stage, message, created_at, payload
                 FROM agent_events WHERE event_id = ?1",
                params![event_id],
                map_agent_event_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("agent event {event_id}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationRoundRecord {
    pub id: String,
    pub task_id: String,
    pub round_index: i64,
    pub status: String,
    pub command_run_id: Option<String>,
    pub analysis: String,
    pub repair_summary: String,
    pub validation_summary: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewValidationRound<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub round_index: i64,
    pub status: &'a str,
    pub command_run_id: Option<&'a str>,
    pub analysis: &'a str,
    pub repair_summary: &'a str,
    pub validation_summary: &'a str,
}

pub struct ValidationRoundRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> ValidationRoundRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn record(&self, round: NewValidationRound<'_>) -> StorageResult<ValidationRoundRecord> {
        let now = now_text();
        self.connection.execute(
            "INSERT INTO validation_rounds (
                id, task_id, round_index, status, command_run_id, analysis,
                repair_summary, validation_summary, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                round.id,
                round.task_id,
                round.round_index,
                round.status,
                round.command_run_id,
                round.analysis,
                round.repair_summary,
                round.validation_summary,
                now,
            ],
        )?;

        self.get_required(round.id)
    }

    pub fn list_for_task(&self, task_id: &str) -> StorageResult<Vec<ValidationRoundRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, round_index, status, command_run_id, analysis,
                repair_summary, validation_summary, created_at, updated_at
             FROM validation_rounds
             WHERE task_id = ?1
             ORDER BY round_index, created_at, id",
        )?;
        let rows = statement.query_map(params![task_id], map_validation_round_record)?;
        let mut rounds = Vec::new();

        for row in rows {
            rounds.push(row?);
        }

        Ok(rounds)
    }

    fn get_required(&self, round_id: &str) -> StorageResult<ValidationRoundRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, round_index, status, command_run_id, analysis,
                    repair_summary, validation_summary, created_at, updated_at
                 FROM validation_rounds WHERE id = ?1",
                params![round_id],
                map_validation_round_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("validation round {round_id}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeRecord {
    pub id: String,
    pub task_id: String,
    pub status: String,
    pub target_branch: String,
    pub source_branch: String,
    pub commit_sha: String,
    pub commit_message: String,
    pub conflict_files: String,
    pub error_reason: Option<String>,
    pub record_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewMergeRecord<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub status: &'a str,
    pub target_branch: &'a str,
    pub source_branch: &'a str,
    pub commit_sha: &'a str,
    pub commit_message: &'a str,
    pub conflict_files: &'a str,
    pub error_reason: Option<&'a str>,
    pub record_path: Option<&'a str>,
}

pub struct MergeRecordRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> MergeRecordRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn record(&self, record: NewMergeRecord<'_>) -> StorageResult<MergeRecord> {
        self.connection.execute(
            "INSERT INTO merge_records (
                id, task_id, status, target_branch, source_branch, commit_sha,
                commit_message, conflict_files, error_reason, record_path, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                record.id,
                record.task_id,
                record.status,
                record.target_branch,
                record.source_branch,
                record.commit_sha,
                record.commit_message,
                record.conflict_files,
                record.error_reason,
                record.record_path,
                now_text(),
            ],
        )?;

        self.get_required(record.id)
    }

    pub fn list_for_task(&self, task_id: &str) -> StorageResult<Vec<MergeRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, status, target_branch, source_branch, commit_sha,
                commit_message, conflict_files, error_reason, record_path, created_at
             FROM merge_records
             WHERE task_id = ?1
             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map(params![task_id], map_merge_record)?;
        let mut records = Vec::new();

        for row in rows {
            records.push(row?);
        }

        Ok(records)
    }

    fn get_required(&self, record_id: &str) -> StorageResult<MergeRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, status, target_branch, source_branch, commit_sha,
                    commit_message, conflict_files, error_reason, record_path, created_at
                 FROM merge_records WHERE id = ?1",
                params![record_id],
                map_merge_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("merge record {record_id}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonalProfileRecord {
    pub id: String,
    pub name: String,
    pub scope: String,
    pub scope_id: Option<String>,
    pub mode: String,
    pub model_id: Option<String>,
    pub reasoning_effort: String,
    pub permission_level: String,
    pub network_policy: String,
    pub privacy_mode: String,
    pub token_budget_total: i64,
    pub token_budget_per_call: i64,
    pub validation_policy: String,
    pub output_language: String,
    pub memory_scope: String,
    pub quality_gate_policy: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub struct PersonalProfileRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> PersonalProfileRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn ensure_default_active_profile(&self) -> StorageResult<()> {
        let now = now_text();
        self.connection.execute(
            "INSERT OR IGNORE INTO personal_profiles (
                id, name, scope, scope_id, mode, model_id, reasoning_effort,
                permission_level, network_policy, privacy_mode, token_budget_total,
                token_budget_per_call, validation_policy, output_language, memory_scope,
                quality_gate_policy, is_active, created_at, updated_at
             ) VALUES (
                ?1, 'Default', 'global', NULL, 'standard', 'model-default', 'balanced',
                'worktree_write', 'approval_required', 'standard', 120000,
                24000, 'auto', 'zh-CN', 'task', 'strict', 1, ?2, ?2
             )",
            params![DEFAULT_PROFILE_ID, now],
        )?;

        let active_count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM personal_profiles WHERE is_active = 1",
            [],
            |row| row.get(0),
        )?;
        if active_count == 0 {
            self.connection.execute(
                "UPDATE personal_profiles
                 SET is_active = CASE WHEN id = ?1 THEN 1 ELSE 0 END, updated_at = ?2",
                params![DEFAULT_PROFILE_ID, now_text()],
            )?;
        }

        Ok(())
    }

    pub fn active_profile(&self) -> StorageResult<PersonalProfileRecord> {
        self.connection
            .query_row(
                "SELECT id, name, scope, scope_id, mode, model_id, reasoning_effort,
                    permission_level, network_policy, privacy_mode, token_budget_total,
                    token_budget_per_call, validation_policy, output_language, memory_scope,
                    quality_gate_policy, is_active, created_at, updated_at
                 FROM personal_profiles
                 WHERE is_active = 1
                 ORDER BY updated_at DESC, id
                 LIMIT 1",
                [],
                map_personal_profile_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound("active profile".to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunContractRecord {
    pub id: String,
    pub task_id: String,
    pub profile_id: Option<String>,
    pub mode: String,
    pub model_id: Option<String>,
    pub reasoning_effort: String,
    pub permission_level: String,
    pub network_policy: String,
    pub allowed_paths_json: String,
    pub allowed_commands_json: String,
    pub validation_command: Option<String>,
    pub token_budget_total: i64,
    pub token_budget_per_call: i64,
    pub output_language: String,
    pub memory_scope: String,
    pub budget_overflow_policy: String,
    pub contract_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewRunContract<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub profile_id: Option<&'a str>,
    pub mode: &'a str,
    pub model_id: Option<&'a str>,
    pub reasoning_effort: &'a str,
    pub permission_level: &'a str,
    pub network_policy: &'a str,
    pub allowed_paths_json: &'a str,
    pub allowed_commands_json: &'a str,
    pub validation_command: Option<&'a str>,
    pub token_budget_total: i64,
    pub token_budget_per_call: i64,
    pub output_language: &'a str,
    pub memory_scope: &'a str,
    pub budget_overflow_policy: &'a str,
    pub contract_json: &'a str,
}

pub struct RunContractRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> RunContractRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn upsert(&self, contract: NewRunContract<'_>) -> StorageResult<RunContractRecord> {
        let now = now_text();
        self.connection.execute(
            "INSERT INTO run_contracts (
                id, task_id, profile_id, mode, model_id, reasoning_effort, permission_level,
                network_policy, allowed_paths_json, allowed_commands_json, validation_command,
                token_budget_total, token_budget_per_call, output_language, memory_scope,
                budget_overflow_policy, contract_json, created_at, updated_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?18
             )
             ON CONFLICT(task_id) DO UPDATE SET
                profile_id = excluded.profile_id,
                mode = excluded.mode,
                model_id = excluded.model_id,
                reasoning_effort = excluded.reasoning_effort,
                permission_level = excluded.permission_level,
                network_policy = excluded.network_policy,
                allowed_paths_json = excluded.allowed_paths_json,
                allowed_commands_json = excluded.allowed_commands_json,
                validation_command = excluded.validation_command,
                token_budget_total = excluded.token_budget_total,
                token_budget_per_call = excluded.token_budget_per_call,
                output_language = excluded.output_language,
                memory_scope = excluded.memory_scope,
                budget_overflow_policy = excluded.budget_overflow_policy,
                contract_json = excluded.contract_json,
                updated_at = excluded.updated_at",
            params![
                contract.id,
                contract.task_id,
                contract.profile_id,
                contract.mode,
                contract.model_id,
                contract.reasoning_effort,
                contract.permission_level,
                contract.network_policy,
                contract.allowed_paths_json,
                contract.allowed_commands_json,
                contract.validation_command,
                contract.token_budget_total,
                contract.token_budget_per_call,
                contract.output_language,
                contract.memory_scope,
                contract.budget_overflow_policy,
                contract.contract_json,
                now,
            ],
        )?;

        self.get_for_task(contract.task_id)?.ok_or_else(|| {
            StorageError::NotFound(format!("run contract for task {}", contract.task_id))
        })
    }

    pub fn get_for_task(&self, task_id: &str) -> StorageResult<Option<RunContractRecord>> {
        self.connection
            .query_row(
                "SELECT id, task_id, profile_id, mode, model_id, reasoning_effort,
                    permission_level, network_policy, allowed_paths_json, allowed_commands_json,
                    validation_command, token_budget_total, token_budget_per_call, output_language,
                    memory_scope, budget_overflow_policy, contract_json, created_at, updated_at
                 FROM run_contracts
                 WHERE task_id = ?1",
                params![task_id],
                map_run_contract_record,
            )
            .optional()
            .map_err(StorageError::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyLedgerEntryRecord {
    pub id: String,
    pub task_id: String,
    pub event_type: String,
    pub data_kind: String,
    pub source_type: String,
    pub source_ref: String,
    pub destination: String,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub action: String,
    pub sensitivity_level: String,
    pub findings_json: String,
    pub redacted: bool,
    pub blocked: bool,
    pub reason: String,
    pub size_bytes: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewPrivacyLedgerEntry<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub event_type: &'a str,
    pub data_kind: &'a str,
    pub source_type: &'a str,
    pub source_ref: &'a str,
    pub destination: &'a str,
    pub provider: Option<&'a str>,
    pub model_id: Option<&'a str>,
    pub action: &'a str,
    pub sensitivity_level: &'a str,
    pub findings_json: &'a str,
    pub redacted: bool,
    pub blocked: bool,
    pub reason: &'a str,
    pub size_bytes: i64,
}

pub struct PrivacyLedgerRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> PrivacyLedgerRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn record(
        &self,
        entry: NewPrivacyLedgerEntry<'_>,
    ) -> StorageResult<PrivacyLedgerEntryRecord> {
        self.connection.execute(
            "INSERT INTO privacy_ledger_entries (
                id, task_id, event_type, data_kind, source_type, source_ref, destination,
                provider, model_id, action, sensitivity_level, findings_json, redacted,
                blocked, reason, size_bytes, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                entry.id,
                entry.task_id,
                entry.event_type,
                entry.data_kind,
                entry.source_type,
                entry.source_ref,
                entry.destination,
                entry.provider,
                entry.model_id,
                entry.action,
                entry.sensitivity_level,
                entry.findings_json,
                bool_to_i64(entry.redacted),
                bool_to_i64(entry.blocked),
                entry.reason,
                entry.size_bytes,
                now_text(),
            ],
        )?;

        self.get_required(entry.id)
    }

    pub fn list_for_task(&self, task_id: &str) -> StorageResult<Vec<PrivacyLedgerEntryRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, event_type, data_kind, source_type, source_ref, destination,
                provider, model_id, action, sensitivity_level, findings_json, redacted,
                blocked, reason, size_bytes, created_at
             FROM privacy_ledger_entries
             WHERE task_id = ?1
             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map(params![task_id], map_privacy_ledger_entry_record)?;
        let mut entries = Vec::new();

        for row in rows {
            entries.push(row?);
        }

        Ok(entries)
    }

    fn get_required(&self, entry_id: &str) -> StorageResult<PrivacyLedgerEntryRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, event_type, data_kind, source_type, source_ref, destination,
                    provider, model_id, action, sensitivity_level, findings_json, redacted,
                    blocked, reason, size_bytes, created_at
                 FROM privacy_ledger_entries WHERE id = ?1",
                params![entry_id],
                map_privacy_ledger_entry_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("privacy ledger entry {entry_id}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenBudgetRecord {
    pub id: String,
    pub task_id: String,
    pub run_id: Option<String>,
    pub call_type: String,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub phase: String,
    pub input_tokens_estimate: i64,
    pub output_tokens_estimate: i64,
    pub total_tokens_estimate: i64,
    pub budget_limit: i64,
    pub budget_remaining: i64,
    pub overflow_policy: String,
    pub quality_fallback: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewTokenBudgetRecord<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub run_id: Option<&'a str>,
    pub call_type: &'a str,
    pub provider: Option<&'a str>,
    pub model_id: Option<&'a str>,
    pub phase: &'a str,
    pub input_tokens_estimate: i64,
    pub output_tokens_estimate: i64,
    pub total_tokens_estimate: i64,
    pub budget_limit: i64,
    pub budget_remaining: i64,
    pub overflow_policy: &'a str,
    pub quality_fallback: &'a str,
}

pub struct TokenBudgetRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> TokenBudgetRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn record(&self, record: NewTokenBudgetRecord<'_>) -> StorageResult<TokenBudgetRecord> {
        self.connection.execute(
            "INSERT INTO token_budget_records (
                id, task_id, run_id, call_type, provider, model_id, phase, input_tokens_estimate,
                output_tokens_estimate, total_tokens_estimate, budget_limit, budget_remaining,
                overflow_policy, quality_fallback, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                record.id,
                record.task_id,
                record.run_id,
                record.call_type,
                record.provider,
                record.model_id,
                record.phase,
                record.input_tokens_estimate,
                record.output_tokens_estimate,
                record.total_tokens_estimate,
                record.budget_limit,
                record.budget_remaining,
                record.overflow_policy,
                record.quality_fallback,
                now_text(),
            ],
        )?;

        self.get_required(record.id)
    }

    pub fn list_for_task(&self, task_id: &str) -> StorageResult<Vec<TokenBudgetRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, run_id, call_type, provider, model_id, phase,
                input_tokens_estimate, output_tokens_estimate, total_tokens_estimate,
                budget_limit, budget_remaining, overflow_policy, quality_fallback, created_at
             FROM token_budget_records
             WHERE task_id = ?1
             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map(params![task_id], map_token_budget_record)?;
        let mut records = Vec::new();

        for row in rows {
            records.push(row?);
        }

        Ok(records)
    }

    fn get_required(&self, record_id: &str) -> StorageResult<TokenBudgetRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, run_id, call_type, provider, model_id, phase,
                    input_tokens_estimate, output_tokens_estimate, total_tokens_estimate,
                    budget_limit, budget_remaining, overflow_policy, quality_fallback, created_at
                 FROM token_budget_records WHERE id = ?1",
                params![record_id],
                map_token_budget_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("token budget record {record_id}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSourceRecord {
    pub id: String,
    pub task_id: String,
    pub run_id: Option<String>,
    pub source_type: String,
    pub source_ref: String,
    pub layer: String,
    pub included: bool,
    pub tokens_estimate: i64,
    pub sensitivity_level: String,
    pub redacted: bool,
    pub blocked: bool,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy)]
pub struct NewContextSource<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub run_id: Option<&'a str>,
    pub source_type: &'a str,
    pub source_ref: &'a str,
    pub layer: &'a str,
    pub included: bool,
    pub tokens_estimate: i64,
    pub sensitivity_level: &'a str,
    pub redacted: bool,
    pub blocked: bool,
    pub reason: &'a str,
}

pub struct ContextSourceRepository<'conn> {
    connection: &'conn Connection,
}

impl<'conn> ContextSourceRepository<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn record(&self, source: NewContextSource<'_>) -> StorageResult<ContextSourceRecord> {
        self.connection.execute(
            "INSERT INTO context_sources (
                id, task_id, run_id, source_type, source_ref, layer, included,
                tokens_estimate, sensitivity_level, redacted, blocked, reason, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                source.id,
                source.task_id,
                source.run_id,
                source.source_type,
                source.source_ref,
                source.layer,
                bool_to_i64(source.included),
                source.tokens_estimate,
                source.sensitivity_level,
                bool_to_i64(source.redacted),
                bool_to_i64(source.blocked),
                source.reason,
                now_text(),
            ],
        )?;

        self.get_required(source.id)
    }

    pub fn list_for_task(&self, task_id: &str) -> StorageResult<Vec<ContextSourceRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, run_id, source_type, source_ref, layer, included,
                tokens_estimate, sensitivity_level, redacted, blocked, reason, created_at
             FROM context_sources
             WHERE task_id = ?1
             ORDER BY created_at, id",
        )?;
        let rows = statement.query_map(params![task_id], map_context_source_record)?;
        let mut sources = Vec::new();

        for row in rows {
            sources.push(row?);
        }

        Ok(sources)
    }

    fn get_required(&self, source_id: &str) -> StorageResult<ContextSourceRecord> {
        self.connection
            .query_row(
                "SELECT id, task_id, run_id, source_type, source_ref, layer, included,
                    tokens_estimate, sensitivity_level, redacted, blocked, reason, created_at
                 FROM context_sources WHERE id = ?1",
                params![source_id],
                map_context_source_record,
            )
            .optional()?
            .ok_or_else(|| StorageError::NotFound(format!("context source {source_id}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupReadiness {
    pub task_id: String,
    pub final_diff_preserved: bool,
    pub approval_records_preserved: bool,
}

pub struct CleanupGuard<'conn> {
    connection: &'conn Connection,
}

impl<'conn> CleanupGuard<'conn> {
    pub fn new(connection: &'conn Connection) -> Self {
        Self { connection }
    }

    pub fn validate_task_cleanup(&self, task_id: &str) -> StorageResult<CleanupReadiness> {
        TaskRepository::new(self.connection).get_required(task_id)?;

        let policy = StoragePolicyRepository::new(self.connection).default_policy()?;
        let mut reasons = Vec::new();

        if !policy.keep_final_diff_forever {
            reasons.push("final diff retention policy is disabled".to_string());
        }

        let artifact_diff_count: i64 = self.connection.query_row(
            "SELECT COUNT(*)
             FROM artifacts
             WHERE task_id = ?1 AND diff_path IS NOT NULL AND trim(diff_path) != ''",
            params![task_id],
            |row| row.get(0),
        )?;
        let artifact_file_diff_count: i64 = self.connection.query_row(
            "SELECT COUNT(*)
             FROM artifact_files
             WHERE task_id = ?1
               AND type IN ('diff', 'patch')
               AND retention_class = 'permanent'
               AND path IS NOT NULL
               AND trim(path) != ''",
            params![task_id],
            |row| row.get(0),
        )?;
        let final_diff_preserved = artifact_diff_count > 0 || artifact_file_diff_count > 0;

        if !final_diff_preserved {
            reasons.push("final diff is not persisted".to_string());
        }

        if !policy.keep_approval_records_forever {
            reasons.push("approval records retention policy is disabled".to_string());
        }

        if !reasons.is_empty() {
            return Err(StorageError::UnsafeCleanup {
                task_id: task_id.to_string(),
                reasons,
            });
        }

        Ok(CleanupReadiness {
            task_id: task_id.to_string(),
            final_diff_preserved,
            approval_records_preserved: policy.keep_approval_records_forever,
        })
    }

    pub fn remove_temporary_artifact_file_records(&self, task_id: &str) -> StorageResult<usize> {
        self.validate_task_cleanup(task_id)?;
        let removed = self.connection.execute(
            "DELETE FROM artifact_files
             WHERE task_id = ?1 AND retention_class = 'temporary'",
            params![task_id],
        )?;
        Ok(removed)
    }
}

fn enable_foreign_keys(connection: &Connection) -> StorageResult<()> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

fn now_text() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    seconds.to_string()
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn i64_to_bool(value: i64) -> bool {
    value != 0
}

fn directory_size(path: &Path) -> StorageResult<u64> {
    if !path.exists() {
        return Ok(0);
    }

    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(0);
    }

    if metadata.is_file() {
        return Ok(metadata.len());
    }

    if !metadata.is_dir() {
        return Ok(0);
    }

    let mut total = 0;
    for entry in fs::read_dir(path)? {
        total += directory_size(&entry?.path())?;
    }
    Ok(total)
}

fn file_size_if_present(path: &Path) -> StorageResult<u64> {
    if !path.exists() {
        return Ok(0);
    }

    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_file() {
        Ok(metadata.len())
    } else {
        Ok(0)
    }
}

fn map_task_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        task_type: row.get(3)?,
        status: row.get(4)?,
        repository_path: row.get(5)?,
        worktree_path: row.get(6)?,
        branch_name: row.get(7)?,
        model_id: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        completed_at: row.get(11)?,
    })
}

fn map_todo_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TodoRecord> {
    Ok(TodoRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        status: row.get(4)?,
        started_at: row.get(5)?,
        completed_at: row.get(6)?,
        error_message: row.get(7)?,
    })
}

fn map_command_run_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommandRunRecord> {
    Ok(CommandRunRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        purpose: row.get(10)?,
        command: row.get(2)?,
        cwd: row.get(3)?,
        status: row.get(4)?,
        stdout_path: row.get(5)?,
        stderr_path: row.get(6)?,
        exit_code: row.get(7)?,
        duration_ms: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn map_approval_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ApprovalRecord> {
    Ok(ApprovalRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        approval_type: row.get(2)?,
        risk_level: row.get(3)?,
        content: row.get(4)?,
        reason: row.get(5)?,
        decision: row.get(6)?,
        comment: row.get(7)?,
        created_at: row.get(8)?,
        decided_at: row.get(9)?,
    })
}

fn map_artifact_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactRecord> {
    Ok(ArtifactRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        changed_files: row.get(2)?,
        diff_path: row.get(3)?,
        test_report_path: row.get(4)?,
        screenshots: row.get(5)?,
        summary: row.get(6)?,
        commit_message: row.get(7)?,
    })
}

fn map_model_config_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelConfigRecord> {
    Ok(ModelConfigRecord {
        id: row.get(0)?,
        provider: row.get(1)?,
        base_url: row.get(2)?,
        model_name: row.get(3)?,
        api_key_secret_ref: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn map_storage_policy(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoragePolicy> {
    Ok(StoragePolicy {
        id: row.get(0)?,
        scope: row.get(1)?,
        keep_recent_messages: row.get(2)?,
        raw_log_retention_days: row.get(3)?,
        screenshot_retention_days: row.get(4)?,
        temporary_context_retention_days: row.get(5)?,
        auto_cleanup_worktree_after_merge: i64_to_bool(row.get(6)?),
        keep_final_diff_forever: i64_to_bool(row.get(7)?),
        keep_approval_records_forever: i64_to_bool(row.get(8)?),
        updated_at: row.get(9)?,
    })
}

fn map_conversation_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationRecord> {
    Ok(ConversationRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        repository_path: row.get(2)?,
        title: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        last_message_id: row.get(6)?,
    })
}

fn map_message_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<MessageRecord> {
    Ok(MessageRecord {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        task_id: row.get(2)?,
        role: row.get(3)?,
        content: row.get(4)?,
        token_count: row.get(5)?,
        is_pinned: i64_to_bool(row.get(6)?),
        retention_class: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn map_memory_item_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryItemRecord> {
    Ok(MemoryItemRecord {
        id: row.get(0)?,
        scope: row.get(1)?,
        scope_id: row.get(2)?,
        key: row.get(3)?,
        value: row.get(4)?,
        confidence: row.get(5)?,
        source: row.get(6)?,
        is_user_editable: i64_to_bool(row.get(7)?),
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn map_artifact_file_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactFileRecord> {
    Ok(ArtifactFileRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        artifact_id: row.get(2)?,
        file_type: row.get(3)?,
        path: row.get(4)?,
        size_bytes: row.get(5)?,
        compressed: i64_to_bool(row.get(6)?),
        retention_class: row.get(7)?,
        created_at: row.get(8)?,
        expires_at: row.get(9)?,
    })
}

fn map_agent_session_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentSessionRecord> {
    Ok(AgentSessionRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        status: row.get(2)?,
        stage: row.get(3)?,
        checkpoint_id: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn map_agent_event_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentEventRecord> {
    Ok(AgentEventRecord {
        event_id: row.get(0)?,
        task_id: row.get(1)?,
        event_type: row.get(2)?,
        stage: row.get(3)?,
        message: row.get(4)?,
        created_at: row.get(5)?,
        payload: row.get(6)?,
    })
}

fn map_validation_round_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ValidationRoundRecord> {
    Ok(ValidationRoundRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        round_index: row.get(2)?,
        status: row.get(3)?,
        command_run_id: row.get(4)?,
        analysis: row.get(5)?,
        repair_summary: row.get(6)?,
        validation_summary: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn map_merge_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<MergeRecord> {
    Ok(MergeRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        status: row.get(2)?,
        target_branch: row.get(3)?,
        source_branch: row.get(4)?,
        commit_sha: row.get(5)?,
        commit_message: row.get(6)?,
        conflict_files: row.get(7)?,
        error_reason: row.get(8)?,
        record_path: row.get(9)?,
        created_at: row.get(10)?,
    })
}

fn map_personal_profile_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersonalProfileRecord> {
    Ok(PersonalProfileRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        scope: row.get(2)?,
        scope_id: row.get(3)?,
        mode: row.get(4)?,
        model_id: row.get(5)?,
        reasoning_effort: row.get(6)?,
        permission_level: row.get(7)?,
        network_policy: row.get(8)?,
        privacy_mode: row.get(9)?,
        token_budget_total: row.get(10)?,
        token_budget_per_call: row.get(11)?,
        validation_policy: row.get(12)?,
        output_language: row.get(13)?,
        memory_scope: row.get(14)?,
        quality_gate_policy: row.get(15)?,
        is_active: i64_to_bool(row.get(16)?),
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn map_run_contract_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunContractRecord> {
    Ok(RunContractRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        profile_id: row.get(2)?,
        mode: row.get(3)?,
        model_id: row.get(4)?,
        reasoning_effort: row.get(5)?,
        permission_level: row.get(6)?,
        network_policy: row.get(7)?,
        allowed_paths_json: row.get(8)?,
        allowed_commands_json: row.get(9)?,
        validation_command: row.get(10)?,
        token_budget_total: row.get(11)?,
        token_budget_per_call: row.get(12)?,
        output_language: row.get(13)?,
        memory_scope: row.get(14)?,
        budget_overflow_policy: row.get(15)?,
        contract_json: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn map_privacy_ledger_entry_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PrivacyLedgerEntryRecord> {
    Ok(PrivacyLedgerEntryRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        event_type: row.get(2)?,
        data_kind: row.get(3)?,
        source_type: row.get(4)?,
        source_ref: row.get(5)?,
        destination: row.get(6)?,
        provider: row.get(7)?,
        model_id: row.get(8)?,
        action: row.get(9)?,
        sensitivity_level: row.get(10)?,
        findings_json: row.get(11)?,
        redacted: i64_to_bool(row.get(12)?),
        blocked: i64_to_bool(row.get(13)?),
        reason: row.get(14)?,
        size_bytes: row.get(15)?,
        created_at: row.get(16)?,
    })
}

fn map_token_budget_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TokenBudgetRecord> {
    Ok(TokenBudgetRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        run_id: row.get(2)?,
        call_type: row.get(3)?,
        provider: row.get(4)?,
        model_id: row.get(5)?,
        phase: row.get(6)?,
        input_tokens_estimate: row.get(7)?,
        output_tokens_estimate: row.get(8)?,
        total_tokens_estimate: row.get(9)?,
        budget_limit: row.get(10)?,
        budget_remaining: row.get(11)?,
        overflow_policy: row.get(12)?,
        quality_fallback: row.get(13)?,
        created_at: row.get(14)?,
    })
}

fn map_context_source_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContextSourceRecord> {
    Ok(ContextSourceRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        run_id: row.get(2)?,
        source_type: row.get(3)?,
        source_ref: row.get(4)?,
        layer: row.get(5)?,
        included: i64_to_bool(row.get(6)?),
        tokens_estimate: row.get(7)?,
        sensitivity_level: row.get(8)?,
        redacted: i64_to_bool(row.get(9)?),
        blocked: i64_to_bool(row.get(10)?),
        reason: row.get(11)?,
        created_at: row.get(12)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated_store() -> SqliteStore {
        let store = SqliteStore::open_in_memory().expect("open in-memory sqlite");
        store.migrate().expect("run migrations");
        store
    }

    fn create_test_task(store: &SqliteStore) {
        TaskRepository::new(store.connection())
            .create(NewTask {
                id: "task-001",
                title: "Fixture task",
                description: "Task for storage tests",
                task_type: "custom",
                status: "created",
                repository_path: "D:/projects/demo",
                worktree_path: Some("D:/codemax/app-data/worktrees/task-001"),
                branch_name: Some("agent/task-001"),
                model_id: None,
            })
            .expect("create fixture task");
    }

    #[test]
    fn migrates_s2_schema_and_seeds_default_policy() {
        let store = migrated_store();

        let tables = store.table_names().expect("read table names");
        for table in [
            "schema_migrations",
            "tasks",
            "todos",
            "command_runs",
            "approvals",
            "artifacts",
            "model_configs",
            "app_settings",
            "conversations",
            "messages",
            "conversation_summaries",
            "memory_items",
            "storage_policies",
            "artifact_files",
            "quality_gate_results",
            "proof_packs",
            "delivery_scores",
            "agent_sessions",
            "agent_events",
            "validation_rounds",
            "merge_records",
            "personal_profiles",
            "run_contracts",
            "contract_breach_records",
            "privacy_ledger_entries",
            "token_budget_records",
            "context_sources",
        ] {
            assert!(tables.contains(&table.to_string()), "missing {table}");
        }

        let policies = StoragePolicyRepository::new(store.connection());
        let policy = policies.default_policy().expect("default storage policy");

        assert_eq!(policy.keep_recent_messages, 50);
        assert_eq!(policy.raw_log_retention_days, 30);
        assert_eq!(policy.screenshot_retention_days, 30);
        assert_eq!(policy.temporary_context_retention_days, 7);
        assert!(policy.keep_final_diff_forever);
        assert!(policy.keep_approval_records_forever);

        let profile = PersonalProfileRepository::new(store.connection())
            .active_profile()
            .expect("default active profile");
        assert_eq!(profile.id, DEFAULT_PROFILE_ID);
        assert!(profile.is_active);
    }

    #[test]
    fn migrates_legacy_0001_databases_to_s12_evidence_tables() {
        let store = SqliteStore::open_in_memory().expect("open in-memory sqlite");
        store
            .connection()
            .execute_batch(
                "CREATE TABLE schema_migrations (
                    version TEXT PRIMARY KEY,
                    applied_at TEXT NOT NULL
                );
                INSERT INTO schema_migrations (version, applied_at)
                VALUES ('0001_initial', '1783372800');
                CREATE TABLE storage_policies (
                    id TEXT PRIMARY KEY,
                    scope TEXT NOT NULL,
                    keep_recent_messages INTEGER NOT NULL DEFAULT 50,
                    raw_log_retention_days INTEGER NOT NULL DEFAULT 30,
                    screenshot_retention_days INTEGER NOT NULL DEFAULT 30,
                    temporary_context_retention_days INTEGER NOT NULL DEFAULT 7,
                    auto_cleanup_worktree_after_merge INTEGER NOT NULL DEFAULT 0,
                    keep_final_diff_forever INTEGER NOT NULL DEFAULT 1,
                    keep_approval_records_forever INTEGER NOT NULL DEFAULT 1,
                    updated_at TEXT NOT NULL
                );",
            )
            .expect("seed legacy 0001 database");

        store.migrate().expect("run follow-up migrations");

        let tables = store.table_names().expect("read table names");
        for table in [
            "quality_gate_results",
            "proof_packs",
            "delivery_scores",
            "agent_sessions",
            "agent_events",
            "validation_rounds",
            "merge_records",
        ] {
            assert!(tables.contains(&table.to_string()), "missing {table}");
        }
        let has_s12_version = store
            .connection()
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version = '0002_s12_evidence'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .expect("query migration version")
            .is_some();
        assert!(has_s12_version);
        let has_a_chain_version = store
            .connection()
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version = '0003_a_task_chain'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .expect("query task chain migration version")
            .is_some();
        assert!(has_a_chain_version);
    }

    #[test]
    fn task_repository_round_trips_task_metadata() {
        let store = migrated_store();
        let tasks = TaskRepository::new(store.connection());

        let created = tasks
            .create(NewTask {
                id: "task-001",
                title: "Fix login bug",
                description: "Investigate and repair the login flow",
                task_type: "bugfix",
                status: "created",
                repository_path: "D:/projects/demo",
                worktree_path: Some("D:/codemax/app-data/worktrees/task-001"),
                branch_name: Some("agent/task-001"),
                model_id: Some("model-default"),
            })
            .expect("create task");

        assert_eq!(created.id, "task-001");
        assert_eq!(created.repository_path, "D:/projects/demo");
        assert_eq!(
            created.worktree_path.as_deref(),
            Some("D:/codemax/app-data/worktrees/task-001")
        );

        tasks
            .update_status("task-001", "completed", Some("2026-07-04T12:00:00Z"))
            .expect("update task status");

        let loaded = tasks
            .get("task-001")
            .expect("load task")
            .expect("task exists");
        assert_eq!(loaded.status, "completed");
        assert_eq!(loaded.completed_at.as_deref(), Some("2026-07-04T12:00:00Z"));

        let updated = tasks
            .update_worktree_metadata(
                "task-001",
                "D:/codemax/app-data/worktrees/task-001",
                "agent/task-001",
            )
            .expect("update worktree metadata");
        assert_eq!(
            updated.worktree_path.as_deref(),
            Some("D:/codemax/app-data/worktrees/task-001")
        );
        assert_eq!(updated.branch_name.as_deref(), Some("agent/task-001"));

        let cleared = tasks
            .clear_worktree_metadata("task-001")
            .expect("clear worktree metadata");
        assert_eq!(cleared.worktree_path, None);
        assert_eq!(cleared.branch_name, None);
    }

    #[test]
    fn core_repositories_round_trip_s2_records() {
        let store = migrated_store();
        create_test_task(&store);

        let todos = TodoRepository::new(store.connection());
        todos
            .create(NewTodo {
                id: "todo-001",
                task_id: "task-001",
                title: "Plan the fix",
                description: "Understand the repository before editing",
                status: "pending",
            })
            .expect("create todo");
        todos
            .update_status("todo-001", "completed", None)
            .expect("update todo");
        let todo_records = todos.list_for_task("task-001").expect("list todos");
        assert_eq!(todo_records[0].status, "completed");

        let command_runs = CommandRunRepository::new(store.connection());
        command_runs
            .record(NewCommandRun {
                id: "command-001",
                task_id: "task-001",
                purpose: "validation",
                command: "npm test",
                cwd: "D:/projects/demo",
                status: "passed",
                stdout_path: Some("app-data/tasks/task-001/logs/stdout.log"),
                stderr_path: Some("app-data/tasks/task-001/logs/stderr.log"),
                exit_code: Some(0),
                duration_ms: Some(1200),
            })
            .expect("record command");
        assert_eq!(
            command_runs
                .list_for_task("task-001")
                .expect("list commands")[0]
                .exit_code,
            Some(0)
        );

        let approvals = ApprovalRepository::new(store.connection());
        approvals
            .create(NewApproval {
                id: "approval-001",
                task_id: "task-001",
                approval_type: "command",
                risk_level: "high",
                content: "Run package install",
                reason: "Dependency install changes local disk state",
            })
            .expect("create approval");
        approvals
            .decide(
                "approval-001",
                "rejected",
                Some("Use existing dependencies"),
            )
            .expect("decide approval");
        assert_eq!(
            approvals.list_for_task("task-001").expect("list approvals")[0]
                .decision
                .as_deref(),
            Some("rejected")
        );

        let artifacts = ArtifactRepository::new(store.connection());
        artifacts
            .record_artifact(NewArtifact {
                id: "artifact-001",
                task_id: "task-001",
                changed_files: "[\"src/lib.rs\"]",
                diff_path: Some("app-data/tasks/task-001/diff.patch"),
                test_report_path: Some("app-data/tasks/task-001/report.json"),
                screenshots: "[]",
                summary: "S2 data layer is ready",
                commit_message: "feat: add local storage model",
            })
            .expect("record artifact");
        assert_eq!(
            artifacts
                .artifacts_for_task("task-001")
                .expect("list artifacts")[0]
                .commit_message,
            "feat: add local storage model"
        );

        let models = ModelConfigRepository::new(store.connection());
        models
            .save(NewModelConfig {
                id: "model-default",
                provider: "openai-compatible",
                base_url: "https://api.example.test/v1",
                model_name: "codemax-test-model",
                api_key_secret_ref: Some("secret://model-default"),
            })
            .expect("save model config");
        assert_eq!(
            models
                .get("model-default")
                .expect("get model config")
                .unwrap()
                .model_name,
            "codemax-test-model"
        );

        let settings = AppSettingsRepository::new(store.connection());
        settings.set("maxRepairRounds", "5").expect("save setting");
        assert_eq!(
            settings
                .get("maxRepairRounds")
                .expect("read setting")
                .as_deref(),
            Some("5")
        );
    }

    #[test]
    fn memory_repository_loads_recent_window_and_user_editable_memories() {
        let store = migrated_store();
        let memory = MemoryRepository::new(store.connection());

        memory
            .create_conversation(NewConversation {
                id: "conversation-001",
                task_id: None,
                repository_path: Some("D:/projects/demo"),
                title: "Demo conversation",
            })
            .expect("create conversation");

        for index in 0..60 {
            memory
                .add_message(NewMessage {
                    id: &format!("message-{index:03}"),
                    conversation_id: "conversation-001",
                    task_id: None,
                    role: "user",
                    content: &format!("visible message {index}"),
                    token_count: 3,
                    is_pinned: false,
                    retention_class: "recent",
                })
                .expect("add message");
        }

        let recent = memory
            .recent_messages("conversation-001", 50)
            .expect("load recent messages");
        assert_eq!(recent.len(), 50);
        assert_eq!(recent.first().unwrap().id, "message-010");
        assert_eq!(recent.last().unwrap().id, "message-059");

        let saved = memory
            .upsert_memory_item(NewMemoryItem {
                id: "memory-001",
                scope: "repository",
                scope_id: Some("D:/projects/demo"),
                key: "defaultTestCommand",
                value: "npm test",
                confidence: 0.9,
                source: "user_setting",
                is_user_editable: true,
            })
            .expect("save memory item");
        assert!(saved.is_user_editable);

        memory
            .delete_memory_item("memory-001")
            .expect("delete memory");
        assert!(memory
            .memory_item("memory-001")
            .expect("query deleted memory")
            .is_none());
    }

    #[test]
    fn artifact_paths_and_files_keep_large_content_out_of_sqlite() {
        let store = migrated_store();
        create_test_task(&store);

        let root =
            std::env::temp_dir().join(format!("codemax-storage-test-{}", uuid::Uuid::new_v4()));
        let roots = StorageRoots::from_app_data_dir(&root);
        let paths = roots.task_artifact_paths("task-001");

        roots
            .ensure_task_artifact_dirs("task-001")
            .expect("create artifact dirs");

        assert!(paths.logs_dir.is_dir());
        assert!(paths.screenshots_dir.is_dir());
        assert!(paths.context_dir.is_dir());
        assert_eq!(paths.diff_path, paths.root.join("diff.patch"));

        let artifacts = ArtifactRepository::new(store.connection());
        artifacts
            .record_file(NewArtifactFile {
                id: "file-001",
                task_id: "task-001",
                artifact_id: None,
                file_type: "diff",
                path: paths.diff_path.to_string_lossy().as_ref(),
                size_bytes: 2048,
                compressed: false,
                retention_class: "permanent",
                expires_at: None,
            })
            .expect("record artifact file path");

        let files = artifacts
            .files_for_task("task-001")
            .expect("list artifact files");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, paths.diff_path.to_string_lossy().as_ref());

        std::fs::remove_dir_all(root).expect("clean temp artifact dirs");
    }

    #[test]
    fn cleanup_guard_requires_final_diff_and_retained_audit_records() {
        let store = migrated_store();
        create_test_task(&store);
        TaskRepository::new(store.connection())
            .update_status("task-001", "completed", Some("2026-07-04T12:00:00Z"))
            .expect("complete task");

        let guard = CleanupGuard::new(store.connection());
        let blocked = guard
            .validate_task_cleanup("task-001")
            .expect_err("cleanup must be blocked without final diff");

        assert!(blocked.to_string().contains("final diff"));
    }

    #[test]
    fn b_line_privacy_contract_budget_repositories_round_trip() {
        let store = migrated_store();
        create_test_task(&store);

        let profile = PersonalProfileRepository::new(store.connection())
            .active_profile()
            .expect("active profile exists");
        assert_eq!(profile.privacy_mode, "standard");

        let contracts = RunContractRepository::new(store.connection());
        let contract = contracts
            .upsert(NewRunContract {
                id: "contract-001",
                task_id: "task-001",
                profile_id: Some(&profile.id),
                mode: &profile.mode,
                model_id: profile.model_id.as_deref(),
                reasoning_effort: &profile.reasoning_effort,
                permission_level: &profile.permission_level,
                network_policy: &profile.network_policy,
                allowed_paths_json: "[\"D:/codemax/app-data/worktrees/task-001\"]",
                allowed_commands_json: "[\"npm test\"]",
                validation_command: Some("npm test"),
                token_budget_total: profile.token_budget_total,
                token_budget_per_call: profile.token_budget_per_call,
                output_language: &profile.output_language,
                memory_scope: &profile.memory_scope,
                budget_overflow_policy: "pause_for_approval",
                contract_json: "{\"source\":\"test\"}",
            })
            .expect("record run contract");
        assert_eq!(contract.task_id, "task-001");
        assert_eq!(contract.profile_id.as_deref(), Some(DEFAULT_PROFILE_ID));

        PrivacyLedgerRepository::new(store.connection())
            .record(NewPrivacyLedgerEntry {
                id: "privacy-001",
                task_id: "task-001",
                event_type: "model_context",
                data_kind: "task_description",
                source_type: "user_input",
                source_ref: "task.description",
                destination: "agent",
                provider: Some("local-agent"),
                model_id: Some("model-default"),
                action: "redacted",
                sensitivity_level: "high",
                findings_json: "[{\"kind\":\"api_key\"}]",
                redacted: true,
                blocked: false,
                reason: "secret token masked before context use",
                size_bytes: 42,
            })
            .expect("record privacy ledger entry");
        assert_eq!(
            PrivacyLedgerRepository::new(store.connection())
                .list_for_task("task-001")
                .expect("list privacy entries")[0]
                .action,
            "redacted"
        );

        TokenBudgetRepository::new(store.connection())
            .record(NewTokenBudgetRecord {
                id: "budget-001",
                task_id: "task-001",
                run_id: Some("agent-create"),
                call_type: "agent_task_create",
                provider: Some("local-agent"),
                model_id: Some("model-default"),
                phase: "created",
                input_tokens_estimate: 11,
                output_tokens_estimate: 0,
                total_tokens_estimate: 11,
                budget_limit: 120000,
                budget_remaining: 119989,
                overflow_policy: "pause_for_approval",
                quality_fallback: "",
            })
            .expect("record token budget");
        assert_eq!(
            TokenBudgetRepository::new(store.connection())
                .list_for_task("task-001")
                .expect("list token records")[0]
                .total_tokens_estimate,
            11
        );

        ContextSourceRepository::new(store.connection())
            .record(NewContextSource {
                id: "context-001",
                task_id: "task-001",
                run_id: Some("agent-create"),
                source_type: "user_input",
                source_ref: "task.description",
                layer: "recent_user_request",
                included: true,
                tokens_estimate: 11,
                sensitivity_level: "high",
                redacted: true,
                blocked: false,
                reason: "redacted before agent context",
            })
            .expect("record context source");
        assert!(
            ContextSourceRepository::new(store.connection())
                .list_for_task("task-001")
                .expect("list context sources")[0]
                .redacted
        );
    }
}
