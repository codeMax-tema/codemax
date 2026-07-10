# A/B/C/D Completion Worktree Baseline

## Snapshot

- Captured: 2026-07-10
- Branch: `main`
- Baseline commit: `9575b2d`
- Design commit: `e55aa96`
- Upstream baseline before planning: `5230c1e`

The worktree already contains substantial uncommitted A/B/D/UI integration work. These files are treated as user-owned input and must not be reverted or overwritten wholesale.

## Existing Modified Areas

### Python Agent

- `agent/app/memory/service.py`
- `agent/tests/test_memory_preference_guard.py` (untracked)

### Rust/Tauri

- Agent, command execution, merge, privacy, repository, task and S11 command modules.
- Git, storage, Tauri command registration and IPC integration.
- `apps/desktop/src-tauri/src/commands/skills.rs` (untracked).
- `database/migrations/0008_agent_telemetry.sql` (untracked but required by `storage/mod.rs`).

### React/Desktop

- App shell, repository, settings, new-task and task-overview pages.
- API client, app store, domain types, global CSS and bilingual locale files.
- Home, search and skills feature directories (untracked).

### Contracts And Checks

- `contracts/ipc.schema.json`
- `tests/frontend/verify-s6-ui.mjs`

## Verified Baseline

- `npm run check:architecture`: passed.
- `npm run check:frontend`: passed.
- `npm run check:release`: passed.
- `npm run build:desktop`: passed.
- Frontend production build emitted one main JavaScript chunk of about 4 MB; route and Monaco lazy loading remain required.

## Known Baseline Failures

- `npm run check:tauri` fails because `command_execution_error` does not cover `CommandExecutionError::InvalidPurpose`.
- `py -m pytest agent/tests -q` cannot run until the Agent development dependencies are installed in the selected Python environment.
- The current D-line source smoke is failed/pending because Tauri check fails and installed-app A/B/C smoke is not implemented.

## Commit Boundaries

1. Compile-baseline changes must include the `InvalidPurpose` mapping, its test, migration 0008 and all Rust references required to compile.
2. Contract changes must keep Rust registration, TypeScript wrappers and JSON Schema in the same commit.
3. UI commits must include both locale files and must not introduce mock-only success states.
4. Database migrations must be committed with the code that includes them.
5. Visual brainstorming files under `.superpowers/brainstorm/` are local temporary artifacts and are ignored.
