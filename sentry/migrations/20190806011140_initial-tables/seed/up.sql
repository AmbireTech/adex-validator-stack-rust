INSERT INTO
channels (id, creator, deposit_asset, deposit_amount, valid_until, targeting_rules, spec)
VALUES
(
    '0x061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088',
    '0x033ed90e0fec3f3ea1c9b005c724d704501e0196',
    '0x89d24A6b4CcB1B6fAA2625fE562bDD9a23260359',
    '1000',
    to_timestamp(4102444800),
    '[]',
    '{"targeting_rules": [], "minPerImpression":"1","maxPerImpression":"10","created":1564383600000,"pricingBounds":{"CLICK":{"min":"0","max":"0"}},"withdrawPeriodStart":4073414400000,"validators":[{"id":"0xce07CbB7e054514D590a0262C93070D838bFBA2e","url":"http://localhost:8005","fee":"100"},{"id":"0xC91763D7F14ac5c5dDfBCD012e0D2A61ab9bDED3","url":"http://localhost:8006","fee":"100"}]}'
);
