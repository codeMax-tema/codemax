# S10 One-Click Merge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a user-confirmed local merge flow that shows final evidence, protects the target branch, records merge results, and never hides conflicts.

**Architecture:** Extend the existing S8 Diff/Delivery review flow instead of creating a separate merge page. Rust owns Git precheck, source commit, local merge, conflict abort, task status update, and permanent merge artifacts; React owns the compact precheck panel, confirmation dialog, merge status, and conflict list.

**Tech Stack:** Tauri v2, Rust, rusqlite, React 18, TypeScript, existing i18n JSON, existing shadcn-style dialog/button primitives.

---

### Task 1: Git Merge Core

**Files:**
- Modify: `apps/desktop/src-tauri/src/git/mod.rs`

- [x] **Step 1: Write failing Rust tests**

Add tests for successful dirty worktree commit plus target merge, and for conflict detection with merge abort.

- [x] **Step 2: Run targeted tests and verify they fail**

Run: `cargo test git::tests::merge_task_branch -- --nocapture`
Expected: FAIL because merge service types/functions do not exist.

- [x] **Step 3: Implement minimal Git merge helpers**

Add structs for precheck/result/conflict data and functions that:
- reject dirty target worktrees,
- commit dirty task worktree changes with the user-confirmed message,
- merge source branch into the current target branch,
- collect conflict files and abort conflicted merges.

- [x] **Step 4: Run targeted tests and verify they pass**

Run: `cargo test git::tests::merge_task_branch -- --nocapture`
Expected: PASS.

### Task 2: Tauri Merge Commands

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/merge.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [x] **Step 1: Write failing command tests**

Add tests for explicit confirmation, validation gate behavior, and conflict-as-response mapping.

- [x] **Step 2: Run targeted command tests and verify they fail**

Run: `cargo test commands::merge::tests -- --nocapture`
Expected: FAIL because merge command module does not exist.

- [x] **Step 3: Implement precheck and merge commands**

Add `prepare_task_merge` and `merge_task` commands. Precheck loads task, latest diff/delivery evidence, target branch state, and validation status. Merge requires `confirmed: true`, passed validation, clean target, and a non-empty commit message.

- [x] **Step 4: Persist merge artifacts and task status**

On success, update task status to `merged`, write a permanent merge record file under task artifacts, record it in `artifacts` and `artifact_files`, and return the commit SHA. On conflict, return `status: conflicted` with conflict files and do not update task status.

### Task 3: Frontend Merge Surface

**Files:**
- Modify: `apps/desktop/src/types/domain.ts`
- Modify: `apps/desktop/src/api/tauriClient.ts`
- Modify: `apps/desktop/src/features/tasks/TaskOverviewPage.tsx`
- Modify: `apps/desktop/src/i18n/locales/zh-CN.json`
- Modify: `apps/desktop/src/i18n/locales/en-US.json`
- Modify: `apps/desktop/src/styles/global.css`

- [x] **Step 1: Add TypeScript contracts and API wrappers**

Expose `prepareTaskMerge` and `mergeTask` with typed precheck/result payloads.

- [x] **Step 2: Add compact merge panel**

Show target branch, source branch, validation status, target cleanliness, final Diff stats, and suggested commit message from the delivery artifact.

- [x] **Step 3: Add confirmation dialog**

Require a second click before merging and allow the user to adjust the commit message. Disable the default merge path when validation is failed/not run or target branch is dirty.

- [x] **Step 4: Add conflict and success states**

Show conflict files inline when the backend reports `conflicted`; show merge commit and merge record path on success.

### Task 4: Verification

**Files:**
- No production files unless verification reveals targeted fixes.

- [x] **Step 1: Run targeted Rust tests**

Run: `cargo test git::tests::merge_task_branch commands::merge::tests -- --nocapture`
Expected: all targeted S10 tests pass.

- [x] **Step 2: Run full Rust tests**

Run: `cargo test`
Expected: all Rust tests pass, including the stabilized non-git temp directory test.

- [x] **Step 3: Run frontend build**

Run: `npm run build` in `apps/desktop`
Expected: may remain blocked by pre-existing Monaco dependency resolution if dependencies are not installed; report exact result.

- [x] **Step 4: Review S10 checklist**

Confirm S10-T01 through S10-T12 are covered: precheck, final Diff, validation, confirmation, local merge, commit, task status, merge log, conflict detection, conflict files, no silent overwrite.

## Execution Notes

- `cargo test commands::merge::tests -- --nocapture`: 5 passed.
- `cargo test merge_task_branch -- --nocapture`: 2 passed.
- `cargo test`: 58 passed.
- `npm run build` in `apps/desktop`: passed; Vite still reports the existing Monaco large chunk warning.
- `rustfmt --check` passed for the touched S10 Rust implementation files `commands/merge.rs` and `git/mod.rs`. Full module-tree rustfmt remains affected by pre-existing newline-style differences in unrelated Rust modules on Windows.
- Read-only review found one blocking issue: conflicted merges returned conflict files but not Git's concrete error reason. S10 now returns `errorReason`, records it in `merge-record.json`, and displays it in the conflict UI.
- Larger audit enhancements noted for later milestones: bind validation evidence to source HEAD/final Diff, enrich merge records with before/after SHAs and diff artifact IDs, and consider a separate conflict diagnostic artifact name.
