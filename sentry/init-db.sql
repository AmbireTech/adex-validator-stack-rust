/**
    WARNING: this file is out of date!
*/
CREATE TABLE channels
(
    channel_id     VARCHAR(255)             NOT NULL,
    creator        VARCHAR(255)             NOT NULL,
    deposit_asset  VARCHAR(3)               NOT NULL,
    deposit_amount BIGINT                   NOT NULL, -- @TODO change the deposit to BigNum compatible field
    valid_until    TIMESTAMP WITH TIME ZONE NOT NULL,

    PRIMARY KEY (channel_id)
);

CREATE TABLE validators
(
    validator_id VARCHAR(255) NOT NULL,
    url          VARCHAR(255) NOT NULL,
    fee          BIGINT       NOT NULL DEFAULT '0',

    PRIMARY KEY (validator_id)
);

CREATE TABLE channel_specs
(
    channel_spec_id UUID                                          NOT NULL,
    channel_id      VARCHAR(255) REFERENCES channels (channel_id) NOT NULL,

    PRIMARY KEY (channel_spec_id)
);

CREATE TABLE channel_spec_validators
(
    channel_spec_id UUID REFERENCES channel_specs (channel_spec_id)   NOT NULL,
    validator_id    VARCHAR(255) REFERENCES validators (validator_id) NOT NULL,

    PRIMARY KEY (channel_spec_id, validator_id)
)