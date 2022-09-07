wrk.method = "POST"
-- uses the PUBLISHER address
wrk.body   = "{ \"events\": [ {\"type\": \"IMPRESSION\", \"publisher\": \"0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9\", \"adUnit\": \"Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f\", \"adSlot\": \"QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR\"} ] }"
wrk.headers["Content-Type"] = "application/json"
-- uses the DUMMY_AUTH[CREATOR] token
-- wrk.headers["authorization"] = "Bearer AUTH_awesomeCreator:chain_id:1337"