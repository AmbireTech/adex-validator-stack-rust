use primitives::sentry::LastApprovedQuery;

fn main() {
    // An empty query - no heartbeats will be included in the response (default).
    {
        // This is treated the same as `withHeartbeat=false` in the route.
        let empty = "";
        let empty_expected = LastApprovedQuery {
            with_heartbeat: None,
        };

        assert_eq!(empty_expected, serde_qs::from_str(empty).unwrap());
    }

    // Query with `with_heartbeat` parameter - latest 2 Heartbeats from
    // each Channel Validator will be returned in the response.
    {
        let with_heartbeat = "withHeartbeat=true";
        let with_heartbeat_expected = LastApprovedQuery {
            with_heartbeat: Some(true),
        };

        assert_eq!(
            with_heartbeat_expected,
            serde_qs::from_str(with_heartbeat).unwrap()
        );
    }
}
