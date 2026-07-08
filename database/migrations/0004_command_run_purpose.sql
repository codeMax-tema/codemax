CREATE TABLE IF NOT EXISTS command_runs (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  command TEXT NOT NULL,
  cwd TEXT NOT NULL,
  status TEXT NOT NULL,
  stdout_path TEXT,
  stderr_path TEXT,
  exit_code INTEGER,
  duration_ms INTEGER,
  created_at TEXT NOT NULL
);

ALTER TABLE command_runs
ADD COLUMN purpose TEXT NOT NULL DEFAULT 'diagnostic';

UPDATE command_runs
SET purpose = 'validation'
WHERE id LIKE 'validation-%';

CREATE INDEX IF NOT EXISTS idx_command_runs_task_id ON command_runs(task_id);

CREATE INDEX IF NOT EXISTS idx_command_runs_task_purpose
ON command_runs(task_id, purpose, created_at);
