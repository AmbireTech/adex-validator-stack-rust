CREATE TABLE channels
(
    id             VARCHAR(66)              NOT NULL,
    creator        VARCHAR(255)             NOT NULL,
    deposit_asset  VARCHAR(42)              NOT NULL,
    deposit_amount VARCHAR(255)             NOT NULL,
    valid_until    TIMESTAMP(2) WITH TIME ZONE NOT NULL,
    spec           JSONB                    NOT NULL,
    exhausted      BOOLEAN[2]

    PRIMARY KEY (id)
);

CREATE INDEX idx_channel_valid_until ON channels (valid_until);
CREATE INDEX idx_channels_spec_created ON channels ((spec ->> 'created'));

CREATE TABLE validator_messages
(
    channel_id VARCHAR(66)              NOT NULL REFERENCES channels (id) ON DELETE RESTRICT,
    "from"     VARCHAR(255)             NOT NULL,
    msg        JSONB                    NOT NULL,
    received   TIMESTAMP(2) WITH TIME ZONE NOT NULL
);

CREATE INDEX idx_validator_messages_received ON validator_messages (received);
CREATE INDEX idx_validator_messages_msg_type ON validator_messages ((msg ->> 'type'));
CREATE INDEX idx_validator_messages_msg_state_root ON validator_messages ((msg ->> 'stateRoot'));

CREATE TABLE event_aggregates
(
    channel_id VARCHAR(66)              NOT NULL REFERENCES channels (id) ON DELETE RESTRICT,
    created    TIMESTAMP(2) WITH TIME ZONE NOT NULL DEFAULT NOW(),
    event_type VARCHAR(255)             NOT NULL,
    earner     VARCHAR(255),
    count      VARCHAR                   NOT NULL,
    payout     VARCHAR                   NOT NULL
);

CREATE INDEX idx_event_aggregates_created ON event_aggregates (created);
CREATE INDEX idx_event_aggregates_channel ON event_aggregates (channel_id);
CREATE INDEX idx_event_aggregates_event_type ON event_aggregates (event_type);

CREATE AGGREGATE jsonb_object_agg(jsonb) (
  SFUNC = 'jsonb_concat',
  STYPE = jsonb,
  INITCOND = '{}'
);
