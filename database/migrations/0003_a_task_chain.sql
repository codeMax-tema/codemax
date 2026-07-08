CREATE TABLE IF NOT EXISTS agent_sessions (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  status TEXT NOT NULL,
  stage TEXT NOT NULL,
  checkpoint_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_events (
  event_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  event_type TEXT NOT NULL,
  stage TEXT NOT NULL,
  message TEXT NOT NULL,
  created_at TEXT NOT NULL,
  payload TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS validation_rounds (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  round_index INTEGER NOT NULL,
  status TEXT NOT NULL,
  command_run_id TEXT REFERENCES command_runs(id) ON DELETE SET NULL,
  analysis TEXT NOT NULL DEFAULT '',
  repair_summary TEXT NOT NULL DEFAULT '',
  validation_summary TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS merge_records (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  status TEXT NOT NULL,
  target_branch TEXT NOT NULL,
  source_branch TEXT NOT NULL,
  commit_sha TEXT NOT NULL DEFAULT '',
  commit_message TEXT NOT NULL DEFAULT '',
  conflict_files TEXT NOT NULL DEFAULT '[]',
  error_reason TEXT,
  record_path TEXT,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_sessions_task_id ON agent_sessions(task_id);
CREATE INDEX IF NOT EXISTS idx_agent_events_task_id ON agent_events(task_id, created_at);
CREATE INDEX IF NOT EXISTS idx_agent_events_type ON agent_events(event_type);
CREATE INDEX IF NOT EXISTS idx_validation_rounds_task_id ON validation_rounds(task_id, round_index);
CREATE INDEX IF NOT EXISTS idx_merge_records_task_id ON merge_records(task_id, created_at);
