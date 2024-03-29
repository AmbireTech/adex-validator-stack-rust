CREATE TABLE channels (
    id varchar(66) NOT NULL,
    leader varchar(42) NOT NULL,
    follower varchar(42) NOT NULL,
    guardian varchar(42) NOT NULL,
    token varchar(42) NOT NULL,
    -- Using varchar for U256 for simplicity
    nonce varchar(78) NOT NULL,
    chain_id integer NOT NULL,
    -- In order to be able to order the channels for the `GET channel` request
    created timestamp(2) with time zone NOT NULL,
    -- Do not rename the Primary key constraint (`channels_pkey`)!
    PRIMARY KEY (id)
);

CREATE INDEX idx_channel_created ON channels (created);

CREATE TABLE campaigns (
    id varchar(34) NOT NULL,
    channel_id varchar(66) NOT NULL,
    creator varchar(42) NOT NULL,
    budget bigint NOT NULL,
    validators jsonb NOT NULL,
    title varchar(255) NULL,
    pricing_bounds jsonb DEFAULT '{}' NULL,
    event_submission jsonb DEFAULT '{}' NULL,
    ad_units jsonb DEFAULT '[]' NOT NULL,
    targeting_rules jsonb DEFAULT '[]' NOT NULL,
    created timestamp(2) with time zone NOT NULL,
    active_from timestamp(2) with time zone NULL,
    active_to timestamp(2) with time zone NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT fk_campaigns_channel_id FOREIGN KEY (channel_id) REFERENCES channels (id) ON DELETE RESTRICT ON UPDATE RESTRICT
);

CREATE INDEX idx_campaign_active_to ON campaigns (active_to);

CREATE INDEX idx_campaign_creator ON campaigns (creator);

CREATE INDEX idx_campaign_created ON campaigns (created);

CREATE TABLE spendable (
    spender varchar(42) NOT NULL,
    channel_id varchar(66) NOT NULL,
    total bigint NOT NULL,
    created timestamp(2) with time zone NOT NULL,
    PRIMARY KEY (spender, channel_id),
    CONSTRAINT fk_spendable_channel_id FOREIGN KEY (channel_id) REFERENCES channels (id) ON DELETE RESTRICT ON UPDATE RESTRICT
);

CREATE TABLE validator_messages (
    channel_id varchar(66) NOT NULL,
    "from" varchar(255) NOT NULL,
    msg jsonb NOT NULL,
    received timestamp(2) with time zone NOT NULL,
    CONSTRAINT fk_validator_messages_channel_id FOREIGN KEY (channel_id) REFERENCES channels (id) ON DELETE RESTRICT ON UPDATE RESTRICT
);

CREATE INDEX idx_validator_messages_received ON validator_messages (received);

CREATE INDEX idx_validator_messages_msg_type ON validator_messages ((msg ->> 'type'));

CREATE INDEX idx_validator_messages_msg_state_root ON validator_messages ((msg ->> 'stateRoot'));

CREATE TYPE AccountingSide AS ENUM (
    'Earner',
    'Spender'
);

CREATE TABLE accounting (
    channel_id varchar(66) NOT NULL,
    side AccountingSide NOT NULL,
    "address" varchar(42) NOT NULL,
    amount bigint NOT NULL,
    updated timestamp(2) with time zone DEFAULT NULL,
    created timestamp(2) with time zone NOT NULL,
    -- Do not rename the Primary key constraint (`accounting_pkey`)!
    PRIMARY KEY (channel_id, side, "address"),
    CONSTRAINT fk_accounting_channel_id FOREIGN KEY (channel_id) REFERENCES channels (id) ON DELETE RESTRICT ON UPDATE RESTRICT
);


-- NOTE: The column names and the `AllowedKey` Display/FromStr impls should match!
-- NOTE: The column names `payout_amount` & `payout_count` should match `Metric::column_name()`
CREATE TABLE analytics (
    campaign_id varchar(34) NOT NULL,
    "time" timestamp(2) with time zone NOT NULL,
    ad_unit varchar(255) NOT NULL,
    ad_slot varchar(255) NOT NULL,
    ad_slot_type varchar(255) NOT NULL,
    advertiser varchar(42) NOT NULL,
    publisher varchar(42) NOT NULL,
    hostname varchar(255) NOT NULL,
    country varchar(255) NOT NULL,
    os_name varchar(255) NOT NULL,
    chain_id integer NOT NULL,
    event_type varchar(255) NOT NULL,
    payout_amount bigint NOT NULL DEFAULT 0,
    payout_count integer NOT NULL DEFAULT 0,
    -- Do not rename the Primary key constraint (`analytics_pkey`)!
    PRIMARY KEY (campaign_id, "time", ad_unit, ad_slot, ad_slot_type, advertiser, publisher, hostname, country, os_name, chain_id, event_type)
);
