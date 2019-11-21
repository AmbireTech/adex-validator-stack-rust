use crate::db::DbPool;
use crate::{Application, ResponseError, RouteParams};
use bb8::RunError;
use hex::FromHex;
use hyper::{Body, Request};
use primitives::adapter::Adapter;
use primitives::{Channel, ChannelId};

pub async fn channel_load<A: Adapter>(
    mut req: Request<Body>,
    app: &Application<A>,
) -> Result<Request<Body>, ResponseError> {
    let id = req
        .extensions()
        .get::<RouteParams>()
        .ok_or_else(|| ResponseError::BadRequest("Route params not found".to_string()))?
        .get(0)
        .ok_or_else(|| ResponseError::BadRequest("No id".to_string()))?;

    let channel_id = ChannelId::from_hex(id)
        .map_err(|_| ResponseError::BadRequest("Wrong Channel Id".to_string()))?;
    let channel = get_channel(&app.pool, &channel_id)
        .await?
        .ok_or_else(|| ResponseError::NotFound)?;

    req.extensions_mut().insert(channel);

    Ok(req)
}

// @TODO: Maybe move this to more generic place?
pub async fn get_channel(
    pool: &DbPool,
    id: &ChannelId,
) -> Result<Option<Channel>, RunError<bb8_postgres::tokio_postgres::Error>> {
    pool
        .run(move |connection| {
            async move {
                match connection.prepare("SELECT id, creator, deposit_asset, deposit_amount, valid_until, spec FROM channels WHERE id = $1 LIMIT 1").await {
                    Ok(select) => match connection.query(&select, &[&id]).await {
                        Ok(results) => Ok(( results.get(0).map(Channel::from) , connection)),
                        Err(e) => Err((e, connection)),
                    },
                    Err(e) => Err((e, connection)),
                }
            }
        })
        .await
}
