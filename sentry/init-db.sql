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

CREATE TABLE eventAggregates
(
    id --@TODO autoincrement
    channel_id  VARCHAR(66)              NOT NULL, --@TODO change to reference
    created TIMESTAMP WITH TIME ZONE NOT NULL,
    events  JSONB                    NOT NULL,
    eventPayouts JSONB NOT NULL,
);

CREATE TABLE validatorMessages
(

);

{ "_id" : ObjectId("5cf801c554d235c376f8cb5f"), 
"channelId" : "awesomeTestChannel", 
"created" : ISODate("2019-06-05T17:54:13.660Z"), 
"events" : { "IMPRESSION" : { "eventCounts" : { "myAwesomePublisher" : "3", "anotherPublisher" : "2" }, 
"eventPayouts" : { "myAwesomePublisher" : "3", "anotherPublisher" : "2" } } } }
