CREATE TABLE IF NOT EXISTS rule_registry (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  category TEXT NOT NULL,
  severity TEXT NOT NULL,
  description TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS rule_hits (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  rule_id TEXT NOT NULL,
  status TEXT NOT NULL,
  message TEXT NOT NULL,
  evidence_path TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS hook_approvals (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  hook_id TEXT NOT NULL,
  request_reason TEXT NOT NULL,
  status TEXT NOT NULL,
  reviewer TEXT,
  resolved_reason TEXT,
  created_at TEXT NOT NULL,
  resolved_at TEXT
);

CREATE TABLE IF NOT EXISTS hook_runs (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  hook_id TEXT NOT NULL,
  lifecycle TEXT NOT NULL,
  status TEXT NOT NULL,
  message TEXT NOT NULL,
  command TEXT,
  evidence_path TEXT,
  approval_id TEXT REFERENCES hook_approvals(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS model_arena_decisions (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  status TEXT NOT NULL,
  selected_model TEXT,
  selected_proposal_id TEXT,
  rationale TEXT NOT NULL,
  compared_models_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rule_hits_task_id ON rule_hits(task_id);
CREATE INDEX IF NOT EXISTS idx_hook_approvals_task_id ON hook_approvals(task_id);
CREATE INDEX IF NOT EXISTS idx_hook_runs_task_id ON hook_runs(task_id);
CREATE INDEX IF NOT EXISTS idx_model_arena_task_id ON model_arena_decisions(task_id);
