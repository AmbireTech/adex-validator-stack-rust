-- This file should undo anything in `up.sql`
DROP CONSTRAINT fk_campaigns_channel_id, fk_spendable_channel_id, fk_validator_messages_channel_id, fk_accounting_channel_id;
DROP AGGREGATE jsonb_object_agg(jsonb);
DROP TABLE event_aggregates, validator_messages, channels, analytics, campaigns, spendable;
