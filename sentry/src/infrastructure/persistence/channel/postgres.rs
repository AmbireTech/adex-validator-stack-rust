use futures::compat::Future01CompatExt;
use futures::future::FutureExt;
use futures_legacy::Future as OldFuture;
use futures_legacy::future::IntoFuture;
use futures_legacy::Stream as OldStream;
use try_future::try_future;

use crate::domain::{Channel, ChannelRepository, RepositoryFuture};
use crate::infrastructure::persistence::DbPool;

pub struct PostgresChannelRepository {
    db_pool: DbPool,
}

impl PostgresChannelRepository {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }
}

impl ChannelRepository for PostgresChannelRepository {
    fn list(&self) -> RepositoryFuture<Vec<Channel>> {
        let fut = self.db_pool
            .run(move |mut conn| {
                conn.prepare("SELECT channel_id, creator, deposit_asset, deposit_amount, valid_until FROM channels")
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
                        // @TODO: Add the ChannelSpecs hydration
                        let channels = rows.iter().map(|row| {
                            Channel {
                                id: row.get("channel_id"),
                                creator: row.get("creator"),
                                deposit_asset: row.get("deposit_asset"),
                                deposit_amount: row.get("deposit_amount"),
                                valid_until: row.get("valid_until"),
                            }
                        }).collect();

                        Ok((channels, conn))
                    })
            })
            .map_err(|err| err.into());

        fut.compat().boxed()
    }

    fn save(&self, _channel: Channel) -> RepositoryFuture<()> {
        unimplemented!("save() for Postgres still needs to be implemented")
    }

    fn find(&self, _channel_id: &String) -> RepositoryFuture<Option<Channel>> {
        unimplemented!("find() for Postgres still needs to be implemented")
    }
}