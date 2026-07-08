use serde::{Deserialize, Serialize};
use std::time::Instant;
use tauri::State;

use crate::{
    core::error::{AppResult, CommandError},
    secrets::{SecretStore, SecretStoreError},
    storage::{
        ManagedStorage, ModelConfigRecord, ModelConfigRepository, NewModelConfig, StorageError,
    },
};

const DEFAULT_MODEL_CONFIG_ID: &str = "model-default";
const MASKED_API_KEY: &str = "********";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveModelConfigRequest {
    pub id: Option<String>,
    pub provider: String,
    pub base_url: String,
    pub model_name: String,
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_api_key: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfigView {
    pub id: String,
    pub provider: String,
    pub base_url: String,
    pub model_name: String,
    pub api_key_configured: bool,
    pub api_key_masked: Option<String>,
    pub secret_storage: Option<String>,
    pub secret_location: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConnectionTestResult {
    pub status: String,
    pub provider: String,
    pub model_name: String,
    pub base_url_host: Option<String>,
    pub latency_ms: u128,
    pub message_key: String,
    pub detail: Option<String>,
}

#[tauri::command]
pub fn get_model_config(
    storage: State<'_, ManagedStorage>,
    id: Option<String>,
) -> AppResult<Option<ModelConfigView>> {
    get_model_config_inner(storage.inner(), id.as_deref())
}

#[tauri::command]
pub fn save_model_config(
    storage: State<'_, ManagedStorage>,
    request: SaveModelConfigRequest,
) -> AppResult<ModelConfigView> {
    save_model_config_inner(storage.inner(), request)
}

#[tauri::command]
pub fn test_model_connection(
    storage: State<'_, ManagedStorage>,
    id: Option<String>,
) -> AppResult<ModelConnectionTestResult> {
    test_model_connection_inner(storage.inner(), id.as_deref())
}

pub(crate) fn load_model_api_key_values(storage: &ManagedStorage) -> Vec<String> {
    let secret_store = SecretStore::new(&storage.roots.app_data_dir);
    let configs = {
        let Ok(store) = storage.store.lock() else {
            return Vec::new();
        };
        ModelConfigRepository::new(store.connection())
            .list()
            .unwrap_or_default()
    };

    configs
        .into_iter()
        .filter_map(|config| config.api_key_secret_ref)
        .filter_map(|secret_ref| secret_store.read_secret_ref(&secret_ref).ok().flatten())
        .filter(|secret| secret.len() >= 4)
        .collect()
}

fn get_model_config_inner(
    storage: &ManagedStorage,
    id: Option<&str>,
) -> AppResult<Option<ModelConfigView>> {
    let id = normalize_config_id(id.unwrap_or(DEFAULT_MODEL_CONFIG_ID))?;
    let secret_store = SecretStore::new(&storage.roots.app_data_dir);
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let config = ModelConfigRepository::new(store.connection())
        .get(id)
        .map_err(storage_error)?;

    config
        .map(|config| model_config_view(config, &secret_store))
        .transpose()
}

fn save_model_config_inner(
    storage: &ManagedStorage,
    request: SaveModelConfigRequest,
) -> AppResult<ModelConfigView> {
    let id = normalize_config_id(request.id.as_deref().unwrap_or(DEFAULT_MODEL_CONFIG_ID))?;
    let provider = required_field("provider", &request.provider)?;
    let model_name = required_field("modelName", &request.model_name)?;
    let base_url = request.base_url.trim().to_string();
    let secret_store = SecretStore::new(&storage.roots.app_data_dir);

    let existing = {
        let store = storage.store.lock().map_err(|_| storage_lock_error())?;
        ModelConfigRepository::new(store.connection())
            .get(id)
            .map_err(storage_error)?
    };
    let mut secret_ref = existing
        .as_ref()
        .and_then(|config| config.api_key_secret_ref.clone());

    if request.clear_api_key {
        if let Some(existing_ref) = secret_ref.as_deref() {
            secret_store
                .remove_secret_ref(existing_ref)
                .map_err(secret_error)?;
        }
        secret_ref = None;
    } else if let Some(api_key) = request.api_key.as_deref().map(str::trim) {
        if !api_key.is_empty() {
            secret_ref = Some(
                secret_store
                    .put_model_api_key(id, api_key)
                    .map_err(secret_error)?,
            );
        }
    }

    let saved = {
        let store = storage.store.lock().map_err(|_| storage_lock_error())?;
        ModelConfigRepository::new(store.connection())
            .save(NewModelConfig {
                id,
                provider: &provider,
                base_url: &base_url,
                model_name: &model_name,
                api_key_secret_ref: secret_ref.as_deref(),
            })
            .map_err(storage_error)?
    };

    model_config_view(saved, &secret_store)
}

fn test_model_connection_inner(
    storage: &ManagedStorage,
    id: Option<&str>,
) -> AppResult<ModelConnectionTestResult> {
    let started = Instant::now();
    let id = normalize_config_id(id.unwrap_or(DEFAULT_MODEL_CONFIG_ID))?;
    let secret_store = SecretStore::new(&storage.roots.app_data_dir);
    let config = {
        let store = storage.store.lock().map_err(|_| storage_lock_error())?;
        ModelConfigRepository::new(store.connection())
            .get(id)
            .map_err(storage_error)?
    };

    let Some(config) = config else {
        return Ok(ModelConnectionTestResult {
            status: "warning".to_string(),
            provider: String::new(),
            model_name: String::new(),
            base_url_host: None,
            latency_ms: started.elapsed().as_millis(),
            message_key: "settings.models.connectionMissingConfig".to_string(),
            detail: None,
        });
    };

    let base_url_host = parse_base_url_host(&config.base_url);
    if config.base_url.trim().is_empty() {
        return Ok(ModelConnectionTestResult {
            status: "warning".to_string(),
            provider: config.provider,
            model_name: config.model_name,
            base_url_host,
            latency_ms: started.elapsed().as_millis(),
            message_key: "settings.models.connectionMissingBaseUrl".to_string(),
            detail: None,
        });
    }

    let api_key_configured = config
        .api_key_secret_ref
        .as_deref()
        .is_some_and(|secret_ref| secret_store.secret_exists(secret_ref));
    if !api_key_configured {
        return Ok(ModelConnectionTestResult {
            status: "warning".to_string(),
            provider: config.provider,
            model_name: config.model_name,
            base_url_host,
            latency_ms: started.elapsed().as_millis(),
            message_key: "settings.models.connectionMissingApiKey".to_string(),
            detail: None,
        });
    }

    Ok(ModelConnectionTestResult {
        status: "ready".to_string(),
        provider: config.provider,
        model_name: config.model_name,
        base_url_host,
        latency_ms: started.elapsed().as_millis(),
        message_key: "settings.models.connectionConfigReady".to_string(),
        detail: None,
    })
}

fn model_config_view(
    config: ModelConfigRecord,
    secret_store: &SecretStore,
) -> AppResult<ModelConfigView> {
    let secret_location = config
        .api_key_secret_ref
        .as_deref()
        .and_then(|secret_ref| secret_store.secret_location(secret_ref).ok());
    let api_key_configured = config
        .api_key_secret_ref
        .as_deref()
        .is_some_and(|secret_ref| secret_store.secret_exists(secret_ref));

    Ok(ModelConfigView {
        id: config.id,
        provider: config.provider,
        base_url: config.base_url,
        model_name: config.model_name,
        api_key_configured,
        api_key_masked: api_key_configured.then(|| MASKED_API_KEY.to_string()),
        secret_storage: api_key_configured.then(|| secret_store.storage_kind().to_string()),
        secret_location: secret_location.map(|path| path.to_string_lossy().to_string()),
        created_at: config.created_at,
        updated_at: config.updated_at,
    })
}

fn parse_base_url_host(value: &str) -> Option<String> {
    let value = value.trim();
    let without_scheme = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .unwrap_or(value);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .split('@')
        .next_back()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .trim();

    (!host.is_empty()).then(|| host.to_string())
}

fn normalize_config_id(value: &str) -> AppResult<&str> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));

    if valid {
        Ok(value)
    } else {
        Err(CommandError::new(
            "model.invalidConfigId",
            "Model config id may only contain ASCII letters, numbers, '-' and '_'.",
        ))
    }
}

fn required_field(field: &str, value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CommandError::new(
            "model.requiredFieldMissing",
            format!("{field} is required."),
        ));
    }

    Ok(value.to_string())
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

fn secret_error(error: SecretStoreError) -> CommandError {
    CommandError::new(
        "model.secretStorageFailed",
        format!("Unable to store model API key securely: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{SqliteStore, StorageRoots};
    use uuid::Uuid;

    fn model_storage() -> (ManagedStorage, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("codemax-models-{}", Uuid::new_v4()));
        let store = SqliteStore::open_in_memory().expect("open sqlite");
        store.migrate().expect("migrate sqlite");
        let storage = ManagedStorage {
            roots: StorageRoots::from_app_data_dir(&root),
            store: std::sync::Mutex::new(store),
        };
        (storage, root)
    }

    #[cfg(windows)]
    #[test]
    fn save_model_config_keeps_api_key_out_of_sqlite() {
        let (storage, root) = model_storage();
        let secret = "sk-test-secret-value";

        let view = save_model_config_inner(
            &storage,
            SaveModelConfigRequest {
                id: Some("model-default".to_string()),
                provider: "openai-compatible".to_string(),
                base_url: "https://api.example.test/v1".to_string(),
                model_name: "codemax-test".to_string(),
                api_key: Some(secret.to_string()),
                clear_api_key: false,
            },
        )
        .expect("save model config");

        assert!(view.api_key_configured);
        assert_eq!(view.api_key_masked.as_deref(), Some(MASKED_API_KEY));

        let store = storage.store.lock().expect("storage lock");
        let config = ModelConfigRepository::new(store.connection())
            .get("model-default")
            .expect("get model config")
            .expect("model config exists");
        assert_ne!(config.api_key_secret_ref.as_deref(), Some(secret));
        assert!(config
            .api_key_secret_ref
            .as_deref()
            .is_some_and(|secret_ref| secret_ref.starts_with("secret://")));

        std::fs::remove_dir_all(root).expect("clean temp model storage");
    }

    #[cfg(windows)]
    #[test]
    fn load_model_api_key_values_reads_saved_secret_for_redaction() {
        let (storage, root) = model_storage();
        let secret = "sk-test-redact-value";

        save_model_config_inner(
            &storage,
            SaveModelConfigRequest {
                id: None,
                provider: "openai-compatible".to_string(),
                base_url: String::new(),
                model_name: "codemax-test".to_string(),
                api_key: Some(secret.to_string()),
                clear_api_key: false,
            },
        )
        .expect("save model config");

        assert_eq!(
            load_model_api_key_values(&storage),
            vec![secret.to_string()]
        );

        std::fs::remove_dir_all(root).expect("clean temp model storage");
    }

    #[test]
    fn model_connection_reports_missing_api_key_without_leaking_secret() {
        let (storage, root) = model_storage();
        save_model_config_inner(
            &storage,
            SaveModelConfigRequest {
                id: None,
                provider: "openai-compatible".to_string(),
                base_url: "https://api.example.test/v1".to_string(),
                model_name: "codemax-test".to_string(),
                api_key: None,
                clear_api_key: false,
            },
        )
        .expect("save config");

        let result = test_model_connection_inner(&storage, None).expect("test model");

        assert_eq!(result.status, "warning");
        assert_eq!(
            result.message_key,
            "settings.models.connectionMissingApiKey"
        );
        assert!(!format!("{result:?}").contains("sk-"));

        let _ = std::fs::remove_dir_all(root);
    }
}
