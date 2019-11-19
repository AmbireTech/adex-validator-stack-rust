use crate::db::DbPool;
use crate::ResponseError;
use bb8::RunError;
use bb8_postgres::tokio_postgres::{types::Json, Row};
use hyper::{Body, Request};
use primitives::{Channel, ChannelId, ChannelSpec};

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
                match connection.prepare("SELECT channel_id, creator, deposit_asset, deposit_amount, valid_until, spec FROM channels WHERE channel_id = $1 LIMIT 1").await {
                    Ok(select) => match connection.query(&select, &[&id]).await {
                        Ok(results) => Ok((results.get(0).map(|row| channel_map(&row)), connection)),
                        Err(e) => Err((e, connection)),
                    },
                    Err(e) => Err((e, connection)),
                }
            }
        })
        .await
}

fn channel_map(row: &Row) -> Channel {
    Channel {
        id: row.get("channel_id"),
        creator: row.get("creator"),
        deposit_asset: row.get("deposit_asset"),
        deposit_amount: row.get("deposit_amount"),
        valid_until: row.get("valid_until"),
        spec: row.get::<_, Json<ChannelSpec>>("spec").0,
    }
}
