use primitives::postgres::field::{BigNumPg, ChannelIdPg};
use crate::db::DbPool;
use bb8_postgres::tokio_postgres::{types::Json, Row};
use hyper::{Body, Request};
use primitives::channel::ChannelId;
use primitives::{Channel, ChannelSpec};
use std::error;

pub async fn channel_load(
    mut req: Request<Body>,
    (pool, id): (DbPool, &ChannelId),
) -> Result<Request<Body>, Box<dyn error::Error>> {
    let channel_id = hex::encode(id);

    let channel = pool
        .run(move |connection| {
            async move {
                match connection.prepare("SELECT channel_id, creator, deposit_asset, deposit_amount, valid_until, spec FROM channels WHERE channel_id = $1").await {
                    Ok(select) => match connection.query_one(&select, &[&channel_id]).await {
                            Ok(row) => Ok((channel_map(&row), connection)),
                            Err(e) => Err((e, connection)),
                        },
                    Err(e) => Err((e, connection)),
                }
            }
        })
        .await?;

    req.extensions_mut().insert(channel);

    Ok(req)
}

fn channel_map(row: &Row) -> Channel {
    Channel {
        id: row.get::<_, ChannelIdPg>("channel_id").into(),
        creator: row.get("creator"),
        deposit_asset: row.get("deposit_asset"),
        deposit_amount: row.get::<_, BigNumPg>("deposit_amount").into(),
        valid_until: row.get("valid_until"),
        spec: row.get::<_, Json<ChannelSpec>>("spec").0,
    }
}
