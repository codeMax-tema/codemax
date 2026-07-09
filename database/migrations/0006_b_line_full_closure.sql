PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS task_memory_usages (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  memory_id TEXT REFERENCES memory_items(id) ON DELETE SET NULL,
  memory_key TEXT NOT NULL,
  memory_scope TEXT NOT NULL,
  memory_scope_id TEXT,
  usage_type TEXT NOT NULL,
  value_preview TEXT NOT NULL DEFAULT '',
  tokens_estimate INTEGER NOT NULL DEFAULT 0,
  redacted INTEGER NOT NULL DEFAULT 0,
  blocked INTEGER NOT NULL DEFAULT 0,
  reason TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS preference_candidates (
  id TEXT PRIMARY KEY,
  task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
  scope TEXT NOT NULL,
  scope_id TEXT,
  preference_key TEXT NOT NULL,
  candidate_value TEXT NOT NULL,
  evidence TEXT NOT NULL DEFAULT '',
  confidence REAL NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'pending',
  redacted INTEGER NOT NULL DEFAULT 0,
  blocked INTEGER NOT NULL DEFAULT 0,
  reason TEXT NOT NULL DEFAULT '',
  decision_comment TEXT,
  accepted_memory_id TEXT REFERENCES memory_items(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  decided_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_task_memory_usages_task_id
ON task_memory_usages(task_id, created_at);

CREATE INDEX IF NOT EXISTS idx_task_memory_usages_memory_id
ON task_memory_usages(memory_id, created_at);

CREATE INDEX IF NOT EXISTS idx_preference_candidates_task_id
ON preference_candidates(task_id, status, created_at);

CREATE INDEX IF NOT EXISTS idx_preference_candidates_status
ON preference_candidates(status, updated_at);
