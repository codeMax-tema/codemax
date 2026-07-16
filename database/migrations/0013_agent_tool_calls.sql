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
