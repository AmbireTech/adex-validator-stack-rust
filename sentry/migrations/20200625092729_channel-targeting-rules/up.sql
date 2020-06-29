ALTER TABLE channels ADD COLUMN targeting_rules JSONB NOT NULL DEFAULT '[]';
