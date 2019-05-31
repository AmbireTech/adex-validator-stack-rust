use futures::compat::Future01CompatExt;
use futures::future::FutureExt;
use futures_legacy::Future as OldFuture;
use futures_legacy::future::IntoFuture;
use futures_legacy::Stream as OldStream;
use tokio_postgres::types::Json;

use domain::{Channel, ChannelId, ChannelListParams, ChannelRepository, ChannelSpec, RepositoryFuture};
use try_future::try_future;

use crate::infrastructure::field::asset::AssetPg;
use crate::infrastructure::field::bignum::BigNumPg;
use crate::infrastructure::persistence::DbPool;
use crate::infrastructure::persistence::postgres::PostgresPersistenceError;

pub struct PostgresChannelRepository {
    db_pool: DbPool,
}

impl PostgresChannelRepository {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }
}

impl ChannelRepository for PostgresChannelRepository {
    // @TODO: use params to filter channels accordingly
    fn list(&self, _params: &ChannelListParams) -> RepositoryFuture<Vec<Channel>> {
        let fut = self.db_pool
            .run(move |mut conn| {
                conn.prepare("SELECT channel_id, creator, deposit_asset, deposit_amount, valid_until, spec FROM channels")
                    .then(move |res| match res {
                        Ok(stmt) => {
                            conn
                                .query(&stmt, &[])
                                .collect()
                                .into_future()
                                .then(|res| match res {
                                    Ok(rows) => Ok((rows, conn)),
                                    Err(err) => Err((err, conn)),
                                })
                                .into()
                        }
                        Err(err) => try_future!(Err((err, conn))),
                    })
                    .and_then(|(rows, conn)| {
                        use std::convert::TryInto;

                        // @TODO: Add the ChannelSpecs hydration
                        let channels = rows.iter().map(|row| {
                            let spec: ChannelSpec = row.get::<_, Json<ChannelSpec>>("spec").0;
                            Channel {
                                // @TODO: Fix this mess by creating a FromSql & ToSql
                                id: row.get::<_, &str>("channel_id").try_into().unwrap(),
                                creator: row.get("creator"),
                                deposit_asset: row.get::<_, AssetPg>("deposit_asset").into(),
                                deposit_amount: row.get::<_, BigNumPg>("deposit_amount").into(),
                                valid_until: row.get("valid_until"),
                                spec,
                            }
                        }).collect();

                        Ok((channels, conn))
                    })
            })
            .map_err(|err| PostgresPersistenceError::from(err).into());

        fut.compat().boxed()
    }

    fn save(&self, _channel: Channel) -> RepositoryFuture<()> {
        unimplemented!("save() for Postgres still needs to be implemented")
    }

    fn find(&self, _channel_id: &ChannelId) -> RepositoryFuture<Option<Channel>> {
        unimplemented!("find() for Postgres still needs to be implemented")
    }
}