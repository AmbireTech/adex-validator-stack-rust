use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use primitives::{
    channel_v5::Channel,
    sentry::accounting::{Accounting, Balances, CheckedState},
    Address, ChannelId, UnifiedNum,
};
use tokio_postgres::types::Json;

use super::{DbPool, PoolError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Accounting Balances error: {0}")]
    Balances(#[from] primitives::sentry::accounting::Error),
    #[error("Fetching Accounting from postgres error: {0}")]
    Postgres(#[from] PoolError),
}

/// ```text
/// SELECT (spenders ->> $1)::bigint as spent FROM accounting WHERE channel_id = $2
/// ```
/// This function returns the spent amount in a `Channel` of a given spender
pub async fn get_accounting_spent(
    pool: DbPool,
    spender: &Address,
    channel_id: &ChannelId,
) -> Result<UnifiedNum, PoolError> {
    let client = pool.get().await?;
    let statement = client
        .prepare("SELECT (spenders ->> $1)::bigint as spent FROM accounting WHERE channel_id = $2")
        .await?;

    let row = client.query_one(&statement, &[spender, channel_id]).await?;

    Ok(row.get("spent"))
}

pub async fn insert_accounting(
    pool: DbPool,
    channel: Channel,
    balances: Balances<CheckedState>,
) -> Result<Accounting<CheckedState>, Error> {
    let client = pool.get().await?;

    let statement = client.prepare("INSERT INTO accounting (channel_id, channel, earners, spenders, updated, created) VALUES ($1, $2, $3, $4, $5, NOW()) RETURNING channel, earners, spenders, updated, created").await.map_err(PoolError::Backend)?;

    let earners = Json(&balances.earners);
    let spenders = Json(&balances.spenders);
    let updated: Option<DateTime<Utc>> = None;

    let row = client
        .query_one(
            &statement,
            &[&channel.id(), &channel, &earners, &spenders, &updated],
        )
        .await
        .map_err(PoolError::Backend)?;

    Accounting::try_from(&row).map_err(Error::Balances)
}

#[cfg(test)]
mod test {
    use primitives::util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN};

    use crate::db::tests_postgres::{setup_test_migrations, DATABASE_POOL};

    use super::*;

    #[tokio::test]
    async fn get_spent() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let channel = DUMMY_CAMPAIGN.channel.clone();

        let spender = ADDRESSES["creator"];
        let earner = ADDRESSES["publisher"];

        let mut balances = Balances::default();
        let spend_amount = UnifiedNum::from(100);
        balances
            .spend(spender, earner, spend_amount)
            .expect("Should be ok");

        let accounting = insert_accounting(database.pool.clone(), channel.clone(), balances)
            .await
            .expect("Should insert");

        let spent = get_accounting_spent(database.pool.clone(), &spender, &channel.id())
            .await
            .expect("Should get spent");

        assert_eq!(spend_amount, spent);
        assert_eq!(
            accounting
                .balances
                .spenders
                .get(&spender)
                .expect("Should contain value"),
            &spent
        );
    }
}
