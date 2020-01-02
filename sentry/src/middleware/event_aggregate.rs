use crate::{Application, ResponseError, RouteParams};
use hyper::{Body, Request};
use primitives::adapter::Adapter;
use primitives::sentry::EarnerAddress;

/// channel_load & channel_if_exist
pub async fn earner_load<A: Adapter>(
    mut req: Request<Body>,
    _app: &Application<A>,
) -> Result<Request<Body>, ResponseError> {
    let earner = req
        .extensions()
        .get::<RouteParams>()
        .ok_or_else(|| ResponseError::BadRequest("Route params not found".to_string()))?
        .get(1)
        .ok_or_else(|| ResponseError::BadRequest("No earner param".to_string()))?;

    let earner_option: Option<EarnerAddress> = if earner.is_empty() {
        None
    } else {
        Some(
            serde_json::from_str(&earner)
                .map_err(|_| ResponseError::BadRequest("Invalid earner param".to_string()))?,
        )
    };

    req.extensions_mut().insert(earner_option);

    Ok(req)
}
