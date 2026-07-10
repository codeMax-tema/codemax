ALTER TABLE agent_sessions ADD COLUMN iterations INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agent_sessions ADD COLUMN repair_round INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agent_sessions ADD COLUMN max_repair_rounds INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agent_sessions ADD COLUMN validation_request_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE agent_sessions ADD COLUMN validation_round INTEGER NOT NULL DEFAULT 0;

ALTER TABLE validation_rounds ADD COLUMN repair_round INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_validation_rounds_repair_round
  ON validation_rounds(task_id, repair_round, round_index);
