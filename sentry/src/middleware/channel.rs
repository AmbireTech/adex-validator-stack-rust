use crate::db::{get_channel_by_id, get_channel_by_id_and_validator};
use crate::{Application, ResponseError, RouteParams};
use futures::future::{BoxFuture, FutureExt};
use hex::FromHex;
use hyper::{Body, Request};
use primitives::adapter::Adapter;
use primitives::{ChannelId, ValidatorId};
use std::convert::TryFrom;

/// channel_load & channel_if_exist
pub fn channel_load<'a, A: Adapter + 'static>(
    mut req: Request<Body>,
    app: &'a Application<A>,
) -> BoxFuture<'a, Result<Request<Body>, ResponseError>> {
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

        req.extensions_mut().insert(channel);

        Ok(req)
    }
    .boxed()
}

pub fn channel_if_active<'a, A: Adapter + 'static>(
    mut req: Request<Body>,
    app: &'a Application<A>,
) -> BoxFuture<'a, Result<Request<Body>, ResponseError>> {
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

        let channel = get_channel_by_id_and_validator(&app.pool, &channel_id, &validator_id)
            .await?
            .ok_or(ResponseError::NotFound)?;

        req.extensions_mut().insert(channel);

        Ok(req)
    }
    .boxed()
}

pub fn get_channel_id<'a, A: Adapter + 'static>(
    mut req: Request<Body>,
    _: &'a Application<A>,
) -> BoxFuture<'a, Result<Request<Body>, ResponseError>> {
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
