use crate::{
    db::{get_channel_by_id, get_channel_by_id_and_validator},
    middleware::Middleware,
    Application, Auth, ResponseError, RouteParams,
};
use adapter::client::Locked;
use futures::future::{BoxFuture, FutureExt};
use hex::FromHex;
use hyper::{Body, Request};
use primitives::{ChannelId, ValidatorId};

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
        channel_load(request, application).await
    }
}

/// channel_load & channel_if_exist
fn channel_load<C: Locked>(
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

        let channel_context = app.config.find_chain_token(channel.token).ok_or(ResponseError::FailedValidation("Channel token is not whitelisted in this validator".into()))?.with_channel(channel);

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

#[derive(Debug)]
#[deprecated = "No longer needed for V4"]
pub struct ChannelIfActive;

#[async_trait]
impl<C: Locked + 'static> Middleware<C> for ChannelIfActive {
    async fn call<'a>(
        &self,
        request: Request<Body>,
        application: &'a Application<C>,
    ) -> Result<Request<Body>, ResponseError> {
        channel_if_active(request, application).await
    }
}

fn channel_if_active<C: Locked>(
    mut req: Request<Body>,
    app: &Application<C>,
) -> BoxFuture<'_, Result<Request<Body>, ResponseError>> {
    async move {
        let route_params = req
            .extensions()
            .get::<RouteParams>()
            .ok_or_else(|| ResponseError::BadRequest("Route params not found".to_string()))?;

        let id = route_params
            .get(0)
            .ok_or_else(|| ResponseError::BadRequest("No id".to_string()))?;

        let channel_id = ChannelId::from_hex(id)
            .map_err(|_| ResponseError::BadRequest("Wrong Channel Id".to_string()))?;

        let validator_id = route_params
            .get(1)
            .ok_or_else(|| ResponseError::BadRequest("No Validator Id".to_string()))?;
        let validator_id = ValidatorId::try_from(&validator_id)
            .map_err(|_| ResponseError::BadRequest("Wrong Validator Id".to_string()))?;

        let channel = get_channel_by_id_and_validator(&app.pool, channel_id, validator_id)
            .await?
            .ok_or(ResponseError::NotFound)?;

        req.extensions_mut().insert(channel);

        Ok(req)
    }
    .boxed()
}

#[derive(Debug)]
pub struct GetChannelId;

#[async_trait]
impl<C: Locked + 'static> Middleware<C> for GetChannelId {
    async fn call<'a>(
        &self,
        request: Request<Body>,
        application: &'a Application<C>,
    ) -> Result<Request<Body>, ResponseError> {
        get_channel_id(request, application).await
    }
}

fn get_channel_id<C: Locked>(
    mut req: Request<Body>,
    _: &Application<C>,
) -> BoxFuture<'_, Result<Request<Body>, ResponseError>> {
    async move {
        match req.extensions().get::<RouteParams>() {
            Some(param) => {
                let id = param.get(0).expect("should have channel id");
                let channel_id = ChannelId::from_hex(id)
                    .map_err(|_| ResponseError::BadRequest("Invalid Channel Id".to_string()))?;
                req.extensions_mut().insert(channel_id);

                Ok(req)
            }
            None => Ok(req),
        }
    }
    .boxed()
}
