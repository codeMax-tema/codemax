# CodeMax V3 Autonomous Tool Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `workflowVersion = 3` the default for new programming tasks and execute them through a persistent model → Rust Runtime tool → tool-result feedback loop, while fully recovering legacy V1/V2 tasks with their original workflow.

**Architecture:** Python owns model messages, checkpoint state and protocol validation; Rust/Tauri owns every Runtime tool execution, persistence, safety policy and side effect. A V3 Python endpoint yields one strict tool request at a time; Rust persists and executes it, posts a sanitized result back, and continues until an approval or terminal boundary. V1/V2 states remain version-dispatched to their existing complete LangGraph workflow.

**Tech Stack:** Python 3.14 / FastAPI / Pydantic / pytest, Rust / Tauri 2 / Tokio / rusqlite, TypeScript contract verification, SQLite migrations.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `agent/app/graph/state.py` | V3 phase/status fields, durable tool-call metadata, default workflow version selection. |
| `agent/app/autonomous/loop.py` | One-model-turn V3 orchestration and tool-result feedback state transitions. |
| `agent/app/autonomous/__init__.py` | Export the V3 orchestration entry points. |
| `agent/app/api/tasks.py` | Version dispatch, `tool-result` endpoint, V1/V2 full legacy recovery routing. |
| `agent/app/model_gateway.py` | Strict model tool-call response handling used by the V3 loop. |
| `agent/app/tools/protocol.py` | Shared Python request/result protocol validation. |
| `agent/tests/test_autonomous_agent_loop.py` | V3 unit tests and legacy recovery regression tests. |
| `agent/tests/test_tool_runtime_protocol.py` | HTTP/API tests for persisted V3 tool request/result exchanges. |
| `contracts/agent-api.schema.json` | Runtime tool-result request/response contract schemas. |
| `database/migrations/0013_agent_tool_calls.sql` | Durable Runtime tool-call records and uniqueness constraints. |
| `apps/desktop/src-tauri/src/storage/mod.rs` | Tool-call repository, idempotency, audit persistence and query methods. |
| `apps/desktop/src-tauri/src/commands/agent_tools.rs` | Rust Runtime authoritative tool dispatcher and structured result generation. |
| `apps/desktop/src-tauri/src/commands/agent.rs` | Drive V3 requests across Agent HTTP and Rust Runtime; post results back. |
| `apps/desktop/src-tauri/src/commands/mod.rs` | Export the `agent_tools` command module. |
| `apps/desktop/src-tauri/src/lib.rs` | Register any new Tauri command only if it is exposed to the frontend. |
| `apps/desktop/src-tauri/src/commands/s11_acceptance.rs` | Regression coverage for a V3 task that uses a real Rust transaction and command result. |
| `tests/contracts/verify-ipc-contract.mjs` | Validate new IPC/runtime bridge types are present and strict. |

## Task 1: Add V3 state invariants and preserve V1/V2 full recovery

**Files:**
- Modify: `agent/app/graph/state.py`
- Modify: `agent/app/api/tasks.py`
- Test: `agent/tests/test_autonomous_agent_loop.py`

- [ ] **Step 1: Write failing default-version and legacy-recovery tests**

```python
def test_new_programming_task_defaults_to_v3(tmp_path) -> None:
    state = create_initial_state(
        task_id="v3-default",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="Use tools",
        task_type="programming",
    )
    assert state.workflow_version == 3


def test_legacy_v2_state_uses_full_legacy_recovery(monkeypatch, tmp_path) -> None:
    state = create_initial_state(
        task_id="legacy-v2",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="Legacy task",
        workflow_version=2,
    )
    calls: list[str] = []
    monkeypatch.setattr("app.api.tasks.run_agent_graph", lambda value: calls.append("legacy") or value)

    recovered = advance_state_for_workflow(state)

    assert recovered.workflow_version == 2
    assert calls == ["legacy"]
```

- [ ] **Step 2: Run the tests and verify they fail**

Run from `D:\codemax\agent`:

```powershell
.\.venv\Scripts\python.exe -m pytest tests/test_autonomous_agent_loop.py -q
```

Expected: failure because new tasks still default to the legacy version and `advance_state_for_workflow` does not exist.

- [ ] **Step 3: Add the minimum state and version-dispatch implementation**

In `agent/app/graph/state.py`, add explicit V3-compatible phases and metadata without altering V1/V2 serialized values:

```python
class AgentPhase(StrEnum):
    # retain every existing V1/V2 value
    RUNNING_MODEL = "running_model"
    WAITING_RUNTIME = "waiting_runtime"
    CANCELLED = "cancelled"


class AgentToolRequest(AgentModel):
    call_id: str = Field(alias="callId")
    tool_name: str = Field(alias="toolName")
    arguments: dict[str, object] = Field(default_factory=dict)
    reason: str = ""
    status: ToolRequestStatus = ToolRequestStatus.REQUESTED
    context_sources: list[str] = Field(default_factory=list, alias="contextSources")
    request_digest: str | None = Field(default=None, alias="requestDigest")
```

In `agent/app/api/tasks.py`, introduce a single dispatcher:

```python
def advance_state_for_workflow(state: AgentState) -> AgentState:
    if state.workflow_version >= 3:
        return advance_autonomous_turn(state)
    return run_agent_graph(state)
```

Create new programming tasks with `workflowVersion=3`; preserve the persisted version of every V1/V2 state and always route it through `run_agent_graph` for complete legacy recovery.

- [ ] **Step 4: Re-run the targeted tests**

```powershell
.\.venv\Scripts\python.exe -m pytest tests/test_autonomous_agent_loop.py -q
```

Expected: the two new tests pass and all pre-existing checkpoint round-trip tests remain green.

- [ ] **Step 5: Commit the state boundary**

```powershell
git add agent/app/graph/state.py agent/app/api/tasks.py agent/tests/test_autonomous_agent_loop.py
git commit -m "feat(agent): default new programming tasks to v3"
```

## Task 2: Implement the Python one-turn autonomous loop

**Files:**
- Create: `agent/app/autonomous/__init__.py`
- Create: `agent/app/autonomous/loop.py`
- Modify: `agent/app/model_gateway.py`
- Modify: `agent/app/tools/protocol.py`
- Test: `agent/tests/test_autonomous_agent_loop.py`

- [ ] **Step 1: Write failing multi-round state tests**

```python
def test_v3_turn_emits_a_runtime_request_and_records_assistant_message(tmp_path) -> None:
    gateway = ScriptedGateway([
        tool_call("call-search-1", "search_text", {"query": "AgentState"}),
    ])
    state = v3_state(tmp_path)

    next_state = advance_autonomous_turn(state, gateway=gateway)

    assert next_state.phase == AgentPhase.WAITING_RUNTIME
    assert next_state.pending_tool_request.call_id == "call-search-1"
    assert next_state.agent_messages[-1].tool_calls[0].name == "search_text"
    assert next_state.agent_round == 1


def test_v3_result_is_appended_as_tool_message_then_drives_next_model_turn(tmp_path) -> None:
    gateway = ScriptedGateway([
        tool_call("call-read-2", "read_file", {"path": "agent/app/graph/state.py"}),
    ])
    pending = advance_autonomous_turn(v3_state(tmp_path), gateway=gateway)

    next_state = apply_runtime_tool_result(
        pending,
        AgentToolResult(
            callId="call-search-1",
            toolName="search_text",
            status="succeeded",
            output={"matches": ["agent/app/graph/state.py"]},
        ),
        gateway=gateway,
    )

    assert next_state.agent_messages[-2].role == "tool"
    assert next_state.agent_messages[-2].tool_call_id == "call-search-1"
    assert next_state.pending_tool_request.call_id == "call-read-2"
```

- [ ] **Step 2: Run the tests and verify they fail**

```powershell
.\.venv\Scripts\python.exe -m pytest tests/test_autonomous_agent_loop.py -q
```

Expected: import error for `app.autonomous.loop` or assertion failures because the fixed graph does not emit Runtime requests.

- [ ] **Step 3: Implement the V3 loop with a single external boundary per tool call**

Create `agent/app/autonomous/loop.py` with these public functions:

```python
def advance_autonomous_turn(
    state: AgentState,
    *,
    gateway: ModelGateway | None = None,
) -> AgentState: ...


def apply_runtime_tool_result(
    state: AgentState,
    result: AgentToolResult,
    *,
    gateway: ModelGateway | None = None,
) -> AgentState: ...
```

`advance_autonomous_turn` must:

1. reject calls unless `workflow_version >= 3` and no pending tool request exists;
2. stop with `NEEDS_INTERVENTION` when the round or token limit is reached;
3. call `gateway.chat(messages=..., tools=builtin_tool_registry().definitions, tool_choice="auto")`;
4. append the returned assistant message exactly once;
5. validate the first returned Tool Call using `ToolRegistry.validate_call`;
6. set `WAITING_RUNTIME` and `pending_tool_request` without executing any local side effect;
7. accept a schema-valid `complete_task` call only as an explicit terminal completion.

`apply_runtime_tool_result` must:

1. require a matching pending `callId` and `toolName`;
2. append a `role="tool"` message containing only the structured, already-sanitized result;
3. clear the pending request and preserve `last_tool_result`;
4. change to `WAITING_APPROVAL`, `CANCELLED`, `NEEDS_INTERVENTION`, or a next `RUNNING_MODEL` turn based on the Runtime result status;
5. fingerprint repeated `(tool_name, canonical_json(arguments), result.status)` cycles and enter `NEEDS_INTERVENTION` before another identical no-progress request.

Do not call `run_agent_graph` from either V3 function.

- [ ] **Step 4: Add strict protocol errors and test them**

Add and cover these cases in `test_autonomous_agent_loop.py`:

```python
@pytest.mark.parametrize("call", [
    tool_call("", "search_text", {"query": "x"}),
    tool_call("call-1", "unknown_tool", {}),
    tool_call("call-1", "search_text", {"unexpected": "x"}),
])
def test_v3_invalid_model_tool_call_enters_needs_intervention(call, tmp_path) -> None: ...


def test_v3_rejects_runtime_result_for_the_wrong_call_id(tmp_path) -> None: ...

def test_v3_identical_no_progress_loop_enters_needs_intervention(tmp_path) -> None: ...
```

- [ ] **Step 5: Run the Python V3 loop tests**

```powershell
.\.venv\Scripts\python.exe -m pytest tests/test_autonomous_agent_loop.py tests/test_model_gateway.py tests/test_tool_registry.py -q
```

Expected: all selected tests pass.

- [ ] **Step 6: Commit the Python autonomous loop**

```powershell
git add agent/app/autonomous agent/app/model_gateway.py agent/app/tools/protocol.py agent/tests/test_autonomous_agent_loop.py
git commit -m "feat(agent): add v3 autonomous tool loop"
```

## Task 3: Expose durable tool-result exchange through the Agent API

**Files:**
- Modify: `agent/app/api/tasks.py`
- Modify: `agent/app/graph/state.py`
- Test: `agent/tests/test_tool_runtime_protocol.py`

- [ ] **Step 1: Write failing API round-trip tests**

```python
def test_v3_advance_then_tool_result_returns_next_pending_request(client, v3_request) -> None:
    created = client.post("/api/v1/tasks", json=v3_request).json()
    task_id = created["taskId"]
    pending = client.post(f"/api/v1/tasks/{task_id}/advance", json={"reason": "start"}).json()

    result = client.post(
        f"/api/v1/tasks/{task_id}/tool-result",
        json={
            "callId": pending["state"]["pendingToolRequest"]["callId"],
            "toolName": "search_text",
            "status": "succeeded",
            "output": {"matches": ["README.md"]},
            "artifactRefs": [],
            "truncated": False,
        },
    )

    assert result.status_code == 200
    assert result.json()["state"]["agentMessages"][-2]["role"] == "tool"


def test_v3_tool_result_rejects_a_mismatched_pending_call(client, v3_request) -> None: ...
```

- [ ] **Step 2: Run the API test and verify it fails**

```powershell
.\.venv\Scripts\python.exe -m pytest tests/test_tool_runtime_protocol.py -q
```

Expected: `404` because `/tool-result` does not yet exist.

- [ ] **Step 3: Add the endpoint and version dispatch**

Add these Pydantic request fields in `agent/app/api/tasks.py`:

```python
class ToolResultRequest(BaseModel):
    call_id: str = Field(alias="callId")
    tool_name: str = Field(alias="toolName")
    status: ToolResultStatus
    output: dict[str, object] = Field(default_factory=dict)
    error_code: str | None = Field(default=None, alias="errorCode")
    error_message: str | None = Field(default=None, alias="errorMessage")
    artifact_refs: list[str] = Field(default_factory=list, alias="artifactRefs")
    truncated: bool = False
```

Add `POST /api/v1/tasks/{task_id}/tool-result`. It must load the persisted state under `_tasks_lock`, call `apply_runtime_tool_result`, update the scheduler, save the resulting checkpoint, and return the same `AdvanceAgentTaskResponse` shape as `advance`.

Replace V1/V2-only calls to `run_agent_graph` in `advance_task`, approval resume, validation result and file-commit result with `advance_state_for_workflow` where the state is V3-capable. Keep the exact existing V1/V2 behavior unchanged.

- [ ] **Step 4: Run API and regression tests**

```powershell
.\.venv\Scripts\python.exe -m pytest tests/test_tool_runtime_protocol.py tests/test_file_commit_protocol.py tests/test_autonomous_agent_loop.py -q
```

Expected: all tests pass; V1/V2 file-commit protocol remains unchanged.

- [ ] **Step 5: Commit the API boundary**

```powershell
git add agent/app/api/tasks.py agent/app/graph/state.py agent/tests/test_tool_runtime_protocol.py
git commit -m "feat(agent): accept runtime tool results for v3 tasks"
```

## Task 4: Define the durable Rust Runtime tool-call contract and audit store

**Files:**
- Modify: `contracts/agent-api.schema.json`
- Create: `database/migrations/0013_agent_tool_calls.sql`
- Modify: `apps/desktop/src-tauri/src/storage/mod.rs`
- Test: `apps/desktop/src-tauri/src/storage/mod.rs`
- Test: `tests/contracts/verify-ipc-contract.mjs`

- [ ] **Step 1: Write failing storage and contract tests**

Add a Rust test:

```rust
#[test]
fn agent_tool_call_is_idempotent_per_task_and_call_id() {
    let store = SqliteStore::open_in_memory().expect("open sqlite");
    store.migrate().expect("migrate");
    let repo = AgentToolCallRepository::new(store.connection());

    repo.begin(NewAgentToolCall::requested("task-1", "call-1", "search_text", "{}"))
        .expect("first request");
    let duplicate = repo.begin(NewAgentToolCall::requested("task-1", "call-1", "search_text", "{}"))
        .expect("idempotent request");

    assert_eq!(duplicate.status, "requested");
}
```

Extend `tests/contracts/verify-ipc-contract.mjs` to require `callId`, `toolName`, `status`, `output`, `artifactRefs`, and `truncated` in the Agent tool result schema.

- [ ] **Step 2: Run the targeted checks and verify failure**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml agent_tool_call_is_idempotent_per_task_and_call_id -- --nocapture
node tests/contracts/verify-ipc-contract.mjs
```

Expected: Rust cannot find `AgentToolCallRepository`; contract test reports missing schema requirements.

- [ ] **Step 3: Add migration, repository and strict schema**

Create `0013_agent_tool_calls.sql` with an `agent_tool_calls` table containing:

```sql
CREATE TABLE agent_tool_calls (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  call_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  request_summary TEXT NOT NULL,
  result_summary TEXT,
  status TEXT NOT NULL,
  duration_ms INTEGER,
  transaction_id TEXT,
  command_run_id TEXT,
  context_sources_json TEXT NOT NULL DEFAULT '[]',
  artifact_refs_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  completed_at TEXT,
  UNIQUE(task_id, call_id)
);
```

Implement `AgentToolCallRepository` with `begin`, `complete`, `get_required`, and a request-digest mismatch rejection. Summaries must be supplied by the caller after sanitization; this table must not store raw tool output or model prompt content.

Update `contracts/agent-api.schema.json` with exact camelCase request/result definitions used by Python and Rust.

- [ ] **Step 4: Run storage and contract tests**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml agent_tool_call -- --nocapture
node tests/contracts/verify-ipc-contract.mjs
```

Expected: new idempotency and mismatch tests pass; IPC contract remains strict.

- [ ] **Step 5: Commit the durable tool-call store**

```powershell
git add contracts/agent-api.schema.json database/migrations/0013_agent_tool_calls.sql apps/desktop/src-tauri/src/storage/mod.rs tests/contracts/verify-ipc-contract.mjs
git commit -m "feat(runtime): persist v3 agent tool calls"
```

## Task 5: Implement Rust Runtime tool dispatch without Python side effects

**Files:**
- Create: `apps/desktop/src-tauri/src/commands/agent_tools.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/desktop/src-tauri/src/commands/agent.rs`
- Test: `apps/desktop/src-tauri/src/commands/agent_tools.rs`

- [ ] **Step 1: Write failing dispatcher tests**

```rust
#[tokio::test]
async fn dispatch_search_text_returns_workspace_matches() {
    let (storage, workspace) = seeded_task_workspace("tool-search");
    std::fs::write(workspace.join("README.md"), "AgentState appears here\n").expect("write fixture");

    let result = dispatch_agent_tool_call(
        &storage,
        AgentToolCallRequest::new("tool-search", "call-search", "search_text", json!({"query":"AgentState"})),
    ).await.expect("dispatch");

    assert_eq!(result.status, "succeeded");
    assert_eq!(result.tool_name, "search_text");
}

#[tokio::test]
async fn dispatch_apply_file_edits_uses_recoverable_transaction() {
    let (storage, workspace) = seeded_task_workspace("tool-edit");

    let result = dispatch_agent_tool_call(
        &storage,
        AgentToolCallRequest::new("tool-edit", "call-edit", "apply_file_edits", json!({
            "edits":[{"operation":"create","path":"v3.txt","content":"ok","summary":"create"}]
        })),
    ).await.expect("dispatch");

    assert!(result.transaction_id.is_some());
    assert_eq!(std::fs::read_to_string(workspace.join("v3.txt")).unwrap(), "ok");
}
```

- [ ] **Step 2: Run the dispatcher tests and verify failure**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml dispatch_agent_tool_call -- --nocapture
```

Expected: compilation failure because `agent_tools` and `dispatch_agent_tool_call` do not exist.

- [ ] **Step 3: Implement the authoritative dispatcher**

Create `agent_tools.rs` with these public types:

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCallRequest {
    pub task_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub context_sources: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolCallResult {
    pub call_id: String,
    pub tool_name: String,
    pub status: String,
    pub output: Value,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub artifact_refs: Vec<String>,
    pub truncated: bool,
    pub transaction_id: Option<String>,
    pub command_run_id: Option<String>,
}
```

`dispatch_agent_tool_call` must first persist `requested`, resolve the task worktree, validate arguments against the Runtime-owned tool name, and then dispatch:

- `list_files`, `search_text`, `read_file`, `git_status`, `git_diff`: read-only and constrained to the selected worktree;
- `apply_file_edits`: translate strict edits into `ExecuteSafeFileOperationsRequest` and call `files::execute_transaction` with `request_id = call_id`;
- `run_command`: call the existing command execution boundary with `run_id = call_id` and preserve existing approval/allowlist controls;
- `update_todos`: validate every Todo item and replace the task Todo set through `TodoRepository`, using `(task_id, call_id)` idempotency so a replay cannot duplicate items;
- `request_approval`: create the persisted approval through the existing Rust approval repository with the current task, action, target, content digest, contract digest, expiry and `call_id`; return `waiting_approval` and never auto-approve it;
- `complete_task`: validate summary, changed-files and remaining-risks arguments, persist a `completed` Runtime result for the call, and return only the structured completion payload that Python uses to enter its terminal state. The existing `sync_agent_state` path remains responsible for reflecting the final Agent state in the task record.

Every branch must call `AgentToolCallRepository::complete` before returning. Errors must become sanitized structured results, never raw filesystem paths, environment values, or model secrets.

- [ ] **Step 4: Add negative tests**

```rust
#[tokio::test]
async fn dispatch_rejects_unknown_tool_without_side_effect() { ... }

#[tokio::test]
async fn dispatch_rejects_path_outside_task_worktree() { ... }

#[tokio::test]
async fn dispatch_reuses_completed_call_id_without_second_write() { ... }

#[tokio::test]
async fn dispatch_update_todos_replaces_task_todos_once() { ... }

#[tokio::test]
async fn dispatch_request_approval_persists_waiting_approval_without_auto_approval() { ... }

#[tokio::test]
async fn dispatch_complete_task_returns_a_validated_terminal_payload() { ... }
```

- [ ] **Step 5: Run Rust dispatcher and full safety regression tests**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml dispatch_ -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml safe_fs -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml agent_file_commit -- --nocapture
```

Expected: dispatcher tests, safe file tests and existing file-commit transaction tests pass.

- [ ] **Step 6: Commit the Rust dispatcher**

```powershell
git add apps/desktop/src-tauri/src/commands/agent_tools.rs apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/commands/agent.rs apps/desktop/src-tauri/src/storage/mod.rs
git commit -m "feat(runtime): dispatch v3 agent tools safely"
```

## Task 6: Drive the complete V3 Agent ↔ Runtime feedback cycle

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/agent.rs`
- Modify: `agent/app/api/tasks.py`
- Test: `apps/desktop/src-tauri/src/commands/agent.rs`
- Test: `agent/tests/test_tool_runtime_protocol.py`

- [ ] **Step 1: Write a failing end-to-end bridge test**

```rust
#[tokio::test]
async fn v3_agent_cycle_posts_runtime_result_and_requests_the_next_tool() {
    let runtime = scripted_agent_server([
        assistant_tool_call("call-search", "search_text", json!({"query":"needle"})),
        assistant_tool_call("call-read", "read_file", json!({"path":"README.md"})),
    ]).await;
    let (storage, workspace) = seeded_task_workspace("v3-cycle");
    std::fs::write(workspace.join("README.md"), "needle\n").unwrap();

    let response = drive_v3_agent_cycle(&runtime, &storage, "v3-cycle", 4).await.unwrap();

    assert_eq!(response.state["pendingToolRequest"]["callId"], "call-read");
    assert!(persisted_tool_call(&storage, "v3-cycle", "call-search").is_completed());
}
```

- [ ] **Step 2: Run the bridge test and verify it fails**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml v3_agent_cycle_posts_runtime_result_and_requests_the_next_tool -- --nocapture
```

Expected: failure because `drive_v3_agent_cycle` does not exist.

- [ ] **Step 3: Implement bounded bridge orchestration**

Add to `commands/agent.rs`:

```rust
async fn drive_v3_agent_cycle(
    agent: &AgentService,
    storage: &ManagedStorage,
    task_id: &str,
    max_tool_steps: usize,
) -> AppResult<Value> { ... }
```

The driver must:

1. call `/advance` once;
2. inspect the returned state;
3. when `workflowVersion >= 3` and `pendingToolRequest` is present, parse and dispatch it through `dispatch_agent_tool_call`;
4. post the structured result to `/tool-result`;
5. sync Agent state after every response;
6. stop immediately at `waiting_approval`, `completed`, `cancelled`, `failed`, or `needs_intervention`;
7. stop with a persisted, user-visible `needs_intervention` result if `max_tool_steps` is reached;
8. never auto-run legacy V1/V2 cycles beyond their existing calls.

Use the existing Agent runtime API error mapping and never place the model API key in a request, event, error message, tool audit or test fixture.

- [ ] **Step 4: Add cancellation and approval-boundary tests**

```rust
#[tokio::test]
async fn v3_cycle_stops_before_dispatching_a_waiting_approval_request() { ... }

#[tokio::test]
async fn v3_cycle_marks_needs_intervention_at_tool_step_limit() { ... }
```

- [ ] **Step 5: Run bridge, Rust, Python and contract checks**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml v3_ -- --nocapture
Set-Location D:\codemax\agent
.\.venv\Scripts\python.exe -m pytest tests/test_autonomous_agent_loop.py tests/test_tool_runtime_protocol.py -q
Set-Location D:\codemax
node tests/contracts/verify-ipc-contract.mjs
```

Expected: bridge tests, Python protocol tests and IPC contract test pass.

- [ ] **Step 6: Commit the end-to-end V3 bridge**

```powershell
git add apps/desktop/src-tauri/src/commands/agent.rs agent/app/api/tasks.py agent/tests/test_tool_runtime_protocol.py
git commit -m "feat(agent): bridge v3 tool results through runtime"
```

## Task 7: Prove V1/V2 full recovery and V3 regression behavior

**Files:**
- Modify: `agent/tests/test_autonomous_agent_loop.py`
- Modify: `apps/desktop/src-tauri/src/recovery.rs`
- Modify: `apps/desktop/src-tauri/src/commands/s11_acceptance.rs`
- Modify: `progress.md`
- Modify: `findings.md`

- [ ] **Step 1: Write V1/V2 recovery regression tests**

```python
@pytest.mark.parametrize("workflow_version, phase", [
    (1, AgentPhase.PLANNED),
    (1, AgentPhase.AWAITING_FILE_COMMIT),
    (2, AgentPhase.VALIDATING),
    (2, AgentPhase.REPAIRING),
    (2, AgentPhase.WAITING_APPROVAL),
])
def test_legacy_checkpoint_resumes_full_matching_workflow(workflow_version, phase, tmp_path) -> None: ...


def test_legacy_checkpoint_with_dangerous_replay_becomes_needs_intervention(tmp_path) -> None: ...
```

- [ ] **Step 2: Write a Rust acceptance test for V3 tool history**

```rust
#[test]
fn s11_v3_acceptance_records_search_edit_validation_and_delivery_evidence() {
    let evidence = run_v3_acceptance_fixture();
    assert_eq!(evidence.tool_names, ["search_text", "read_file", "apply_file_edits", "run_command", "complete_task"]);
    assert!(evidence.file_transaction_id.is_some());
    assert!(evidence.command_run_id.is_some());
}
```

- [ ] **Step 3: Run the tests and verify they fail before compatibility implementation is complete**

```powershell
Set-Location D:\codemax\agent
.\.venv\Scripts\python.exe -m pytest tests/test_autonomous_agent_loop.py -q
Set-Location D:\codemax
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml s11_v3_acceptance_records_search_edit_validation_and_delivery_evidence -- --nocapture
```

Expected: the new assertions fail until legacy dispatch and V3 auditing are fully wired.

- [ ] **Step 4: Implement only the missing recovery/audit pieces**

- Preserve all V1/V2 phase values and call their legacy resume route.
- When a legacy recovery would replay delete, overwrite, command, push or merge without a valid persisted authorization, persist `needsIntervention` with a diagnostic reason instead of silently downgrading to read-only.
- Extend S11 fixture support to expose the persisted V3 tool-call audit records and their transaction/command references.

- [ ] **Step 5: Run the full regression suite**

```powershell
Set-Location D:\codemax\agent
.\.venv\Scripts\python.exe -m pytest tests -q
Set-Location D:\codemax
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
npm run check
npm run check:a-line
npm run check:s11
```

Expected: Python, Rust, architecture, contract, frontend, runtime script, A-line and S11 checks all pass.

- [ ] **Step 6: Record evidence without marking unrelated P0 items complete**

Append actual command output, artifact paths, remaining P0-001/P0-004/P0-005/P0-006/P0-007 gaps, and the V3 scope boundary to `progress.md` and `findings.md`. Do not add a “P0-009 closed” conclusion until an installed-app E2E has been run and evidence files are present.

- [ ] **Step 7: Commit compatibility and verification evidence**

```powershell
git add agent/tests/test_autonomous_agent_loop.py apps/desktop/src-tauri/src/recovery.rs apps/desktop/src-tauri/src/commands/s11_acceptance.rs progress.md findings.md
git commit -m "test(agent): verify v3 loop and legacy recovery"
```

## Plan Self-Review

- **Spec coverage:** Tasks 1 and 7 implement the required full V1/V2 recovery policy. Tasks 2, 3 and 6 implement model decision, Runtime result feedback and bounded persistent V3 execution. Tasks 4 and 5 establish the strict contract, audit persistence and Rust-only side effects. Task 7 supplies the regression and evidence boundary.
- **Scope boundary:** No task claims to close Windows handle-pinned reads, merge approval binding, mandatory privacy interception, signing, installer lifecycle or clean-Windows E2E; those remain separate P0/P1 plans.
- **TDD:** Every implementation task begins with a named failing test, a failing command, minimal implementation, passing command and commit.
- **Consistency:** `workflowVersion=3`, `pendingToolRequest`, `AgentToolCallRequest`, `AgentToolCallResult`, `dispatch_agent_tool_call`, `advance_autonomous_turn`, `apply_runtime_tool_result`, and `drive_v3_agent_cycle` are used consistently throughout the plan.

## Task 3A: Harden V3 callback, persistence and concurrency boundaries

**Files:**
- Modify: `agent/app/api/tasks.py`
- Modify: `agent/app/graph/state.py`
- Modify: `agent/app/graph/checkpoint.py` only if a compare-and-save helper is required
- Modify: `agent/tests/test_tool_runtime_protocol.py`
- Modify: `agent/tests/test_autonomous_agent_loop.py`

- [ ] **Step 1: Add failing safety-boundary API tests**

```python
def test_unknown_workflow_version_fails_closed_without_state_mutation(client, store) -> None: ...
def test_mismatched_tool_result_returns_409_without_saving_or_scheduler_update(client, store) -> None: ...
def test_tool_result_save_failure_does_not_transition_scheduler(client, failing_store, scheduler) -> None: ...
def test_non_finite_or_non_json_tool_result_is_rejected_with_422(client) -> None: ...
def test_slow_v3_model_turn_does_not_hold_global_task_lock(client, blocking_gateway) -> None: ...
```

- [ ] **Step 2: Confirm the new tests fail**

```powershell
Set-Location D:\codemax\.worktrees\codex-v3-autonomous-tool-loop\agent
D:\codemax\agent\.venv\Scripts\python.exe -m pytest tests/test_tool_runtime_protocol.py tests/test_autonomous_agent_loop.py -q
```

Expected: failures demonstrate the prior mixed version routing, `200 accepted` conflict path, scheduler-before-save ordering, loose JSON request values, or global-lock model call.

- [ ] **Step 3: Implement fail-closed version classification and strict ToolResult validation**

Introduce one shared classifier used by every callback:

```python
def workflow_kind(workflow_version: int) -> Literal["legacy", "v3"]:
    if workflow_version in (1, 2):
        return "legacy"
    if workflow_version == 3:
        return "v3"
    raise HTTPException(status_code=409, detail="Unsupported workflow version.")
```

Use recursive `JsonValue` validation and reject NaN, Infinity, non-string artifact refs, empty call IDs and overlong identifiers at the FastAPI request boundary with `422`.

- [ ] **Step 4: Implement safe callback ordering and conflict semantics**

For `tool-result`:

```text
load + classify task
→ reject non-V3 / no pending / call mismatch with 409 and no mutation
→ exact consumed replay returns current state
→ apply result
→ save checkpoint successfully
→ update scheduler from saved state
→ return response
```

A checkpoint save error must leave scheduler unchanged. Do not return `accepted` for conflicts.

- [ ] **Step 5: Move V3 model network work outside the global lock**

Within the existing synchronous API architecture, split V3 advance into: lock-protected snapshot/lease acquisition; lock-free `advance_autonomous_turn`; lock-protected compare-and-save. A stale checkpoint or lost lease returns `409` with no scheduler mutation. V1/V2 retain their existing serial path.

- [ ] **Step 6: Add V3 scheduler slot ownership**

Acquire the scheduler slot only after a V3 task successfully reaches a model turn or pending Runtime request. Release it only after the terminal checkpoint is saved. Add a concurrency test that verifies V3 cannot exceed the configured concurrent task limit.

- [ ] **Step 7: Run verification and commit**

```powershell
D:\codemax\agent\.venv\Scripts\python.exe -m pytest tests/test_tool_runtime_protocol.py tests/test_autonomous_agent_loop.py -q
D:\codemax\agent\.venv\Scripts\python.exe -m pytest tests -q
git diff --check
git add agent/app/api/tasks.py agent/app/graph/state.py agent/app/graph/checkpoint.py agent/tests/test_tool_runtime_protocol.py agent/tests/test_autonomous_agent_loop.py
git commit -m "fix(agent): harden v3 callback boundaries"
```
