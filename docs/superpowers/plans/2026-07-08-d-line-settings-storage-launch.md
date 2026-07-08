# D Line Settings Storage Launch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first D-line readiness slice: model connection testing, transparent storage usage and cleanup, persisted UI preferences, startup health, official icon wiring, and smoke coverage.

**Architecture:** Add focused Tauri commands for settings, storage, model testing, and startup health, then expose them through typed IPC wrappers. Keep the existing React settings page and Codex-like styling while wiring real data into model, storage, appearance, language, and health panels.

**Tech Stack:** Tauri 2, Rust, rusqlite, React 18, TypeScript, Zustand, lucide-react, existing i18n JSON dictionaries.

---

## File Structure

- Modify `apps/desktop/src-tauri/src/commands/app.rs`: add app setting commands, storage usage, cleanup, startup health, and unit-testable helpers.
- Modify `apps/desktop/src-tauri/src/commands/models.rs`: add sanitized model connection validation command and tests.
- Modify `apps/desktop/src-tauri/src/commands/mod.rs`: expose any new command module if needed.
- Modify `apps/desktop/src-tauri/src/lib.rs`: register new Tauri commands.
- Modify `apps/desktop/src-tauri/tauri.conf.json`: reference official icon assets.
- Modify `apps/desktop/src/api/tauriClient.ts`: add typed IPC wrappers.
- Modify `apps/desktop/src/types/domain.ts`: add D-line response interfaces.
- Modify `apps/desktop/src/state/appStore.ts`: add async preference hydration and persisted setters.
- Modify `apps/desktop/src/app/App.tsx`: hydrate persisted preferences on startup.
- Modify `apps/desktop/src/features/settings/SettingsPage.tsx`: add connection test, storage usage/cleanup, startup health, and persisted controls.
- Modify `apps/desktop/src/i18n/locales/zh-CN.json`: add D-line Chinese strings.
- Modify `apps/desktop/src/i18n/locales/en-US.json`: add D-line English strings.
- Modify `apps/desktop/src/styles/global.css`: add minimal layout classes for D-line diagnostic rows.
- Modify `tests/frontend/verify-s6-ui.mjs`: assert D-line UI and IPC markers.

## Task 1: Backend App Settings And Startup Health

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/app.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing Rust tests for app setting persistence and degraded startup health**

Add tests inside `commands::app`:

```rust
#[test]
fn app_setting_round_trips_values() {
    let (storage, _root) = test_storage();

    set_app_setting_inner(&storage, "locale", "en-US").expect("save setting");

    assert_eq!(
        get_app_setting_inner(&storage, "locale").expect("read setting").as_deref(),
        Some("en-US")
    );
}

#[test]
fn startup_health_reports_missing_model_as_degraded() {
    let (storage, _root) = test_storage();

    let health = get_startup_health_inner(&storage, false).expect("health");

    assert_eq!(health.status, "degraded");
    assert!(health.items.iter().any(|item| item.key == "model" && item.status == "warning"));
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml app_setting_round_trips_values startup_health_reports_missing_model_as_degraded`

Expected: fail because `set_app_setting_inner`, `get_app_setting_inner`, and `get_startup_health_inner` do not exist.

- [ ] **Step 3: Implement minimal app settings and health helpers**

Add serializable structs and helpers:

```rust
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
```

Use `AppSettingsRepository` for storage, `ModelConfigRepository` for model readiness, filesystem checks for roots, and an `agent_available` boolean parameter for testability.

- [ ] **Step 4: Add Tauri commands and register them**

Expose:

```rust
#[tauri::command]
pub fn get_app_setting(storage: State<'_, ManagedStorage>, key: String) -> AppResult<AppSettingValue>

#[tauri::command]
pub fn set_app_setting(storage: State<'_, ManagedStorage>, request: SetAppSettingRequest) -> AppResult<AppSettingValue>

#[tauri::command]
pub fn get_startup_health(storage: State<'_, ManagedStorage>) -> AppResult<StartupHealthResponse>
```

Register these in `tauri::generate_handler!`.

- [ ] **Step 5: Run tests and verify GREEN**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml app_setting_round_trips_values startup_health_reports_missing_model_as_degraded`

Expected: both tests pass.

## Task 2: Backend Storage Usage And Protected Cleanup

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/app.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing tests for aggregate usage and dry-run cleanup**

Add tests:

```rust
#[test]
fn storage_usage_counts_temporary_and_permanent_categories() {
    let (storage, root) = test_storage();
    std::fs::create_dir_all(storage.roots.artifact_root.join("task-1/logs")).unwrap();
    std::fs::create_dir_all(storage.roots.artifact_root.join("task-1/screenshots")).unwrap();
    std::fs::create_dir_all(storage.roots.artifact_root.join("task-1/context")).unwrap();
    std::fs::write(storage.roots.artifact_root.join("task-1/logs/stdout.log"), b"1234").unwrap();
    std::fs::write(storage.roots.artifact_root.join("task-1/screenshots/a.png"), b"123").unwrap();
    std::fs::write(storage.roots.artifact_root.join("task-1/context/chunk.txt"), b"12").unwrap();
    std::fs::write(storage.roots.artifact_root.join("task-1/diff.patch"), b"12345").unwrap();

    let usage = get_storage_usage_inner(&storage).expect("usage");

    assert_eq!(usage.logs_bytes, 4);
    assert_eq!(usage.screenshots_bytes, 3);
    assert_eq!(usage.temporary_context_bytes, 2);
    assert_eq!(usage.permanent_evidence_bytes, 5);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cleanup_storage_dry_run_does_not_delete_files() {
    let (storage, root) = test_storage();
    let log = storage.roots.artifact_root.join("task-1/logs/stdout.log");
    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    std::fs::write(&log, b"1234").unwrap();

    let result = cleanup_storage_inner(&storage, CleanupStorageRequest {
        logs: true,
        screenshots: false,
        temporary_context: false,
        dry_run: true,
    }).expect("cleanup");

    assert_eq!(result.deleted_files, 1);
    assert_eq!(result.deleted_bytes, 4);
    assert!(log.exists());

    std::fs::remove_dir_all(root).unwrap();
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml storage_usage_counts_temporary_and_permanent_categories cleanup_storage_dry_run_does_not_delete_files`

Expected: fail because storage usage and cleanup helpers do not exist.

- [ ] **Step 3: Implement storage usage and cleanup**

Add response structs:

```rust
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
```

Traverse only `logs`, `screenshots`, and `context` directories for cleanup. Count `diff.patch`, `proof-pack`, approvals, reports, and artifact roots outside those temporary folders as protected evidence.

- [ ] **Step 4: Add Tauri commands and register them**

Expose:

```rust
#[tauri::command]
pub fn get_storage_usage(storage: State<'_, ManagedStorage>) -> AppResult<StorageUsageResponse>

#[tauri::command]
pub fn cleanup_storage(storage: State<'_, ManagedStorage>, request: CleanupStorageRequest) -> AppResult<CleanupStorageResponse>
```

- [ ] **Step 5: Run tests and verify GREEN**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml storage_usage_counts_temporary_and_permanent_categories cleanup_storage_dry_run_does_not_delete_files`

Expected: both tests pass.

## Task 3: Backend Model Connection Validation

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/models.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing tests for sanitized validation**

Add tests:

```rust
#[test]
fn model_connection_reports_missing_api_key_without_leaking_secret() {
    let (storage, root) = model_storage();
    save_model_config_inner(&storage, SaveModelConfigRequest {
        id: None,
        provider: "openai-compatible".to_string(),
        base_url: "https://api.example.test/v1".to_string(),
        model_name: "codemax-test".to_string(),
        api_key: None,
        clear_api_key: false,
    }).expect("save config");

    let result = test_model_connection_inner(&storage, None).expect("test model");

    assert_eq!(result.status, "warning");
    assert_eq!(result.message_key, "settings.models.connectionMissingApiKey");
    assert!(!format!("{result:?}").contains("sk-"));

    std::fs::remove_dir_all(root).expect("clean temp model storage");
}
```

- [ ] **Step 2: Run test and verify RED**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml model_connection_reports_missing_api_key_without_leaking_secret`

Expected: fail because `test_model_connection_inner` does not exist.

- [ ] **Step 3: Implement deterministic validation command**

Add:

```rust
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
```

For this slice, validate saved config, parse the base URL host, check API key presence, and label success as `settings.models.connectionConfigReady`. Do not perform live inference yet.

- [ ] **Step 4: Add Tauri command and register it**

Expose:

```rust
#[tauri::command]
pub fn test_model_connection(storage: State<'_, ManagedStorage>, id: Option<String>) -> AppResult<ModelConnectionTestResult>
```

- [ ] **Step 5: Run test and verify GREEN**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml model_connection_reports_missing_api_key_without_leaking_secret`

Expected: test passes.

## Task 4: Typed IPC And Persisted App Store

**Files:**
- Modify: `apps/desktop/src/types/domain.ts`
- Modify: `apps/desktop/src/api/tauriClient.ts`
- Modify: `apps/desktop/src/state/appStore.ts`
- Modify: `apps/desktop/src/app/App.tsx`

- [ ] **Step 1: Add frontend type definitions**

Add interfaces for `ModelConnectionTestResult`, `StorageUsageResponse`, `CleanupStorageRequest`, `CleanupStorageResponse`, `StartupHealthResponse`, and `AppSettingValue`.

- [ ] **Step 2: Add typed IPC wrappers**

Add functions:

```ts
export function testModelConnection(id = 'model-default') {
  return invokeCommand<ModelConnectionTestResult>('test_model_connection', { id });
}

export function getStorageUsage() {
  return invokeCommand<StorageUsageResponse>('get_storage_usage');
}

export function cleanupStorage(request: CleanupStorageRequest) {
  return invokeCommand<CleanupStorageResponse>('cleanup_storage', { request });
}

export function getStartupHealth() {
  return invokeCommand<StartupHealthResponse>('get_startup_health');
}

export function getAppSetting(key: string) {
  return invokeCommand<AppSettingValue>('get_app_setting', { key });
}

export function setAppSetting(key: string, value: string) {
  return invokeCommand<AppSettingValue>('set_app_setting', { request: { key, value } });
}
```

- [ ] **Step 3: Update store actions to persist settings**

Add `hydratePreferences` and make setters call `setAppSetting` after updating local state. Use fire-and-forget persistence with console-free error swallowing to avoid leaking values.

- [ ] **Step 4: Hydrate preferences in `App.tsx`**

Call `hydratePreferences()` once in a `useEffect`.

- [ ] **Step 5: Run frontend type check**

Run: `npm run build:desktop`

Expected: TypeScript passes or reports only implementation mistakes to fix before moving on.

## Task 5: Settings UI, i18n, And Styling

**Files:**
- Modify: `apps/desktop/src/features/settings/SettingsPage.tsx`
- Modify: `apps/desktop/src/i18n/locales/zh-CN.json`
- Modify: `apps/desktop/src/i18n/locales/en-US.json`
- Modify: `apps/desktop/src/styles/global.css`

- [ ] **Step 1: Wire model connection test UI**

Import `testModelConnection`, add a button beside save, and render status with localized `messageKey` from the backend.

- [ ] **Step 2: Wire startup health panel**

Load `getStartupHealth()` in `GeneralSettings`, render overall status and subsystem rows.

- [ ] **Step 3: Wire storage usage and cleanup UI**

Load `getStorageUsage()` in `StorageSettings`, format bytes locally, add refresh, dry-run preview, and confirmed cleanup actions for logs, screenshots, and temporary context.

- [ ] **Step 4: Add i18n keys**

Add Chinese and English keys for model test, health statuses, storage usage rows, cleanup preview/action labels, and permanent evidence explanation.

- [ ] **Step 5: Add scoped CSS**

Add classes for `.settings-diagnostic-list`, `.settings-status-pill`, `.settings-usage-grid`, `.settings-cleanup-actions`, and `.settings-byte-value`.

- [ ] **Step 6: Run frontend build**

Run: `npm run build:desktop`

Expected: build passes.

## Task 6: Icon Wiring And Smoke Coverage

**Files:**
- Modify: `apps/desktop/src-tauri/tauri.conf.json`
- Modify: `tests/frontend/verify-s6-ui.mjs`

- [ ] **Step 1: Configure Tauri bundle icons**

Add:

```json
"icon": [
  "icons/icon.ico"
]
```

under `bundle`, using the existing generated icon asset derived from the official source.

- [ ] **Step 2: Extend frontend smoke markers**

Assert markers for:

```js
'testModelConnection',
'getStorageUsage',
'cleanupStorage',
'getStartupHealth',
'hydratePreferences',
'settings.models.testConnection',
'settings.storage.usageTitle',
'settings.health.title'
```

- [ ] **Step 3: Run full verification**

Run:

```powershell
npm run check
npm run build:desktop
npm run check:tauri
```

Expected: all commands exit 0. If not, fix the failing task or report the exact blocker.

## Self-Review

- Spec coverage: model connection testing is covered by Task 3 and Task 5; storage paths, usage, and cleanup by Task 2 and Task 5; persisted language/theme by Task 4; startup health by Task 1 and Task 5; icon wiring by Task 6; smoke checks by Task 6.
- Placeholder scan: no TBD/TODO/later placeholders are used.
- Type consistency: backend command names match frontend IPC wrapper names and planned smoke markers.
