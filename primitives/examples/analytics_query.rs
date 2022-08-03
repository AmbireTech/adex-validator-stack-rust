use primitives::{
    analytics::{
        query::{AllowedKey, Time},
        AnalyticsQuery, Metric, OperatingSystem, Timeframe,
    },
    sentry::{DateHour, EventType},
    Address, CampaignId, ChainId, IPFS,
};
use std::str::FromStr;

fn main() {
    // Empty query - default values only
    {
        let empty_query = "";
        let query: AnalyticsQuery = serde_qs::from_str(empty_query).unwrap();

        assert_eq!(100, query.limit);
        assert_eq!(EventType::Impression, query.event_type);
        assert!(matches!(query.metric, Metric::Count));
        assert!(matches!(query.time.timeframe, Timeframe::Day));
    }
    // Query with different metric/chain/eventType
    {
        let query_str = "limit=200&eventType=CLICK&metric=paid&timeframe=month";
        let query: AnalyticsQuery = serde_qs::from_str(query_str).unwrap();

        assert_eq!(200, query.limit);
        assert_eq!(EventType::Click, query.event_type);
        assert!(matches!(query.metric, Metric::Paid));
        assert!(matches!(query.time.timeframe, Timeframe::Month));
    }

    // Query with allowed keys for guest - country, slotType
    {
        let query_str = "country=Bulgaria&adSlotType=legacy_300x100";
        let query: AnalyticsQuery = serde_qs::from_str(query_str).unwrap();

        assert_eq!(Some("Bulgaria".to_string()), query.country);
        assert_eq!(Some("legacy_300x100".to_string()), query.ad_slot_type);
    }

    // Query with all possible fields (publisher/advertiser/admin)
    {
        let query_str = r#"limit=200
            &eventType=CLICK
            &metric=paid
            &segmentBy=country
            &timeframe=week
            &start=420
            &campaignId=0x936da01f9abd4d9d80c702af85c822a8
            &adUnit=QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR
            &adSlot=Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f
            &adSlotType=legacy_300x100
            &avertiser=0xDd589B43793934EF6Ad266067A0d1D4896b0dff0
            &publisher=0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9
            &hostname=localhost
            &country=Bulgaria
            &osName=Windows
            &chains[0]=1&chains[1]=1337
        "#;
        let query: AnalyticsQuery = serde_qs::from_str(query_str).unwrap();

        assert_eq!(query.limit, 200);
        assert_eq!(query.event_type, EventType::Click);
        assert!(matches!(query.metric, Metric::Paid));
        assert_eq!(query.segment_by, Some(AllowedKey::Country));
        assert_eq!(
            query.time,
            Time {
                timeframe: Timeframe::Week,
                start: DateHour::from_ymdh(2021, 12, 31, 22),
                end: None,
            }
        );
        assert_eq!(
            query.campaign_id,
            Some(
                CampaignId::from_str("0x936da01f9abd4d9d80c702af85c822a8")
                    .expect("should be valid")
            )
        );
        assert_eq!(
            query.ad_unit,
            Some(
                IPFS::from_str("QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR")
                    .expect("should be valid")
            )
        );
        assert_eq!(
            query.ad_slot,
            Some(
                IPFS::from_str("Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f")
                    .expect("should be valid")
            )
        );
        assert_eq!(query.ad_slot_type, Some("legacy_300x100".to_string()));
        assert_eq!(
            query.advertiser,
            Some(
                Address::from_str("0xDd589B43793934EF6Ad266067A0d1D4896b0dff0")
                    .expect("should be valid")
            )
        );
        assert_eq!(
            query.publisher,
            Some(
                Address::from_str("0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9")
                    .expect("should be valid")
            )
        );
        assert_eq!(query.hostname, Some("localhost".to_string()));
        assert_eq!(query.country, Some("Bulgaria".to_string()));
        assert_eq!(
            query.os_name,
            Some(OperatingSystem::Whitelisted("Windows".to_string()))
        );
        assert_eq!(query.chains, vec!(ChainId::new(1), ChainId::new(1337)));
    }
}
