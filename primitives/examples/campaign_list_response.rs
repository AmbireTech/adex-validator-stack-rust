use primitives::sentry::campaign_list::CampaignListResponse;
use serde_json::{from_value, json};

fn main() {
    let json = json!({
      "campaigns": [
        {
          "id": "0x936da01f9abd4d9d80c702af85c822a8",
          "channel": {
            "leader": "0x80690751969B234697e9059e04ed72195c3507fa",
            "follower": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
            "guardian": "0xe061E1EB461EaBE512759aa18A201B20Fe90631D",
            "token": "0x2BCaf6968aEC8A3b5126FBfAb5Fd419da6E8AD8E",
            "nonce": "0"
          },
          "creator": "0xDd589B43793934EF6Ad266067A0d1D4896b0dff0",
          "budget": "15000000000",
          "validators": [
            {
              "id": "0x80690751969B234697e9059e04ed72195c3507fa",
              "fee": "500000000",
              "url": "http://localhost:8005/"
            },
            {
              "id": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
              "fee": "400000000",
              "url": "http://localhost:8006/"
            }
          ],
          "title": "Dummy Campaign",
          "pricingBounds": {
            "CLICK": {
              "min": "6000",
              "max": "10000"
            },
            "IMPRESSION": {
              "min": "4000",
              "max": "5000"
            }
          },
          "eventSubmission": {
            "allow": []
          },
          "adUnits": [
            {
              "ipfs": "Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f",
              "type": "legacy_250x250",
              "mediaUrl": "ipfs://QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR",
              "mediaMime": "image/jpeg",
              "targetUrl": "https://www.adex.network/?stremio-test-banner-1",
              "owner": "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9",
              "created": 1564390800000_u64,
              "title": "Dummy AdUnit 1",
              "description": "Dummy AdUnit description 1",
              "archived": false
            },
            {
              "ipfs": "QmVhRDGXoM3Fg3HZD5xwMuxtb9ZErwC8wHt8CjsfxaiUbZ",
              "type": "legacy_250x250",
              "mediaUrl": "ipfs://QmQB7uz7Gxfy7wqAnrnBcZFaVJLos8J9gn8mRcHQU6dAi1",
              "mediaMime": "image/jpeg",
              "targetUrl": "https://www.adex.network/?adex-campaign=true&pub=stremio",
              "owner": "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9",
              "created": 1564390800000_u64,
              "title": "Dummy AdUnit 2",
              "description": "Dummy AdUnit description 2",
              "archived": false
            }
          ],
          "targetingRules": [],
          "created": 1612162800000_u64,
          "activeTo": 4073414400000_u64
        },
        {
          "id": "0x127b98248f4e4b73af409d10f62daeaa",
          "channel": {
            "leader": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
            "follower": "0x80690751969B234697e9059e04ed72195c3507fa",
            "guardian": "0x79D358a3194d737880B3eFD94ADccD246af9F535",
            "token": "0x2BCaf6968aEC8A3b5126FBfAb5Fd419da6E8AD8E",
            "nonce": "0"
          },
          "creator": "0xDd589B43793934EF6Ad266067A0d1D4896b0dff0",
          "budget": "2000000000",
          "validators": [
            {
              "id": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
              "fee": "10000000",
              "url": "http://localhost:8006/"
            },
            {
              "id": "0x80690751969B234697e9059e04ed72195c3507fa",
              "fee": "5000000",
              "url": "http://localhost:8005/"
            }
          ],
          "title": "Dummy Campaign 2 in Chain #1337",
          "pricingBounds": {
            "CLICK": {
              "min": "300000",
              "max": "500000"
            },
            "IMPRESSION": {
              "min": "100000",
              "max": "200000"
            }
          },
          "eventSubmission": {
            "allow": []
          },
          "adUnits": [
            {
              "ipfs": "Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f",
              "type": "legacy_250x250",
              "mediaUrl": "ipfs://QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR",
              "mediaMime": "image/jpeg",
              "targetUrl": "https://www.adex.network/?stremio-test-banner-1",
              "owner": "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9",
              "created": 1564390800000_u64,
              "title": "Dummy AdUnit 1",
              "description": "Dummy AdUnit description 1",
              "archived": false
            },
            {
              "ipfs": "QmVhRDGXoM3Fg3HZD5xwMuxtb9ZErwC8wHt8CjsfxaiUbZ",
              "type": "legacy_250x250",
              "mediaUrl": "ipfs://QmQB7uz7Gxfy7wqAnrnBcZFaVJLos8J9gn8mRcHQU6dAi1",
              "mediaMime": "image/jpeg",
              "targetUrl": "https://www.adex.network/?adex-campaign=true&pub=stremio",
              "owner": "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9",
              "created": 1564390800000_u64,
              "title": "Dummy AdUnit 2",
              "description": "Dummy AdUnit description 2",
              "archived": false
            }
          ],
          "targetingRules": [],
          "created": 1612162800000_u64,
          "activeFrom": 1656280800000_u64,
          "activeTo": 4073414400000_u64
        },
        {
          "id": "0xa78f3492481b41a688488a7aa1ff17df",
          "channel": {
            "leader": "0x80690751969B234697e9059e04ed72195c3507fa",
            "follower": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
            "guardian": "0x79D358a3194d737880B3eFD94ADccD246af9F535",
            "token": "0x12a28f2bfBFfDf5842657235cC058242f40fDEa6",
            "nonce": "1"
          },
          "creator": "0x541b401362Ea1D489D322579552B099e801F3632",
          "budget": "2000000000",
          "validators": [
            {
              "id": "0x80690751969B234697e9059e04ed72195c3507fa",
              "fee": "200000000",
              "url": "http://localhost:8005/"
            },
            {
              "id": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
              "fee": "175000000",
              "url": "http://localhost:8006/"
            }
          ],
          "title": "Dummy Campaign 3 in Chain #1",
          "pricingBounds": {
            "CLICK": {
              "min": "3500",
              "max": "6500"
            },
            "IMPRESSION": {
              "min": "1500",
              "max": "2500"
            }
          },
          "eventSubmission": {
            "allow": []
          },
          "adUnits": [
            {
              "ipfs": "QmYwcpMjmqJfo9ot1jGe9rfXsszFV1WbEA59QS7dEVHfJi",
              "type": "legacy_250x250",
              "mediaUrl": "ipfs://QmQB7uz7Gxfy7wqAnrnBcZFaVJLos8J9gn8mRcHQU6dAi1",
              "mediaMime": "image/jpeg",
              "targetUrl": "https://www.adex.network/?adex-campaign=true",
              "owner": "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9",
              "created": 1564390800000_u64,
              "title": "Dummy AdUnit 3",
              "description": "Dummy AdUnit description 3",
              "archived": false
            },
            {
              "ipfs": "QmTAF3FsFDS7Ru8WChoD9ofiHTH8gAQfR4mYSnwxqTDpJH",
              "type": "legacy_250x250",
              "mediaUrl": "ipfs://QmQAcfBJpDDuH99A4p3pFtUmQwamS8UYStP5HxHC7bgYXY",
              "mediaMime": "image/jpeg",
              "targetUrl": "https://adex.network",
              "owner": "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9",
              "created": 1564390800000_u64,
              "title": "Dummy AdUnit 4",
              "description": "Dummy AdUnit description 4",
              "archived": false
            }
          ],
          "targetingRules": [],
          "created": 1612162800000_u64,
          "activeTo": 4073414400000_u64
        }
      ],
      "totalPages": 1,
      "page": 0
    });

    assert!(from_value::<CampaignListResponse>(json).is_ok());
}
