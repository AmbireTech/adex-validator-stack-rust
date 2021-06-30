use chrono::{DateTime, Utc};
use primitives::{
    Address, ChannelId, UnifiedNum,
};
use tokio_postgres::{IsolationLevel, Row, types::{FromSql, ToSql}};

use super::{DbPool, PoolError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Accounting Balances error: {0}")]
    Balances(#[from] primitives::sentry::accounting::Error),
    #[error("Fetching Accounting from postgres error: {0}")]
    Postgres(#[from] PoolError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accounting {
    pub channel_id: ChannelId,
    pub side: Side,
    pub address: Address,
    pub amount: UnifiedNum,
    pub updated: Option<DateTime<Utc>>,
    pub created: DateTime<Utc>,
}

impl From<&Row> for Accounting {
    fn from(row: &Row) -> Self {
        Self {
            channel_id: row.get("channel_id"),
            side: row.get("side"),
            address: row.get("address"),
            amount: row.get("amount"),
            updated: row.get("updated"),
            created: row.get("created"),
        }
    }
}

#[derive(Debug, Clone, Copy, ToSql, FromSql, PartialEq, Eq)]
#[postgres(name = "accountingside")]
pub enum Side {
    Earner,
    Spender,
}

pub enum SpendError {
    Pool(PoolError),
    NoRecordsUpdated,
}

/// ```text
/// SELECT channel_id, side, address, amount, updated, created FROM accounting WHERE channel_id = $1 AND address = $2 AND side = $3
/// ```
pub async fn get_accounting(
    pool: DbPool,
    channel_id: ChannelId,
    address: Address,
    side: Side,
) -> Result<Option<Accounting>, PoolError> {
    let client = pool.get().await?;
    let statement = client
        .prepare("SELECT channel_id, side, address, amount, updated, created FROM accounting WHERE channel_id = $1 AND address = $2 AND side = $3")
        .await?;

    let row = client
        .query_opt(&statement, &[&channel_id, &address, &side])
        .await?;

    Ok(row.as_ref().map(Accounting::from))
}

/// Will update current Spender/Earner amount or insert a new Accounting record
///
/// See `UPDATE_ACCOUNTING_STATEMENT` static for full query.
static UPDATE_ACCOUNTING_STATEMENT: &str = "INSERT INTO accounting(channel_id, side, address, amount, updated, created) VALUES($1, $2, $3, $4, $5, $6) ON CONFLICT ON CONSTRAINT accounting_pkey DO UPDATE SET amount = accounting.amount + $4, updated = $6 WHERE accounting.channel_id = $1 AND accounting.side = $2 AND accounting.address = $3 RETURNING channel_id, side, address, amount, updated, created";
pub async fn update_accounting(
    pool: DbPool,
    channel_id: ChannelId,
    address: Address,
    side: Side,
    amount: UnifiedNum,
) -> Result<Accounting, PoolError> {
    let client = pool.get().await?;
    let statement = client
        .prepare(UPDATE_ACCOUNTING_STATEMENT)
        .await?;

    let now = Utc::now();
    let updated: Option<DateTime<Utc>> = None;

    let row = client
        .query_one(
            &statement,
            &[&channel_id, &side, &address, &amount, &updated, &now],
        )
        .await?;

    Ok(Accounting::from(&row))
}

/// Will use `UPDATE_ACCOUNTING_STATEMENT` to create and run the query twice - once for Earner and once for Spender accounting.
///
/// It runs both queries in a transaction in order to rollback if one of the queries fails.
pub async fn spend_accountings(
    pool: DbPool,
    channel_id: ChannelId,
    earner: Address,
    spender: Address,
    amount: UnifiedNum,
) -> Result<(Accounting, Accounting), PoolError> {
    let mut client = pool.get().await?;

    // The reads and writes in this transaction must be able to be committed as an atomic “unit” with respect to reads and writes of all other concurrent serializable transactions without interleaving.
    let transaction = client.build_transaction().isolation_level(IsolationLevel::Serializable).start().await?;

    let statement = transaction.prepare(UPDATE_ACCOUNTING_STATEMENT).await?;

    let now = Utc::now();
    let updated: Option<DateTime<Utc>> = None;

    let earner_row = transaction.query_one(&statement, &[&channel_id, &Side::Earner, &earner, &amount, &updated, &now]).await?;
    let spender_row = transaction.query_one(&statement, &[&channel_id, &Side::Spender, &spender, &amount, &updated, &now]).await?;

    transaction.commit().await?;

    Ok((Accounting::from(&earner_row), Accounting::from(&spender_row)))
}

#[cfg(test)]
mod test {
    use primitives::util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN};

    use crate::db::tests_postgres::{setup_test_migrations, DATABASE_POOL};

    use super::*;

    #[tokio::test]
    async fn insert_update_and_get_accounting() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let channel_id = DUMMY_CAMPAIGN.channel.id();
        let earner = ADDRESSES["publisher"];
        let spender = ADDRESSES["creator"];

        let amount = UnifiedNum::from(100_000_000);
        let update_amount = UnifiedNum::from(200_000_000);

        // Spender insert/update
        {
            let inserted = update_accounting(
                database.pool.clone(),
                channel_id,
                spender,
                Side::Spender,
                amount,
            )
            .await
            .expect("Should insert");
            assert_eq!(spender, inserted.address);
            assert_eq!(Side::Spender, inserted.side);
            assert_eq!(UnifiedNum::from(100_000_000), inserted.amount);

            let updated = update_accounting(
                database.pool.clone(),
                channel_id,
                spender,
                Side::Spender,
                update_amount,
            )
            .await
            .expect("Should insert");
            assert_eq!(spender, updated.address);
            assert_eq!(Side::Spender, updated.side);
            assert_eq!(
                UnifiedNum::from(300_000_000),
                updated.amount,
                "Should add the newly spent amount to the existing one"
            );

            let spent = get_accounting(database.pool.clone(), channel_id, spender, Side::Spender).await.expect("Should query for the updated accounting");
            assert_eq!(Some(updated), spent);

            let earned = get_accounting(database.pool.clone(), channel_id, spender, Side::Earner).await.expect("Should query for accounting");
            assert!(earned.is_none(), "Spender shouldn't have an earned amount");
        }

        // Earner insert/update
        {
            let inserted = update_accounting(
                database.pool.clone(),
                channel_id,
                earner,
                Side::Earner,
                amount,
            )
            .await
            .expect("Should insert");
            assert_eq!(earner, inserted.address);
            assert_eq!(Side::Earner, inserted.side);
            assert_eq!(UnifiedNum::from(100_000_000), inserted.amount);

            let updated = update_accounting(
                database.pool.clone(),
                channel_id,
                earner,
                Side::Earner,
                update_amount,
            )
            .await
            .expect("Should insert");
            assert_eq!(earner, updated.address);
            assert_eq!(Side::Earner, updated.side);
            assert_eq!(
                UnifiedNum::from(300_000_000),
                updated.amount,
                "Should add the newly earned amount to the existing one"
            );

            let earned = get_accounting(database.pool.clone(), channel_id, earner, Side::Earner).await.expect("Should query for the updated accounting");
            assert_eq!(Some(updated), earned);

            let spent = get_accounting(database.pool.clone(), channel_id, earner, Side::Spender).await.expect("Should query for accounting");
            assert!(spent.is_none(), "Earner shouldn't have a spent amount");
        }


        // Spender as Earner & another Spender
        // Will test the previously spent amount as well!
        {
            let spender_as_earner = spender;

            let inserted = update_accounting(
                database.pool.clone(),
                channel_id,
                spender_as_earner,
                Side::Earner,
                amount,
            )
            .await
            .expect("Should insert");
            assert_eq!(spender_as_earner, inserted.address);
            assert_eq!(Side::Earner, inserted.side);
            assert_eq!(UnifiedNum::from(100_000_000), inserted.amount);

            let updated = update_accounting(
                database.pool.clone(),
                channel_id,
                spender_as_earner,
                Side::Earner,
                UnifiedNum::from(999),
            )
            .await
            .expect("Should insert");
            assert_eq!(spender, updated.address);
            assert_eq!(Side::Earner, updated.side);
            assert_eq!(
                UnifiedNum::from(100_000_999),
                updated.amount,
                "Should add the newly spent amount to the existing one"
            );

            let earned_acc = get_accounting(database.pool.clone(), channel_id, spender_as_earner, Side::Earner).await.expect("Should query for earned accounting").expect("Should have Earned accounting for Spender as Earner");
            assert_eq!(UnifiedNum::from(100_000_999), earned_acc.amount);
            
            let spent_acc = get_accounting(database.pool.clone(), channel_id, spender_as_earner, Side::Spender).await.expect("Should query for spent accounting").expect("Should have Spent accounting for Spender as Earner");
            assert_eq!(UnifiedNum::from(300_000_000), spent_acc.amount);
            
        }
    }
    
    #[tokio::test]
    async fn test_spending_accountings() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let channel_id = DUMMY_CAMPAIGN.channel.id();
        let earner = ADDRESSES["publisher"];
        let spender = ADDRESSES["creator"];
        let other_spender = ADDRESSES["tester"];

        let amount = UnifiedNum::from(100_000_000);
        let update_amount = UnifiedNum::from(200_000_000);

        // Spender & Earner insert
        let (inserted_earner, inserted_spender) = spend_accountings(database.pool.clone(), channel_id, earner, spender, amount).await.expect("Should insert Earner and Spender");
        assert_eq!(earner, inserted_earner.address);
        assert_eq!(Side::Earner, inserted_earner.side);
        assert_eq!(UnifiedNum::from(100_000_000), inserted_earner.amount);
        
        assert_eq!(spender, inserted_spender.address);
        assert_eq!(Side::Spender, inserted_spender.side);
        assert_eq!(UnifiedNum::from(100_000_000), inserted_spender.amount);

        // Spender & Earner update
        let (updated_earner, updated_spender) = spend_accountings(database.pool.clone(), channel_id, earner, spender, update_amount).await.expect("Should update Earner and Spender");

        assert_eq!(earner, updated_earner.address);
        assert_eq!(Side::Earner, updated_earner.side);
        assert_eq!(UnifiedNum::from(300_000_000), updated_earner.amount, "Should add the newly earned amount to the existing one");
        
        assert_eq!(spender, updated_spender.address);
        assert_eq!(Side::Spender, updated_spender.side);
        assert_eq!(UnifiedNum::from(300_000_000), updated_spender.amount, "Should add the newly spend amount to the existing one");

        // Spender as an Earner & another spender
        let (spender_as_earner, inserted_other_spender) = spend_accountings(database.pool.clone(), channel_id, spender, other_spender, UnifiedNum::from(999)).await.expect("Should update Spender as Earner and the Other Spender");

        assert_eq!(spender, spender_as_earner.address);
        assert_eq!(Side::Earner, spender_as_earner.side);
        assert_eq!(UnifiedNum::from(999), spender_as_earner.amount, "Should add earner accounting for the previous Spender");

        assert_eq!(other_spender, inserted_other_spender.address);
        assert_eq!(Side::Spender, inserted_other_spender.side);
        assert_eq!(UnifiedNum::from(999), inserted_other_spender.amount);

        let earned = get_accounting(database.pool.clone(), channel_id, spender, Side::Earner).await.expect("Should query for accounting").expect("Should have Earned accounting for Spender as Earner");
        assert_eq!(UnifiedNum::from(999), earned.amount);
        
        let spent = get_accounting(database.pool.clone(), channel_id, spender, Side::Spender).await.expect("Should query for accounting").expect("Should have Spent accounting for Spender as Earner");
        assert_eq!(UnifiedNum::from(300_000_000), spent.amount);
    }
}
