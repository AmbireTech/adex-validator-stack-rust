//! `/v5/analytics` routes
//!

use std::collections::HashSet;

use crate::{db::analytics::get_analytics, success_response, Application, Auth, ResponseError};
use adapter::client::Locked;
use hyper::{Body, Request, Response};
use primitives::analytics::{
    query::{AllowedKey, ALLOWED_KEYS},
    AnalyticsQuery, AuthenticateAs,
};

/// `GET /v5/analytics` request
/// with query parameters: [`primitives::analytics::AnalyticsQuery`].
pub async fn analytics<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
    request_allowed: Option<HashSet<AllowedKey>>,
    authenticate_as: Option<AuthenticateAs>,
) -> Result<Response<Body>, ResponseError> {
    let mut query = serde_qs::from_str::<AnalyticsQuery>(req.uri().query().unwrap_or(""))?;
    // If we have a route that requires authentication the Chain will be extracted
    // from the sentry's authentication, which guarantees the value will exist
    // This will also override a query parameter for the chain if it is provided
    if let Some(auth) = req.extensions().get::<Auth>() {
        query.chains = vec![auth.chain.chain_id]
    }

    let applied_limit = query.limit.min(app.config.analytics_find_limit);

    let route_allowed_keys: HashSet<AllowedKey> =
        request_allowed.unwrap_or_else(|| ALLOWED_KEYS.clone());

    if let Some(segment_by) = query.segment_by {
        if !route_allowed_keys.contains(&segment_by) {
            return Err(ResponseError::Forbidden(format!(
                "Disallowed segmentBy `{}`",
                segment_by.to_camelCase()
            )));
        }
    }

    // for all `ALLOWED_KEYS` check if we have a value passed and if so,
    // cross check it with the allowed keys for the route
    let disallowed_key = ALLOWED_KEYS.iter().find(|allowed| {
        let in_query = query.get_key(**allowed).is_some();

        // is there a value for this key in the query
        // but not allowed for this route
        // return the key
        in_query && !route_allowed_keys.contains(*allowed)
    });

    // Return a error if value passed in the query is not allowed for this route
    if let Some(disallowed_key) = disallowed_key {
        return Err(ResponseError::Forbidden(format!(
            "Disallowed query key `{}`",
            disallowed_key.to_camelCase()
        )));
    }

    let analytics_maxtime = std::time::Duration::from_millis(app.config.analytics_maxtime.into());

    let analytics = match tokio::time::timeout(
        analytics_maxtime,
        get_analytics(
            &app.pool,
            query.clone(),
            route_allowed_keys,
            authenticate_as,
            applied_limit,
        ),
    )
    .await
    {
        Ok(Ok(analytics)) => analytics,
        // Error getting the analytics
        Ok(Err(err)) => return Err(err.into()),
        // Timeout error
        Err(_elapsed) => {
            return Err(ResponseError::BadRequest(
                "Timeout when fetching analytics data".into(),
            ))
        }
    };

    Ok(success_response(serde_json::to_string(&analytics)?))
}
