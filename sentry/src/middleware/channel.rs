use std::sync::Arc;

use crate::{
    db::get_channel_by_id, middleware::Middleware, response::ResponseError,
    routes::routers::RouteParams, Application, Auth,
};
use adapter::client::Locked;
use axum::{
    extract::{Path, RequestParts},
    middleware::Next,
};
use futures::future::{BoxFuture, FutureExt};
use hex::FromHex;
use hyper::{Body, Request};
use primitives::ChannelId;

use async_trait::async_trait;

#[derive(Debug)]
pub struct ChannelLoad;

#[async_trait]
impl<C: Locked + 'static> Middleware<C> for ChannelLoad {
    async fn call<'a>(
        &self,
        request: Request<Body>,
        application: &'a Application<C>,
    ) -> Result<Request<Body>, ResponseError> {
        channel_load_old(request, application).await
    }
}

/// channel_load & channel_if_exist
fn channel_load_old<C: Locked>(
    mut req: Request<Body>,
    app: &Application<C>,
) -> BoxFuture<'_, Result<Request<Body>, ResponseError>> {
    async move {
        let id = req
            .extensions()
            .get::<RouteParams>()
            .ok_or_else(|| ResponseError::BadRequest("Route params not found".to_string()))?
            .get(0)
            .ok_or_else(|| ResponseError::BadRequest("No id".to_string()))?;

        let channel_id = ChannelId::from_hex(id)
            .map_err(|_| ResponseError::BadRequest("Wrong Channel Id".to_string()))?;

        let channel = get_channel_by_id(&app.pool, &channel_id)
            .await?
            .ok_or(ResponseError::NotFound)?;

        let channel_context = app.config.find_chain_of(channel.token).ok_or_else(|| ResponseError::FailedValidation("Channel token is not whitelisted in this validator".into()))?.with_channel(channel);

        // If this is an authenticated call
        // Check if the Channel context (Chain Id) aligns with the Authentication token Chain id
        match req.extensions().get::<Auth>() {
            // If Chain Ids differ, the requester hasn't generated Auth token
            // to access the Channel in it's Chain Id.
            Some(auth) if auth.chain.chain_id != channel_context.chain.chain_id => {
                return Err(ResponseError::Forbidden("Authentication token is generated for different Chain and differs from the Channel's Chain".into()))
            }
            _ => {},
        }

        req.extensions_mut().insert(channel_context);

        Ok(req)
    }
    .boxed()
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

    let channel_id = request_parts
        .extract::<Path<ChannelId>>()
        .await
        .map_err(|_| ResponseError::BadRequest("Bad Channel Id".to_string()))?;

    let channel = get_channel_by_id(&app.pool, &channel_id)
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
