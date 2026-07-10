ALTER TABLE tasks ADD COLUMN target_branch TEXT NOT NULL DEFAULT '';
ALTER TABLE tasks ADD COLUMN workspace_kind TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE tasks ADD COLUMN source_path TEXT;
ALTER TABLE tasks ADD COLUMN original_write_authorized INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN workspace_estimated_bytes INTEGER NOT NULL DEFAULT 0;

UPDATE tasks
SET source_path = repository_path
WHERE source_path IS NULL OR TRIM(source_path) = '';
