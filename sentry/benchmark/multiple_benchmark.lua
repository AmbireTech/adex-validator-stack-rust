-- This script will submit events for 3 campaigns
-- The 3 campaigns can be found in `primitives::test_util`
wrk.method = "POST"
-- uses the PUBLISHER address
wrk.body   = "{ \"events\": [ {\"type\": \"IMPRESSION\", \"publisher\": \"0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9\", \"adUnit\": \"Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f\", \"adSlot\": \"QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR\"} ] }"
wrk.headers["Content-Type"] = "application/json"
-- uses the DUMMY_AUTH[CREATOR] token
-- wrk.headers["authorization"] = "Bearer AUTH_awesomeCreator:chain_id:1337"


init = function(args)
    local r = {}

    -- Campaign 1
    r[1] = wrk.format(nil, "/v5/campaign/0x936da01f9abd4d9d80c702af85c822a8/events") 
    -- Campaign 2
    r[2] = wrk.format(nil, "/v5/campaign/0x127b98248f4e4b73af409d10f62daeaa/events") 
    -- Campaign 3
    r[3] = wrk.format(nil, "/v5/campaign/0xa78f3492481b41a688488a7aa1ff17df/events") 

    req = table.concat(r)
end

request = function()
    return req
 end