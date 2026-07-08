PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS personal_profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  scope TEXT NOT NULL,
  scope_id TEXT,
  mode TEXT NOT NULL,
  model_id TEXT,
  reasoning_effort TEXT NOT NULL,
  permission_level TEXT NOT NULL,
  network_policy TEXT NOT NULL,
  privacy_mode TEXT NOT NULL,
  token_budget_total INTEGER NOT NULL,
  token_budget_per_call INTEGER NOT NULL,
  validation_policy TEXT NOT NULL,
  output_language TEXT NOT NULL,
  memory_scope TEXT NOT NULL,
  quality_gate_policy TEXT NOT NULL,
  is_active INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS run_contracts (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  profile_id TEXT REFERENCES personal_profiles(id) ON DELETE SET NULL,
  mode TEXT NOT NULL,
  model_id TEXT,
  reasoning_effort TEXT NOT NULL,
  permission_level TEXT NOT NULL,
  network_policy TEXT NOT NULL,
  allowed_paths_json TEXT NOT NULL DEFAULT '[]',
  allowed_commands_json TEXT NOT NULL DEFAULT '[]',
  validation_command TEXT,
  token_budget_total INTEGER NOT NULL,
  token_budget_per_call INTEGER NOT NULL,
  output_language TEXT NOT NULL,
  memory_scope TEXT NOT NULL,
  budget_overflow_policy TEXT NOT NULL,
  contract_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(task_id)
);

CREATE TABLE IF NOT EXISTS contract_breach_records (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  contract_id TEXT REFERENCES run_contracts(id) ON DELETE SET NULL,
  breach_type TEXT NOT NULL,
  requested_value TEXT NOT NULL,
  policy_value TEXT NOT NULL,
  status TEXT NOT NULL,
  approval_id TEXT REFERENCES approvals(id) ON DELETE SET NULL,
  reason TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS privacy_ledger_entries (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  event_type TEXT NOT NULL,
  data_kind TEXT NOT NULL,
  source_type TEXT NOT NULL,
  source_ref TEXT NOT NULL,
  destination TEXT NOT NULL,
  provider TEXT,
  model_id TEXT,
  action TEXT NOT NULL,
  sensitivity_level TEXT NOT NULL,
  findings_json TEXT NOT NULL DEFAULT '[]',
  redacted INTEGER NOT NULL DEFAULT 0,
  blocked INTEGER NOT NULL DEFAULT 0,
  reason TEXT NOT NULL DEFAULT '',
  size_bytes INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS token_budget_records (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  run_id TEXT,
  call_type TEXT NOT NULL,
  provider TEXT,
  model_id TEXT,
  phase TEXT NOT NULL,
  input_tokens_estimate INTEGER NOT NULL DEFAULT 0,
  output_tokens_estimate INTEGER NOT NULL DEFAULT 0,
  total_tokens_estimate INTEGER NOT NULL DEFAULT 0,
  budget_limit INTEGER NOT NULL DEFAULT 0,
  budget_remaining INTEGER NOT NULL DEFAULT 0,
  overflow_policy TEXT NOT NULL,
  quality_fallback TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS context_sources (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  run_id TEXT,
  source_type TEXT NOT NULL,
  source_ref TEXT NOT NULL,
  layer TEXT NOT NULL,
  included INTEGER NOT NULL DEFAULT 1,
  tokens_estimate INTEGER NOT NULL DEFAULT 0,
  sensitivity_level TEXT NOT NULL DEFAULT 'none',
  redacted INTEGER NOT NULL DEFAULT 0,
  blocked INTEGER NOT NULL DEFAULT 0,
  reason TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_personal_profiles_active
ON personal_profiles(is_active, scope, updated_at);

CREATE INDEX IF NOT EXISTS idx_run_contracts_task_id
ON run_contracts(task_id);

CREATE INDEX IF NOT EXISTS idx_contract_breach_records_task_id
ON contract_breach_records(task_id, created_at);

CREATE INDEX IF NOT EXISTS idx_privacy_ledger_entries_task_id
ON privacy_ledger_entries(task_id, created_at);

CREATE INDEX IF NOT EXISTS idx_privacy_ledger_entries_task_action
ON privacy_ledger_entries(task_id, action, created_at);

CREATE INDEX IF NOT EXISTS idx_token_budget_records_task_id
ON token_budget_records(task_id, created_at);

CREATE INDEX IF NOT EXISTS idx_context_sources_task_id
ON context_sources(task_id, created_at);
