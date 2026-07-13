CREATE TABLE IF NOT EXISTS file_edit_transactions (
  transaction_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  request_id TEXT NOT NULL,
  request_digest TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('prepared', 'applying', 'committed', 'rolling_back', 'rolled_back', 'failed')),
  operations_json TEXT NOT NULL,
  inverse_operations_json TEXT NOT NULL,
  results_json TEXT NOT NULL DEFAULT '[]',
  applied_count INTEGER NOT NULL DEFAULT 0,
  approval_id TEXT,
  diff_artifact_id TEXT,
  validation_round_id TEXT,
  proof_pack_id TEXT,
  error_category TEXT,
  error_message TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  committed_at TEXT,
  rolled_back_at TEXT,
  UNIQUE(task_id, request_id)
);
CREATE INDEX IF NOT EXISTS idx_file_edit_transactions_recovery ON file_edit_transactions(status, updated_at);
CREATE INDEX IF NOT EXISTS idx_file_edit_transactions_task ON file_edit_transactions(task_id, created_at);
