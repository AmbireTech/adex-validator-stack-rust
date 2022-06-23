use chrono::{TimeZone, Utc};
use primitives::{
    sentry::campaign_list::{CampaignListQuery, ValidatorParam},
    test_util::{ADVERTISER, FOLLOWER, LEADER},
};

fn main() {
    // Empty query - default values only
    {
        let empty_query = "";
        let query: CampaignListQuery = serde_qs::from_str(empty_query).unwrap();

        assert_eq!(0, query.page);
        assert!(
            Utc::now() >= query.active_to_ge,
            "By default `activeTo` is set to `Utc::now()`"
        );
        assert!(query.creator.is_none());
        assert!(query.validator.is_none());
    }

    // Query with `activeTo` only
    {
        let active_to_query = "activeTo=1624192200";
        let active_to = CampaignListQuery {
            page: 0,
            active_to_ge: Utc.ymd(2021, 6, 20).and_hms(12, 30, 0),
            creator: None,
            validator: None,
        };

        assert_eq!(active_to, serde_qs::from_str(active_to_query).unwrap());
    }

    // Query with `page` & `activeTo`
    {
        let with_page_query = "page=14&activeTo=1624192200";
        let with_page = CampaignListQuery {
            page: 14,
            active_to_ge: Utc.ymd(2021, 6, 20).and_hms(12, 30, 0),
            creator: None,
            validator: None,
        };

        assert_eq!(with_page, serde_qs::from_str(with_page_query).unwrap());
    }

    // Query with `creator`
    {
        let with_creator_query =
            "activeTo=1624192200&creator=0xDd589B43793934EF6Ad266067A0d1D4896b0dff0";

        let with_creator = CampaignListQuery {
            page: 0,
            active_to_ge: Utc.ymd(2021, 6, 20).and_hms(12, 30, 0),
            creator: Some(*ADVERTISER),
            validator: None,
        };

        assert_eq!(
            with_creator,
            serde_qs::from_str(with_creator_query).unwrap()
        );
    }

    // Query with `validator`
    {
        let with_creator_validator_query = "page=0&activeTo=1624192200&creator=0xDd589B43793934EF6Ad266067A0d1D4896b0dff0&validator=0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7";
        let with_creator_validator = CampaignListQuery {
            page: 0,
            active_to_ge: Utc.ymd(2021, 6, 20).and_hms(12, 30, 0),
            creator: Some(*ADVERTISER),
            validator: Some(ValidatorParam::Validator((*FOLLOWER).into())),
        };

        assert_eq!(
            with_creator_validator,
            serde_qs::from_str(with_creator_validator_query).unwrap()
        );
    }

    // Query with `leader`
    {
        let with_leader_query = "page=0&activeTo=1624192200&creator=0xDd589B43793934EF6Ad266067A0d1D4896b0dff0&leader=0x80690751969B234697e9059e04ed72195c3507fa";

        let with_leader = CampaignListQuery {
            page: 0,
            active_to_ge: Utc.ymd(2021, 6, 20).and_hms(12, 30, 0),
            creator: Some(*ADVERTISER),
            validator: Some(ValidatorParam::Leader((*LEADER).into())),
        };

        assert_eq!(with_leader, serde_qs::from_str(with_leader_query).unwrap());
    }
}
