ALTER TABLE approvals ADD COLUMN actor TEXT;
ALTER TABLE approvals ADD COLUMN action TEXT;
ALTER TABLE approvals ADD COLUMN target TEXT;
ALTER TABLE approvals ADD COLUMN arguments_digest TEXT;
ALTER TABLE approvals ADD COLUMN content_digest TEXT;
ALTER TABLE approvals ADD COLUMN scope TEXT;
ALTER TABLE approvals ADD COLUMN nonce TEXT;
ALTER TABLE approvals ADD COLUMN contract_digest TEXT;
ALTER TABLE approvals ADD COLUMN expires_at TEXT;
ALTER TABLE approvals ADD COLUMN consumed_at TEXT;
ALTER TABLE approvals ADD COLUMN consumed_by_call_id TEXT;
ALTER TABLE approvals ADD COLUMN invalidated_at TEXT;
ALTER TABLE approvals ADD COLUMN invalidation_reason TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_approvals_nonce
  ON approvals(nonce)
  WHERE nonce IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_approvals_authorization_lookup
  ON approvals(task_id, action, arguments_digest, content_digest, decision, consumed_at);
