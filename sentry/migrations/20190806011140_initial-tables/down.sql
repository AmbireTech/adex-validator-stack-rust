-- This file should undo anything in `up.sql`
DROP AGGREGATE jsonb_object_agg(jsonb);
DROP TABLE event_aggregates, validator_messages, channels;
