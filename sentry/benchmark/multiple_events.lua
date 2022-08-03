-- Multiple events to multiple campaigns
--
-- This script will submit events for 3 campaigns
-- The 3 campaigns can be found in `primitives::test_util`
-- Each requests consist of 2 events - IMPRESSION & CLICK
-- and each event has differnet publihser (PUBLIHSER & PUBLISHER_2) as well as different AdUnit & AdSlot
-- The same events are used for all campaigns.
wrk.method = "POST"
-- uses the PUBLISHER (for IMPRESSION) & PUBLISHER_2 (for CLICK) address
wrk.body = "{ \"events\": [ {\"type\": \"IMPRESSION\", \"publisher\": \"0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9\", \"adUnit\": \"Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f\", \"adSlot\": \"QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR\"} ] }"
wrk.headers["Content-Type"] = "application/json"
-- uses the DUMMY_AUTH[CREATOR] token
-- wrk.headers["authorization"] = "Bearer AUTH_awesomeCreator:chain_id:1337"


init = function(args)
    local r = {}

    -- with 2 different publishers (PUBLISHER, PUBLISHER_2), different AdUnits & different AdSlots
    local two_events_body = "{ \"events\": [ {\"type\": \"IMPRESSION\", \"publisher\": \"0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9\", \"adUnit\": \"Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f\", \"adSlot\": \"QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR\"}, {\"type\": \"CLICK\", \"publisher\": \"0x0e880972A4b216906F05D67EeaaF55d16B5EE4F1\", \"adUnit\": \"QmQnu8zrHsuVvnTJsEgDHYA8c1MmRL7YLiMD8uzDUJKcNq\", \"adSlot\": \"QmYYBULc9QDEaDr8HAXvVWHDmFfL2GvyumYRr1g4ERBC96\"} ] }"

    -- Campaign 1
    r[1] = wrk.format(nil, "/v5/campaign/0x936da01f9abd4d9d80c702af85c822a8/events", nil, two_events_body)
    -- Campaign 2
    r[2] = wrk.format(nil, "/v5/campaign/0x127b98248f4e4b73af409d10f62daeaa/events", nil, two_events_body)
    -- Campaign 3
    r[3] = wrk.format(nil, "/v5/campaign/0xa78f3492481b41a688488a7aa1ff17df/events", nil, two_events_body)

    req = table.concat(r)
end

request = function()
    return req
 end