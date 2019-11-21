use crate::db::DbPool;
use crate::ResponseError;
use bb8::RunError;
use hyper::{Body, Request};
use primitives::{Channel, ChannelId};

pub async fn channel_load(
    mut req: Request<Body>,
    (pool, id): (&DbPool, &ChannelId),
) -> Result<Request<Body>, ResponseError> {
    let channel = get_channel(pool, id)
        .await?
        .ok_or(ResponseError::NotFound)?;

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
