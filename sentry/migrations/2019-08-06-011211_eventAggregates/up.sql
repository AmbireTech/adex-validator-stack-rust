CREATE TABLE eventAggregates
(
    channel_id    VARCHAR(66)              NOT NULL,
    created       TIMESTAMP WITH TIME ZONE NOT NULL,
    events        JSONB                    NOT NULL,
    
    PRIMARY KEY (channel_id)
);

CREATE INDEX idx_created ON eventAggregates(created);
