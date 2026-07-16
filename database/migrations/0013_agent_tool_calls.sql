CREATE TABLE IF NOT EXISTS agent_tool_calls (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  call_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  request_summary TEXT NOT NULL,
  result_summary TEXT,
  status TEXT NOT NULL CHECK (status IN ('requested', 'succeeded', 'failed', 'cancelled')),
  duration_ms INTEGER,
  transaction_id TEXT,
  command_run_id TEXT,
  context_sources_json TEXT NOT NULL DEFAULT '[]',
  artifact_refs_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  completed_at TEXT,
  UNIQUE(task_id, call_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_tool_calls_task_id ON agent_tool_calls(task_id);
