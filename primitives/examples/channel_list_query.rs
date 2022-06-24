use primitives::{
    sentry::channel_list::ChannelListQuery,
    test_util::{IDS, LEADER},
    ChainId,
};

fn main() {
    // An empty query
    {
        let empty = "";
        let empty_expected = ChannelListQuery {
            page: 0,
            validator: None,
            chains: vec![],
        };

        assert_eq!(empty_expected, serde_qs::from_str(empty).unwrap());
    }

    // Query with `page`
    {
        let only_page = "page=14";
        let only_page_expected = ChannelListQuery {
            page: 14,
            validator: None,
            chains: vec![],
        };

        assert_eq!(only_page_expected, serde_qs::from_str(only_page).unwrap());
    }

    // Query with `validator`
    {
        let only_validator = "validator=0x80690751969B234697e9059e04ed72195c3507fa";
        let only_validator_expected = ChannelListQuery {
            page: 0,
            validator: Some(IDS[&LEADER]),
            chains: vec![],
        };

        assert_eq!(
            only_validator_expected,
            serde_qs::from_str(only_validator).unwrap()
        );
    }

    // Query with `chains`
    {
        let chains_query = "chains[]=1&chains[]=1337";
        let chains_expected = ChannelListQuery {
            page: 0,
            validator: None,
            chains: vec![ChainId::new(1), ChainId::new(1337)],
        };

        assert_eq!(chains_expected, serde_qs::from_str(chains_query).unwrap());
    }

    // Query with all parameters
    {
        let all_query =
            "page=14&validator=0x80690751969B234697e9059e04ed72195c3507fa&chains[]=1&chains[]=1337";
        let all_expected = ChannelListQuery {
            page: 14,
            validator: Some(IDS[&LEADER]),
            chains: vec![ChainId::new(1), ChainId::new(1337)],
        };

        assert_eq!(all_expected, serde_qs::from_str(all_query).unwrap());
    }
}
