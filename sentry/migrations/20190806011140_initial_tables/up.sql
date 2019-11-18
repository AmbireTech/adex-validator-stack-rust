CREATE TABLE channels
(
    channel_id     VARCHAR(66)              NOT NULL,
    creator        VARCHAR(255)             NOT NULL,
    deposit_asset  VARCHAR(42)              NOT NULL,
    deposit_amount VARCHAR(255)             NOT NULL, -- @TODO change the deposit to BigNum compatible field
    valid_until    TIMESTAMP WITH TIME ZONE NOT NULL,
    spec           JSONB                    NOT NULL,

    PRIMARY KEY (channel_id)
);

CREATE INDEX idx_valid_until ON channels(valid_until);
CREATE INDEX idx_spec ON channels((spec->'validator'->>'id'));

CREATE TABLE validator_messages
(
    channel_id     VARCHAR(66)              NOT NULL,
    "from"         VARCHAR(255)             NOT NULL,
    msg            JSONB                    NOT NULL,
    received       TIMESTAMP WITH TIME ZONE NOT NULL,

    PRIMARY KEY (channel_id)
);

CREATE INDEX idx_received ON validator_messages(received);
CREATE INDEX ON validator_messages((msg->>'type'));
CREATE INDEX ON validator_messages((msg->>'stateRoot'));

CREATE TABLE event_aggregates
(
    channel_id    VARCHAR(66)              NOT NULL,
    created       TIMESTAMP WITH TIME ZONE NOT NULL,
    events        JSONB                    NOT NULL,

    PRIMARY KEY (channel_id)
);

CREATE INDEX idx_created ON event_aggregates(created);
