use futures::compat::Future01CompatExt;
use futures::future::FutureExt;
use futures_legacy::Future as OldFuture;
use tokio_postgres::types::Json;
use tokio_postgres::Row;

use primitives::{Channel, ChannelId, ChannelSpec, RepositoryFuture};
use try_future::try_future;

use crate::domain::channel::{ChannelListParams, ChannelRepository};
use crate::infrastructure::field::{asset::AssetPg, bignum::BigNumPg, channel_id::ChannelIdPg};
use crate::infrastructure::persistence::postgres::PostgresPersistenceError;
use crate::infrastructure::persistence::DbPool;
use crate::infrastructure::util::bb8::query_result;

#[derive(Debug)]
pub struct ChannelRepository {
    db_pool: DbPool,
}

pub trait ChannelRepository: Send + Sync {
    /// Returns a list of channels, based on the passed Parameters for this method
    fn list(&self, params: &ChannelListParams) -> RepositoryFuture<Vec<Channel>>;

    fn list_count(&self, params: &ChannelListParams) -> RepositoryFuture<u64>;

    fn find(&self, channel_id: &ChannelId) -> RepositoryFuture<Option<Channel>>;

    fn add(&self, channel: Channel) -> RepositoryFuture<()>;
}

impl ChannelRepository {
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
                        Ok(stmt) => query_result(conn.query(&stmt, &[]), conn),
                        Err(err) => try_future!(Err((err, conn))),
                    })
                    .and_then(|(rows, conn)| {
                        let channels = rows.iter().map(|row| channel_map(row)).collect();

                        Ok((channels, conn))
                    })
            })
            .map_err(|err| PostgresPersistenceError::from(err).into());

        fut.compat().boxed()
    }

    fn list_count(&self, _params: &ChannelListParams) -> RepositoryFuture<u64> {
        let fut = self
            .db_pool
            .run(move |mut conn| {
                conn.prepare("SELECT COUNT(channel_id)::TEXT FROM channels")
                    .then(move |res| match res {
                        Ok(stmt) => query_result(conn.query(&stmt, &[]), conn),
                        Err(err) => try_future!(Err((err, conn))),
                    })
                    .and_then(|(rows, conn)| {
                        let count = rows[0]
                            .get::<_, &str>(0)
                            .parse::<u64>()
                            .expect("Not possible to have that many rows");

                        Ok((count, conn))
                    })
            })
            .map_err(|err| PostgresPersistenceError::from(err).into());

        fut.compat().boxed()
    }

    fn find(&self, _channel_id: &ChannelId) -> RepositoryFuture<Option<Channel>> {
        unimplemented!("find() for Postgres still needs to be implemented")
    }

    fn add(&self, _channel: Channel) -> RepositoryFuture<()> {
        unimplemented!("create() for Postgres still needs to be implemented")
    }
}

fn channel_map(row: &Row) -> Channel {
    let spec: ChannelSpec = row.get::<_, Json<ChannelSpec>>("spec").0;
    Channel {
        id: row.get::<_, ChannelIdPg>("channel_id").into(),
        creator: row.get("creator"),
        deposit_asset: row.get::<_, AssetPg>("deposit_asset").into(),
        deposit_amount: row.get::<_, BigNumPg>("deposit_amount").into(),
        valid_until: row.get("valid_until"),
        spec,
    }
}
