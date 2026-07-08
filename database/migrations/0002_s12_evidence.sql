CREATE TABLE IF NOT EXISTS quality_gate_results (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  gate_type TEXT NOT NULL,
  status TEXT NOT NULL,
  message TEXT NOT NULL,
  evidence_path TEXT,
  override_reason TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS proof_packs (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  summary TEXT NOT NULL,
  proof_dir TEXT NOT NULL,
  export_path TEXT,
  delivery_score INTEGER NOT NULL DEFAULT 0,
  risk_level TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS delivery_scores (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  score INTEGER NOT NULL,
  test_score INTEGER NOT NULL,
  risk_score INTEGER NOT NULL,
  diff_score INTEGER NOT NULL,
  approval_score INTEGER NOT NULL,
  explanation TEXT NOT NULL,
  created_at TEXT NOT NULL
);
