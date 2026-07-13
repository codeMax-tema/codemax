CREATE TABLE IF NOT EXISTS recovery_actions (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  resource_type TEXT NOT NULL,
  resource_id TEXT,
  previous_status TEXT NOT NULL,
  recovery_status TEXT NOT NULL,
  strategy TEXT NOT NULL,
  reason TEXT NOT NULL,
  requires_confirmation INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  resolved_at TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_recovery_actions_open_resource
  ON recovery_actions(resource_type, resource_id, recovery_status)
  WHERE resolved_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_recovery_actions_task
  ON recovery_actions(task_id, created_at);
