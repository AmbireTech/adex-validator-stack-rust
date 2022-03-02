use chrono::{DateTime, Utc};
use primitives::{
    balances::{Balances, CheckedState},
    Address, ChannelId, UnifiedNum,
};
use tokio_postgres::{
    types::{FromSql, ToSql},
    IsolationLevel, Row,
};

use super::{DbPool, PoolError};
use serde::Serialize;
use thiserror::Error;

static UPDATE_ACCOUNTING_STATEMENT: &str = "INSERT INTO accounting(channel_id, side, address, amount, updated, created) VALUES($1, $2, $3, $4, $5, $6) ON CONFLICT ON CONSTRAINT accounting_pkey DO UPDATE SET amount = accounting.amount + $4, updated = $6 WHERE accounting.channel_id = $1 AND accounting.side = $2 AND accounting.address = $3 RETURNING channel_id, side, address, amount, updated, created";

#[derive(Debug, Error)]
pub enum Error {
    #[error("Accounting Balances error: {0}")]
    Balances(#[from] primitives::balances::Error),
    #[error("Fetching Accounting from postgres error: {0}")]
    Postgres(#[from] PoolError),
}

impl From<tokio_postgres::Error> for Error {
    fn from(error: tokio_postgres::Error) -> Self {
        Self::Postgres(PoolError::Backend(error))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, Copy, ToSql, FromSql, PartialEq, Eq, Serialize)]
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

pub async fn get_all_accountings_for_channel(
    pool: DbPool,
    channel_id: ChannelId,
) -> Result<Vec<Accounting>, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare("SELECT channel_id, side, address, amount, updated, created FROM accounting WHERE channel_id = $1").await?;

    let rows = client.query(&statement, &[&channel_id]).await?;

    let accountings = rows.iter().map(Accounting::from).collect();

    Ok(accountings)
}

/// Will update current Spender/Earner amount or insert a new Accounting record
///
/// See `UPDATE_ACCOUNTING_STATEMENT` static for full query.
pub async fn update_accounting(
    pool: DbPool,
    channel_id: ChannelId,
    address: Address,
    side: Side,
    amount: UnifiedNum,
) -> Result<Accounting, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare(UPDATE_ACCOUNTING_STATEMENT).await?;

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

/// `delta_balances` defines the Balances that need to be added to the spending or earnings of the `Accounting`s.
/// It will **not** override the whole `Accounting` value
/// Returns a tuple of `(Vec<Earners Accounting>, Vec<Spenders Accounting>)`
pub async fn spend_amount(
    pool: DbPool,
    channel_id: ChannelId,
    delta_balances: Balances<CheckedState>,
) -> Result<(Vec<Accounting>, Vec<Accounting>), PoolError> {
    let mut client = pool.get().await?;

    // The reads and writes in this transaction must be able to be committed as an atomic “unit” with respect to reads and writes of all other concurrent serializable transactions without interleaving.
    let transaction = client
        .build_transaction()
        .isolation_level(IsolationLevel::Serializable)
        .start()
        .await?;

    let statement = transaction.prepare(UPDATE_ACCOUNTING_STATEMENT).await?;

    let now = Utc::now();
    let updated: Option<DateTime<Utc>> = None;

    let (mut earners, mut spenders) = (vec![], vec![]);

    // Earners
    for (earner, amount) in delta_balances.earners {
        let row = transaction
            .query_one(
                &statement,
                &[&channel_id, &Side::Earner, &earner, &amount, &updated, &now],
            )
            .await?;

        earners.push(Accounting::from(&row))
    }

    // Spenders
    for (spender, amount) in delta_balances.spenders {
        let row = transaction
            .query_one(
                &statement,
                &[
                    &channel_id,
                    &Side::Spender,
                    &spender,
                    &amount,
                    &updated,
                    &now,
                ],
            )
            .await?;

        spenders.push(Accounting::from(&row))
    }

    transaction.commit().await?;

    Ok((earners, spenders))
}

#[cfg(test)]
mod test {
    use primitives::test_util::{
        ADVERTISER, ADVERTISER_2, CREATOR, DUMMY_CAMPAIGN, PUBLISHER, PUBLISHER_2,
    };

    use crate::db::{
        insert_channel,
        tests_postgres::{setup_test_migrations, DATABASE_POOL},
    };

    use super::*;

    #[tokio::test]
    async fn insert_update_and_get_accounting() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");
        // insert the channel into the DB
        let channel = insert_channel(&database.pool, DUMMY_CAMPAIGN.channel)
            .await
            .expect("Should insert");

        let channel_id = channel.id();
        let earner = *PUBLISHER;
        let spender = *CREATOR;

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
            assert_eq!(amount, inserted.amount);

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
                amount + update_amount,
                updated.amount,
                "Should add the newly spent amount to the existing one"
            );

            let spent = get_accounting(database.pool.clone(), channel_id, spender, Side::Spender)
                .await
                .expect("Should query for the updated accounting");
            assert_eq!(Some(updated), spent);

            let earned = get_accounting(database.pool.clone(), channel_id, spender, Side::Earner)
                .await
                .expect("Should query for accounting");
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
            assert_eq!(amount, inserted.amount);

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
                amount + update_amount,
                updated.amount,
                "Should add the newly earned amount to the existing one"
            );

            let earned = get_accounting(database.pool.clone(), channel_id, earner, Side::Earner)
                .await
                .expect("Should query for the updated accounting");
            assert_eq!(Some(updated), earned);

            let spent = get_accounting(database.pool.clone(), channel_id, earner, Side::Spender)
                .await
                .expect("Should query for accounting");
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
            assert_eq!(amount, inserted.amount);

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

            let earned_acc = get_accounting(
                database.pool.clone(),
                channel_id,
                spender_as_earner,
                Side::Earner,
            )
            .await
            .expect("Should query for earned accounting")
            .expect("Should have Earned accounting for Spender as Earner");
            assert_eq!(UnifiedNum::from(100_000_999), earned_acc.amount);

            let spent_acc = get_accounting(
                database.pool.clone(),
                channel_id,
                spender_as_earner,
                Side::Spender,
            )
            .await
            .expect("Should query for spent accounting")
            .expect("Should have Spent accounting for Spender as Earner");
            assert_eq!(UnifiedNum::from(300_000_000), spent_acc.amount);
        }
    }

    fn assert_accounting(
        expected: (Address, Side, UnifiedNum),
        accounting: Accounting,
        with_set_updated: bool,
    ) {
        assert_eq!(
            expected.0, accounting.address,
            "Accounting address is not the same"
        );
        assert_eq!(
            expected.1, accounting.side,
            "Accounting side is not the same"
        );
        assert_eq!(
            expected.2, accounting.amount,
            "Accounting amount is not the same"
        );

        if with_set_updated {
            assert!(
                accounting.updated.is_some(),
                "Accounting should have been updated"
            )
        } else {
            assert!(
                accounting.updated.is_none(),
                "Accounting should not have been updated"
            )
        }
    }

    #[tokio::test]
    async fn test_spend_amount() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        // insert the channel into the DB
        let channel = insert_channel(&database.pool, DUMMY_CAMPAIGN.channel)
            .await
            .expect("Should insert");

        let channel_id = channel.id();
        let earner = *PUBLISHER;
        let spender = *CREATOR;
        let spender_as_earner = spender;
        let other_spender = *ADVERTISER;

        let cases = [
            // Spender & Earner insert
            (
                UnifiedNum::from(100_000_000),
                earner,
                spender,
                [
                    vec![(earner, Side::Earner, UnifiedNum::from(100_000_000), false)],
                    vec![(spender, Side::Spender, UnifiedNum::from(100_000_000), false)],
                ],
            ),
            // Spender & Earner update
            (
                UnifiedNum::from(200_000_000),
                earner,
                spender,
                [
                    vec![(earner, Side::Earner, UnifiedNum::from(300_000_000), true)],
                    vec![(spender, Side::Spender, UnifiedNum::from(300_000_000), true)],
                ],
            ),
            // Spender as an Earner & another spender
            (
                UnifiedNum::from(999),
                spender_as_earner,
                other_spender,
                [
                    vec![(spender, Side::Earner, UnifiedNum::from(999), false)],
                    vec![(other_spender, Side::Spender, UnifiedNum::from(999), false)],
                ],
            ),
        ];

        for (amount_to_spend, earner, spender, [earners, spenders]) in cases {
            // Spender & Earner insert
            let mut balances = Balances::<CheckedState>::default();
            balances
                .spend(spender, earner, amount_to_spend)
                .expect("Should spend");

            let (actual_earners, actual_spenders) =
                spend_amount(database.pool.clone(), channel_id, balances)
                    .await
                    .expect("Should insert Earner and Spender");

            for (actual, expected) in actual_earners.into_iter().zip(earners) {
                assert_accounting((expected.0, expected.1, expected.2), actual, expected.3)
            }

            for (actual, expected) in actual_spenders.into_iter().zip(spenders) {
                assert_accounting((expected.0, expected.1, expected.2), actual, expected.3)
            }
        }

        // Check the final amounts of Spent/Earned for the Spender
        let earned = get_accounting(database.pool.clone(), channel_id, spender, Side::Earner)
            .await
            .expect("Should query for accounting")
            .expect("Should have Earned accounting for Spender as Earner");
        assert_eq!(UnifiedNum::from(999), earned.amount);

        let spent = get_accounting(database.pool.clone(), channel_id, spender, Side::Spender)
            .await
            .expect("Should query for accounting")
            .expect("Should have Spent accounting for Spender as Earner");
        assert_eq!(UnifiedNum::from(300_000_000), spent.amount);
    }

    #[tokio::test]
    async fn test_spend_amount_with_multiple_spends() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        // insert the channel into the DB
        let channel = insert_channel(&database.pool, DUMMY_CAMPAIGN.channel)
            .await
            .expect("Should insert");

        let channel_id = channel.id();
        let earner = *PUBLISHER;
        let other_earner = *PUBLISHER_2;
        let spender = *CREATOR;
        let spender_as_earner = spender;
        let other_spender = *ADVERTISER;
        let third_spender = *ADVERTISER_2;

        // Spenders & Earners insert
        {
            let mut balances = Balances::<CheckedState>::default();
            balances
                .spend(spender, earner, UnifiedNum::from(400_000))
                .expect("Should spend");
            balances
                .spend(other_spender, other_earner, UnifiedNum::from(500_000))
                .expect("Should spend");

            let (earners_acc, spenders_acc) =
                spend_amount(database.pool.clone(), channel_id, balances)
                    .await
                    .expect("Should insert Earners and Spenders");

            assert_eq!(2, earners_acc.len());
            assert_eq!(2, spenders_acc.len());

            // Earners assertions
            assert_accounting(
                (earner, Side::Earner, UnifiedNum::from(400_000)),
                earners_acc
                    .iter()
                    .find(|a| a.address == earner)
                    .unwrap()
                    .clone(),
                false,
            );
            assert_accounting(
                (other_earner, Side::Earner, UnifiedNum::from(500_000)),
                earners_acc
                    .iter()
                    .find(|a| a.address == other_earner)
                    .unwrap()
                    .clone(),
                false,
            );

            // Spenders assertions
            assert_accounting(
                (spender, Side::Spender, UnifiedNum::from(400_000)),
                spenders_acc
                    .iter()
                    .find(|a| a.address == spender)
                    .unwrap()
                    .clone(),
                false,
            );
            assert_accounting(
                (other_spender, Side::Spender, UnifiedNum::from(500_000)),
                spenders_acc
                    .iter()
                    .find(|a| a.address == other_spender)
                    .unwrap()
                    .clone(),
                false,
            );
        }
        // Spenders & Earners update with 1 insert (third_spender & spender_as_earner)
        {
            let mut balances = Balances::<CheckedState>::default();
            balances
                .spend(spender, earner, UnifiedNum::from(1_400_000))
                .expect("Should spend");
            balances
                .spend(other_spender, other_earner, UnifiedNum::from(1_500_000))
                .expect("Should spend");
            balances
                .spend(third_spender, spender_as_earner, UnifiedNum::from(600_000))
                .expect("Should spend");

            let (earners_acc, spenders_acc) =
                spend_amount(database.pool.clone(), channel_id, balances)
                    .await
                    .expect("Should update & insert new Earners and Spenders");

            assert_eq!(3, earners_acc.len());
            assert_eq!(3, spenders_acc.len());

            // Earners assertions
            assert_accounting(
                (earner, Side::Earner, UnifiedNum::from(1_800_000)),
                earners_acc
                    .iter()
                    .find(|a| a.address == earner)
                    .unwrap()
                    .clone(),
                true,
            );
            assert_accounting(
                (other_earner, Side::Earner, UnifiedNum::from(2_000_000)),
                earners_acc
                    .iter()
                    .find(|a| a.address == other_earner)
                    .unwrap()
                    .clone(),
                true,
            );
            assert_accounting(
                (spender_as_earner, Side::Earner, UnifiedNum::from(600_000)),
                earners_acc
                    .iter()
                    .find(|a| a.address == spender_as_earner)
                    .unwrap()
                    .clone(),
                false,
            );

            // Spenders assertions
            assert_accounting(
                (spender, Side::Spender, UnifiedNum::from(1_800_000)),
                spenders_acc
                    .iter()
                    .find(|a| a.address == spender)
                    .unwrap()
                    .clone(),
                true,
            );
            assert_accounting(
                (other_spender, Side::Spender, UnifiedNum::from(2_000_000)),
                spenders_acc
                    .iter()
                    .find(|a| a.address == other_spender)
                    .unwrap()
                    .clone(),
                true,
            );
            assert_accounting(
                (third_spender, Side::Spender, UnifiedNum::from(600_000)),
                spenders_acc
                    .iter()
                    .find(|a| a.address == third_spender)
                    .unwrap()
                    .clone(),
                false,
            );
        }
    }
}
