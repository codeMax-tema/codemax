# CodeMax D Line Settings, Storage, And Launch Readiness Design

## Scope

This design covers the first D-line delivery slice after the A task chain is complete. It focuses on real user-facing desktop readiness rather than demo screens:

- Model configuration and connection testing.
- Transparent storage paths, usage, and cleanup policy.
- Persisted UI language and theme settings.
- Official CodeMax icon wiring.
- Startup health checks for user-actionable launch problems.
- Smoke checks that can run against the desktop app and backend commands.

Packaging, full installer validation, release notes, and final all-line E2E remain follow-up D-line work after B and C expose their final privacy, contract, proof, and gate interfaces.

## Product Principles

The D-line experience must make the local app feel installable, configurable, recoverable, and honest. Users should always know where data is stored, which data can be cleaned, which evidence is permanent, and why a startup or model check failed.

The default UI remains minimal. The implementation keeps the existing Codex-like settings layout and adds real controls instead of introducing a new visual direction. Additional UI styles remain available through the theme setting.

All new visible text must go through the existing i18n dictionaries for `zh-CN` and `en-US`.

## Existing Context

The desktop app is a Tauri + React application:

- Frontend settings live in `apps/desktop/src/features/settings/SettingsPage.tsx`.
- Frontend IPC wrappers live in `apps/desktop/src/api/tauriClient.ts`.
- Global UI state lives in `apps/desktop/src/state/appStore.ts`.
- i18n dictionaries live in `apps/desktop/src/i18n/locales`.
- Tauri commands already provide `get_storage_roots`, `get_model_config`, and `save_model_config`.
- Storage roots are runtime-configurable through `CODEMAX_APP_DATA_DIR`, `CODEMAX_ARTIFACT_ROOT`, `CODEMAX_WORKTREE_ROOT`, and `CODEMAX_DATABASE_URL`.
- The required brand image exists at `D:\codemax\ico\CodeMax.png`.

## Approach

Use the existing settings page as the main D-line product surface. Add real backend commands where the app needs trustworthy data, and keep expensive operations explicit so the page stays responsive.

The first implementation slice should avoid broad restyling. It should add focused sections:

- A model connection test button in the model settings pane.
- A storage usage and cleanup section in the storage pane.
- A startup health section that summarizes launch blockers.
- Persisted theme and locale settings behind the existing appearance and language controls.

## Backend Commands

### `test_model_connection`

Input:

- Optional config id, defaulting to `model-default`.

Behavior:

- Loads the saved model config and API key through the secure secret store.
- Validates that provider, base URL, model name, and API key are usable.
- Performs a minimal OpenAI-compatible request when enough data is present.
- Returns a sanitized result with status, latency, provider, model name, and a user-facing failure reason.
- Never returns or logs the plaintext API key.

If live provider calls are not yet stable, the command may start with a deterministic configuration check plus an HTTP reachability check to the configured base URL. It must label the result honestly as configuration/reachability, not as a full model inference test.

### `get_storage_usage`

Output:

- App data directory.
- Database path and size.
- Artifact root and size.
- Worktree root and size.
- Logs size.
- Screenshots size.
- Temporary context size.
- Permanent evidence size.
- Total size.

Behavior:

- Measures directory sizes recursively.
- Treats missing directories as zero bytes.
- Keeps the command read-only.
- Avoids loading file contents into memory.

### `cleanup_storage`

Input:

- Cleanup targets: logs, screenshots, temporary context.
- Optional dry-run flag.

Behavior:

- Deletes only temporary categories.
- Refuses to delete final diffs, proof packs, approvals, merge records, and other permanent evidence.
- Returns deleted file count and reclaimed bytes.
- Supports dry-run for previewing cleanup impact.

### `get_startup_health`

Output:

- Storage status.
- Database status.
- Model configuration status.
- Agent runtime status.
- Icon/config status where practical.
- A list of actionable warnings/errors.

Behavior:

- Uses existing health and storage primitives where possible.
- Does not fail the whole command for one bad subsystem; returns a degraded health report.
- Messages are stable status codes plus frontend-localized labels.

### `get_app_setting` / `set_app_setting`

Behavior:

- Stores UI settings such as locale, theme, compact mode, and high contrast mode in the existing `app_settings` table.
- Returns string values with lightweight frontend parsing.
- Keeps defaults unchanged when no persisted setting exists.

## Frontend Changes

### Model Settings

Add a test connection action near the save button:

- Disabled while loading or saving.
- Shows pending, success, warning, or error state.
- Displays sanitized details only: provider, model, base URL host, latency, and failure message.
- Keeps API key preview masked.

### Storage Settings

Expand the storage pane with:

- Real path cards for app data, database, artifact root, and worktree root.
- Usage rows for database, worktrees, logs, screenshots, temporary context, permanent evidence, and total.
- A refresh button.
- A cleanup preview button.
- A cleanup action that requires explicit user confirmation in the UI.

Permanent evidence should be visually separated from cleanup targets so users understand what will remain.

### Appearance And Language

Persist:

- Theme: `minimal`, `dark`, `highContrast`.
- Compact mode.
- High contrast mode.
- Locale: `zh-CN` or `en-US`.

Apply stored values on startup before the user changes settings during the session.

### Startup Health

Add a compact startup health panel in settings, preferably under General or Storage:

- Overall status: ready, degraded, or blocked.
- Subsystem rows with concise labels.
- User-actionable next steps.

The panel should not read as a marketing card. It is a diagnostic surface for repeated use.

### Icon Wiring

Use `D:\codemax\ico\CodeMax.png` as the source for Tauri icons. Generate or update the Tauri icon assets needed by Windows builds, then reference them through `tauri.conf.json` bundle icon settings.

## Data And Error Handling

- Backend command errors should use stable error codes.
- Frontend display strings should be localized through i18n.
- Sensitive values must be masked before crossing the IPC boundary.
- Storage measurement should handle missing directories and permission errors gracefully.
- Cleanup must default to dry-run before destructive deletion.
- UI state must avoid large in-memory file lists; only aggregate counts and sizes should be kept.

## Testing Strategy

Follow test-first implementation for behavior changes:

- Rust unit tests for storage usage measurement, cleanup protection, app setting persistence, and model connection validation paths.
- Frontend build/type checks for IPC wrappers and settings page changes.
- Existing architecture and frontend checks remain required.
- Add or extend smoke scripts to verify D-line strings and UI entry points exist without relying on mock-only data.

Verification commands for this slice:

- `npm run check`
- `npm run build:desktop`
- `npm run check:tauri`

If a command cannot run in the local environment, report the exact blocker and the last successful narrower verification.

## Acceptance Criteria

- Users can save a model config and run a sanitized connection test.
- Users can see real storage paths and aggregate storage usage.
- Users can preview and run cleanup for temporary data without deleting permanent evidence.
- Locale and theme settings persist across app reloads.
- Startup health exposes model, storage, database, and Agent readiness problems.
- The app icon uses the official CodeMax asset source.
- New UI copy exists in Chinese and English.
- No API key plaintext appears in UI, SQLite, logs, or command output.
