use std::sync::Arc;

use axum::{
    extract::{Path, RequestParts},
    middleware::Next,
};
use serde::Deserialize;

use adapter::client::Locked;
use primitives::ChannelId;

use crate::{db::get_channel_by_id, response::ResponseError, Application, Auth};

/// This struct is required because of routes that have more parameters
/// apart from the `ChannelId`
#[derive(Debug, Deserialize)]
struct ChannelParam {
    pub id: ChannelId,
}

pub async fn channel_load<C: Locked + 'static, B>(
    request: axum::http::Request<B>,
    next: Next<B>,
) -> Result<axum::response::Response, ResponseError>
where
    B: Send,
{
    let app = request
        .extensions()
        .get::<Arc<Application<C>>>()
        .expect("Application should always be present")
        .clone();

    // running extractors requires a `RequestParts`
    let mut request_parts = RequestParts::new(request);

    let channel_param = request_parts
        .extract::<Path<ChannelParam>>()
        .await
        .map_err(|_| ResponseError::BadRequest("Bad Channel Id".to_string()))?;

    let channel = get_channel_by_id(&app.pool, &channel_param.id)
        .await?
        .ok_or(ResponseError::NotFound)?;

    let channel_context = app
        .config
        .find_chain_of(channel.token)
        .ok_or_else(|| {
            ResponseError::FailedValidation(
                "Channel token is not whitelisted in this validator".into(),
            )
        })?
        .with_channel(channel);

    // If this is an authenticated call
    // Check if the Channel context (Chain Id) aligns with the Authentication token Chain id
    match request_parts.extensions().get::<Auth>() {
            // If Chain Ids differ, the requester hasn't generated Auth token
            // to access the Channel in it's Chain Id.
            Some(auth) if auth.chain.chain_id != channel_context.chain.chain_id => {
                return Err(ResponseError::Forbidden("Authentication token is generated for different Chain and differs from the Channel's Chain".into()))
            }
            _ => {},
        }

    request_parts.extensions_mut().insert(channel_context);

    let request = request_parts.try_into_request().expect("Body extracted");

    Ok(next.run(request).await)
}
