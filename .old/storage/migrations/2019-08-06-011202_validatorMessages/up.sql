CREATE TABLE validatorMessages
(
    channel_id     VARCHAR(66)              NOT NULL,
    from           VARCHAR(255)             NOT NULL,
    msg            JSONB                    NOT NULL,
    received       TIMESTAMP WITH TIME ZONE NOT NULL,

    PRIMARY KEY (channel_id)
);

CREATE INDEX idx_received ON validatorMessages(received);
CREATE INDEX ON validatorMessages((msg->>'type'));
CREATE INDEX ON validatorMessages((msg->>'stateRoot'));
