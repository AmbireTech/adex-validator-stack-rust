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
