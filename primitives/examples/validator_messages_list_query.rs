use primitives::sentry::validator_messages::ValidatorMessagesListQuery;

fn main() {
    // Empty query - default values only
    {
        let empty_query = "";
        let query: ValidatorMessagesListQuery = serde_qs::from_str(empty_query).unwrap();

        assert_eq!(None, query.limit);
    }
    // Query with set limit
    {
        let query_str = "limit=200";
        let query: ValidatorMessagesListQuery = serde_qs::from_str(query_str).unwrap();

        assert_eq!(Some(200), query.limit);
    }
}
