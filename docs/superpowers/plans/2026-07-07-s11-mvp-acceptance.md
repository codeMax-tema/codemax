# S11 MVP Acceptance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add repeatable S11 acceptance coverage for the MVP path from demo repository setup through worktree validation, Agent repair, Diff, delivery report, approval, merge, memory, and cleanup.

**Architecture:** Keep S11 as a verification layer over existing S3-S10 modules. Rust acceptance tests exercise local Git, storage, command execution, Diff, delivery, approvals, merge, and cleanup with a temporary demo repository. Python Agent tests exercise deterministic validation failure, structured repair, retry, completion, and memory-window behavior.

**Tech Stack:** Rust cargo tests, Python pytest, local Git, existing SQLite repositories, existing command executor, existing Agent graph.

---

### Task 1: Python Agent MVP Repair Acceptance

**Files:**
- Create: `agent/tests/test_s11_mvp_acceptance.py`

- [ ] **Step 1: Write the failing test**

```python
def test_s11_agent_repairs_demo_repo_until_validation_passes(tmp_path):
    # Create a demo worktree with a failing validation command.
    # Run the Agent graph, execute validation, feed the failure back, verify
    # CODEMAX_REPAIR is applied, run validation again, and assert completion.
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests/test_s11_mvp_acceptance.py -q` from `agent`.
Expected: FAIL before the new test file and repair harness are fully wired.

- [ ] **Step 3: Keep implementation inside the test harness**

Use the existing `run_agent_graph`, `ValidationResult`, and `create_initial_state` APIs. Do not add production Agent behavior unless the test exposes a real bug.

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest tests/test_s11_mvp_acceptance.py -q` from `agent`.
Expected: PASS with no network or model calls.

### Task 2: Rust MVP E2E Acceptance

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/diff.rs`
- Modify: `apps/desktop/src-tauri/src/commands/delivery.rs`
- Modify: `apps/desktop/src-tauri/src/commands/merge.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Create: `apps/desktop/src-tauri/src/commands/s11_acceptance.rs`

- [ ] **Step 1: Write the failing Rust acceptance test**

```rust
#[tokio::test]
async fn mvp_demo_repo_runs_from_worktree_to_local_merge() {
    // Build a temp Git repo, create task storage, create worktree, run failed
    // validation, apply a fix, run passed validation, generate Diff and delivery,
    // prepare merge, merge locally, then confirm cleanup preserves permanent evidence.
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml s11 -- --nocapture`.
Expected: FAIL until command inner helpers are reusable and delivery reports latest validation status correctly.

- [ ] **Step 3: Expose existing command inner helpers only within the crate**

Delegate Tauri command wrappers to `pub(crate)` inner functions for Diff, delivery, prepare merge, and merge. Keep external IPC behavior unchanged.

- [ ] **Step 4: Fix final delivery status if S11 red test proves retry history is treated as final failure**

Delivery should summarize the latest run per validation command/cwd, matching merge precheck behavior, so repaired tasks can produce a passed final report.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml s11 -- --nocapture`.
Expected: PASS and create only temporary files under system temp directories.

### Task 3: S11 Contracts, Scripts, And Handoff

**Files:**
- Modify: `package.json`
- Modify: `tests/architecture/verify-architecture.mjs`
- Create: `docs/s11/mvp-acceptance.md`
- Modify: `task_plan.md`
- Modify: `findings.md`
- Modify: `progress.md`

- [ ] **Step 1: Write the contract red check**

Extend the architecture contract to require the S11 acceptance test and S11 documentation.

- [ ] **Step 2: Add the S11 check script**

Add `check:s11` to run both Python and Rust S11 acceptance checks without starting the desktop UI.

- [ ] **Step 3: Document acceptance coverage**

Record how S11-T01 through S11-T14 are covered, including user-facing storage and evidence guarantees.

- [ ] **Step 4: Run full verification**

Run:

```powershell
npm run check:architecture
npm run check:s11
npm run check:frontend
npm run build:desktop
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

Expected: All pass, aside from known non-blocking build warnings if Vite reports large Monaco chunks.

## Self-Review

- Spec coverage: S11-T01 to S11-T14 map to Python Agent acceptance, Rust MVP E2E acceptance, storage cleanup checks, memory checks, and handoff docs.
- Placeholder scan: No TBD/TODO placeholders are used.
- Type consistency: The plan uses existing names from `tauriClient.ts`, Rust command modules, and Python Agent graph APIs.
