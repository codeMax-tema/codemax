use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, State};

use crate::{
    agent::{AgentService, AgentServiceStatus},
    core::error::{AppResult, CommandError},
    events,
    storage::{AppSettingsRepository, ManagedStorage, ModelConfigRepository, StorageError},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub service: &'static str,
    pub status: &'static str,
    pub version: &'static str,
}

#[tauri::command]
pub fn health() -> HealthResponse {
    HealthResponse {
        service: "codemax-desktop",
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {
    pub message: &'static str,
}

#[tauri::command]
pub fn ping() -> PingResponse {
    PingResponse { message: "pong" }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageRootsResponse {
    pub app_data_dir: String,
    pub artifact_root: String,
    pub worktree_root: String,
    pub database_path: String,
}

#[tauri::command]
pub fn get_storage_roots(storage: State<'_, ManagedStorage>) -> StorageRootsResponse {
    StorageRootsResponse {
        app_data_dir: storage.roots.app_data_dir.to_string_lossy().to_string(),
        artifact_root: storage.roots.artifact_root.to_string_lossy().to_string(),
        worktree_root: storage.roots.worktree_root.to_string_lossy().to_string(),
        database_path: storage.roots.database_path().to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub fn emit_app_ready(app: AppHandle) -> Result<(), String> {
    events::emit_app_ready(&app).map_err(|error| error.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingValue {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAppSettingRequest {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupHealthItem {
    pub key: String,
    pub status: String,
    pub message_key: String,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupHealthResponse {
    pub status: String,
    pub items: Vec<StartupHealthItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsageResponse {
    pub app_data_dir: String,
    pub database_path: String,
    pub artifact_root: String,
    pub worktree_root: String,
    pub database_bytes: u64,
    pub artifact_bytes: u64,
    pub worktree_bytes: u64,
    pub logs_bytes: u64,
    pub screenshots_bytes: u64,
    pub temporary_context_bytes: u64,
    pub permanent_evidence_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupStorageRequest {
    pub logs: bool,
    pub screenshots: bool,
    pub temporary_context: bool,
    #[serde(default = "default_true")]
    pub dry_run: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupStorageResponse {
    pub dry_run: bool,
    pub scanned_files: u64,
    pub deleted_files: u64,
    pub deleted_bytes: u64,
    pub protected_bytes: u64,
}

#[tauri::command]
pub fn get_app_setting(
    storage: State<'_, ManagedStorage>,
    key: String,
) -> AppResult<AppSettingValue> {
    let key = normalize_setting_key(&key)?;
    let value = get_app_setting_inner(storage.inner(), key)?;
    Ok(AppSettingValue {
        key: key.to_string(),
        value,
    })
}

#[tauri::command]
pub fn set_app_setting(
    storage: State<'_, ManagedStorage>,
    request: SetAppSettingRequest,
) -> AppResult<AppSettingValue> {
    let key = normalize_setting_key(&request.key)?;
    set_app_setting_inner(storage.inner(), key, &request.value)?;
    Ok(AppSettingValue {
        key: key.to_string(),
        value: Some(request.value),
    })
}

#[tauri::command]
pub async fn get_startup_health(
    storage: State<'_, ManagedStorage>,
    agent: State<'_, AgentService>,
) -> AppResult<StartupHealthResponse> {
    let agent_status = agent.status().await.ok();
    get_startup_health_inner(storage.inner(), agent_status.as_ref())
}

#[tauri::command]
pub fn get_storage_usage(storage: State<'_, ManagedStorage>) -> AppResult<StorageUsageResponse> {
    get_storage_usage_inner(storage.inner())
}

#[tauri::command]
pub fn cleanup_storage(
    storage: State<'_, ManagedStorage>,
    request: CleanupStorageRequest,
) -> AppResult<CleanupStorageResponse> {
    cleanup_storage_inner(storage.inner(), request)
}

fn get_app_setting_inner(storage: &ManagedStorage, key: &str) -> AppResult<Option<String>> {
    let key = normalize_setting_key(key)?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    AppSettingsRepository::new(store.connection())
        .get(key)
        .map_err(storage_error)
}

fn set_app_setting_inner(storage: &ManagedStorage, key: &str, value: &str) -> AppResult<()> {
    let key = normalize_setting_key(key)?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    AppSettingsRepository::new(store.connection())
        .set(key, value)
        .map_err(storage_error)
}

fn get_startup_health_inner(
    storage: &ManagedStorage,
    agent_status: Option<&AgentServiceStatus>,
) -> AppResult<StartupHealthResponse> {
    let mut items = Vec::new();

    items.push(path_health_item(
        "storage",
        &storage.roots.app_data_dir,
        "settings.health.storageReady",
        "settings.health.storageMissing",
    ));
    items.push(path_health_item(
        "database",
        &storage.roots.database_path(),
        "settings.health.databaseReady",
        "settings.health.databaseMissing",
    ));

    let model_ready = {
        let store = storage.store.lock().map_err(|_| storage_lock_error())?;
        ModelConfigRepository::new(store.connection())
            .get("model-default")
            .map_err(storage_error)?
            .is_some()
    };
    items.push(if model_ready {
        health_item(
            "model",
            "ready",
            "settings.health.modelReady",
            None::<String>,
        )
    } else {
        health_item(
            "model",
            "warning",
            "settings.health.modelMissing",
            None::<String>,
        )
    });

    items.push(agent_health_item(agent_status));

    let status = if items.iter().any(|item| item.status == "blocked") {
        "blocked"
    } else if items.iter().any(|item| item.status == "warning") {
        "degraded"
    } else {
        "ready"
    };

    Ok(StartupHealthResponse {
        status: status.to_string(),
        items,
    })
}

fn agent_health_item(status: Option<&AgentServiceStatus>) -> StartupHealthItem {
    let Some(status) = status else {
        return health_item(
            "agent",
            "warning",
            "settings.health.agentUnavailable",
            None::<String>,
        );
    };

    let detail = Some(format!(
        "{} | {} | {}",
        status.health_url, status.python_executable, status.agent_dir
    ));

    if !Path::new(&status.agent_dir).is_dir() {
        return health_item(
            "agent",
            "blocked",
            "settings.health.agentDirectoryMissing",
            detail,
        );
    }

    if status.running {
        health_item("agent", "ready", "settings.health.agentReady", detail)
    } else {
        health_item("agent", "warning", "settings.health.agentStopped", detail)
    }
}

fn get_storage_usage_inner(storage: &ManagedStorage) -> AppResult<StorageUsageResponse> {
    let database_path = storage.roots.database_path();
    let database_bytes = file_size_if_present(&database_path).map_err(storage_error)?;
    let artifact_bytes = directory_size(&storage.roots.artifact_root).map_err(storage_error)?;
    let worktree_bytes = directory_size(&storage.roots.worktree_root).map_err(storage_error)?;
    let logs_bytes =
        named_directory_size(&storage.roots.artifact_root, "logs").map_err(storage_error)?;
    let screenshots_bytes =
        named_directory_size(&storage.roots.artifact_root, "screenshots").map_err(storage_error)?;
    let temporary_context_bytes =
        named_directory_size(&storage.roots.artifact_root, "context").map_err(storage_error)?;
    let temporary_bytes = logs_bytes + screenshots_bytes + temporary_context_bytes;
    let permanent_evidence_bytes = artifact_bytes.saturating_sub(temporary_bytes);
    let total_bytes = database_bytes + artifact_bytes + worktree_bytes;

    Ok(StorageUsageResponse {
        app_data_dir: storage.roots.app_data_dir.to_string_lossy().to_string(),
        database_path: database_path.to_string_lossy().to_string(),
        artifact_root: storage.roots.artifact_root.to_string_lossy().to_string(),
        worktree_root: storage.roots.worktree_root.to_string_lossy().to_string(),
        database_bytes,
        artifact_bytes,
        worktree_bytes,
        logs_bytes,
        screenshots_bytes,
        temporary_context_bytes,
        permanent_evidence_bytes,
        total_bytes,
    })
}

fn cleanup_storage_inner(
    storage: &ManagedStorage,
    request: CleanupStorageRequest,
) -> AppResult<CleanupStorageResponse> {
    let usage = get_storage_usage_inner(storage)?;
    let mut files = Vec::new();

    if request.logs {
        collect_files_in_named_dirs(&storage.roots.artifact_root, "logs", &mut files)
            .map_err(storage_error)?;
    }
    if request.screenshots {
        collect_files_in_named_dirs(&storage.roots.artifact_root, "screenshots", &mut files)
            .map_err(storage_error)?;
    }
    if request.temporary_context {
        collect_files_in_named_dirs(&storage.roots.artifact_root, "context", &mut files)
            .map_err(storage_error)?;
    }

    let mut deleted_files = 0;
    let mut deleted_bytes = 0;

    for file in files {
        let bytes = file_size_if_present(&file).map_err(storage_error)?;
        if !request.dry_run {
            fs::remove_file(&file)
                .map_err(StorageError::Io)
                .map_err(storage_error)?;
        }
        deleted_files += 1;
        deleted_bytes += bytes;
    }

    Ok(CleanupStorageResponse {
        dry_run: request.dry_run,
        scanned_files: deleted_files,
        deleted_files,
        deleted_bytes,
        protected_bytes: usage.permanent_evidence_bytes,
    })
}

fn path_health_item(
    key: &str,
    path: &std::path::Path,
    ready_message_key: &str,
    missing_message_key: &str,
) -> StartupHealthItem {
    if path.exists() {
        health_item(
            key,
            "ready",
            ready_message_key,
            Some(path.to_string_lossy().to_string()),
        )
    } else {
        health_item(
            key,
            "warning",
            missing_message_key,
            Some(path.to_string_lossy().to_string()),
        )
    }
}

fn health_item(
    key: &str,
    status: &str,
    message_key: &str,
    detail: Option<String>,
) -> StartupHealthItem {
    StartupHealthItem {
        key: key.to_string(),
        status: status.to_string(),
        message_key: message_key.to_string(),
        detail,
    }
}

fn normalize_setting_key(value: &str) -> AppResult<&str> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        });

    if valid {
        Ok(value)
    } else {
        Err(CommandError::new(
            "settings.invalidKey",
            "Setting key may only contain ASCII letters, numbers, '.', '-' and '_'.",
        ))
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
        StorageError::NotFound(message) => CommandError::new("storage.notFound", message),
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

fn default_true() -> bool {
    true
}

fn directory_size(path: &Path) -> Result<u64, StorageError> {
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

fn file_size_if_present(path: &Path) -> Result<u64, StorageError> {
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

fn named_directory_size(root: &Path, name: &str) -> Result<u64, StorageError> {
    let mut dirs = Vec::new();
    collect_named_dirs(root, name, &mut dirs)?;
    dirs.into_iter()
        .map(|path| directory_size(&path))
        .try_fold(0, |total, size| size.map(|size| total + size))
}

fn collect_files_in_named_dirs(
    root: &Path,
    name: &str,
    files: &mut Vec<PathBuf>,
) -> Result<(), StorageError> {
    let mut dirs = Vec::new();
    collect_named_dirs(root, name, &mut dirs)?;
    for dir in dirs {
        collect_files(&dir, files)?;
    }
    Ok(())
}

fn collect_named_dirs(
    root: &Path,
    name: &str,
    dirs: &mut Vec<PathBuf>,
) -> Result<(), StorageError> {
    if !root.exists() {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(());
    }

    if root.file_name().and_then(|value| value.to_str()) == Some(name) {
        dirs.push(root.to_path_buf());
        return Ok(());
    }

    for entry in fs::read_dir(root)? {
        collect_named_dirs(&entry?.path(), name, dirs)?;
    }
    Ok(())
}

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), StorageError> {
    if !root.exists() {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        files.push(root.to_path_buf());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(root)? {
        collect_files(&entry?.path(), files)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{ManagedStorage, SqliteStore, StorageRoots};
    use uuid::Uuid;

    fn test_storage() -> (ManagedStorage, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("codemax-app-command-{}", Uuid::new_v4()));
        let store = SqliteStore::open_in_memory().expect("open sqlite");
        store.migrate().expect("migrate sqlite");
        let storage = ManagedStorage {
            roots: StorageRoots::from_app_data_dir(&root),
            store: std::sync::Mutex::new(store),
        };
        (storage, root)
    }

    #[test]
    fn app_setting_round_trips_values() {
        let (storage, root) = test_storage();

        set_app_setting_inner(&storage, "locale", "en-US").expect("save setting");

        assert_eq!(
            get_app_setting_inner(&storage, "locale")
                .expect("read setting")
                .as_deref(),
            Some("en-US")
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn startup_health_reports_missing_model_as_degraded() {
        let (storage, root) = test_storage();

        let health = get_startup_health_inner(&storage, None).expect("health");

        assert_eq!(health.status, "degraded");
        assert!(health
            .items
            .iter()
            .any(|item| item.key == "model" && item.status == "warning"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn startup_health_reports_existing_agent_runtime_as_stopped_warning() {
        let (storage, root) = test_storage();
        let agent_dir = root.join("agent-runtime");
        std::fs::create_dir_all(&agent_dir).expect("create agent dir");
        let status = AgentServiceStatus {
            running: false,
            pid: None,
            host: "127.0.0.1".to_string(),
            port: 8765,
            health_url: "http://127.0.0.1:8765/health".to_string(),
            agent_dir: agent_dir.to_string_lossy().to_string(),
            python_executable: "python".to_string(),
            health: None,
        };

        let health = get_startup_health_inner(&storage, Some(&status)).expect("health");
        let agent = health
            .items
            .iter()
            .find(|item| item.key == "agent")
            .expect("agent health item");

        assert_eq!(agent.status, "warning");
        assert_eq!(agent.message_key, "settings.health.agentStopped");
        assert!(agent
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("127.0.0.1:8765") && detail.contains("python")));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn startup_health_reports_running_agent_runtime_as_ready() {
        let (storage, root) = test_storage();
        let agent_dir = root.join("agent-runtime");
        std::fs::create_dir_all(&agent_dir).expect("create agent dir");
        let status = AgentServiceStatus {
            running: true,
            pid: Some(1234),
            host: "127.0.0.1".to_string(),
            port: 8765,
            health_url: "http://127.0.0.1:8765/health".to_string(),
            agent_dir: agent_dir.to_string_lossy().to_string(),
            python_executable: "python".to_string(),
            health: None,
        };

        let health = get_startup_health_inner(&storage, Some(&status)).expect("health");
        let agent = health
            .items
            .iter()
            .find(|item| item.key == "agent")
            .expect("agent health item");

        assert_eq!(agent.status, "ready");
        assert_eq!(agent.message_key, "settings.health.agentReady");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn storage_usage_counts_temporary_and_permanent_categories() {
        let (storage, root) = test_storage();
        let task_root = storage.roots.artifact_root.join("task-1");
        std::fs::create_dir_all(task_root.join("logs")).expect("create logs");
        std::fs::create_dir_all(task_root.join("screenshots")).expect("create screenshots");
        std::fs::create_dir_all(task_root.join("context")).expect("create context");
        std::fs::write(task_root.join("logs/stdout.log"), b"1234").expect("write log");
        std::fs::write(task_root.join("screenshots/a.png"), b"123").expect("write screenshot");
        std::fs::write(task_root.join("context/chunk.txt"), b"12").expect("write context");
        std::fs::write(task_root.join("diff.patch"), b"12345").expect("write diff");

        let usage = get_storage_usage_inner(&storage).expect("usage");

        assert_eq!(usage.logs_bytes, 4);
        assert_eq!(usage.screenshots_bytes, 3);
        assert_eq!(usage.temporary_context_bytes, 2);
        assert_eq!(usage.permanent_evidence_bytes, 5);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_storage_dry_run_does_not_delete_files() {
        let (storage, root) = test_storage();
        let log = storage.roots.artifact_root.join("task-1/logs/stdout.log");
        std::fs::create_dir_all(log.parent().expect("log parent")).expect("create logs");
        std::fs::write(&log, b"1234").expect("write log");

        let result = cleanup_storage_inner(
            &storage,
            CleanupStorageRequest {
                logs: true,
                screenshots: false,
                temporary_context: false,
                dry_run: true,
            },
        )
        .expect("cleanup");

        assert_eq!(result.deleted_files, 1);
        assert_eq!(result.deleted_bytes, 4);
        assert!(log.exists());

        let _ = std::fs::remove_dir_all(root);
    }
}
