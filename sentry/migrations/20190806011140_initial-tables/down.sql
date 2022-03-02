-- This file should undo anything in `up.sql`
ALTER TABLE campaigns DROP CONSTRAINT fk_campaigns_channel_id;
ALTER TABLE spendable DROP CONSTRAINT fk_spendable_channel_id;
ALTER TABLE validator_messages DROP CONSTRAINT fk_validator_messages_channel_id;
ALTER TABLE accounting DROP CONSTRAINT fk_accounting_channel_id;
DROP TABLE validator_messages, channels, analytics, campaigns, spendable, accounting;
-- Types should be dropped last
DROP TYPE AccountingSide;
