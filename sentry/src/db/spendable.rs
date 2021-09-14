use primitives::{spender::Spendable, Address, ChannelId};

use super::{DbPool, PoolError};

/// ```text
/// INSERT INTO spendable (spender, channel_id, channel, total, still_on_create2)
/// values ('0xce07CbB7e054514D590a0262C93070D838bFBA2e', '0x061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088', '{}', 10.00000000, 2.00000000);
/// ```
pub async fn insert_spendable(pool: DbPool, spendable: &Spendable) -> Result<bool, PoolError> {
    let client = pool.get().await?;
    let stmt = client.prepare("INSERT INTO spendable (spender, channel_id, channel, total, still_on_create2) values ($1, $2, $3, $4, $5)").await?;

    let row = client
        .execute(
            &stmt,
            &[
                &spendable.spender,
                &spendable.channel.id(),
                &spendable.channel,
                &spendable.deposit.total,
                &spendable.deposit.still_on_create2,
            ],
        )
        .await?;

    let is_inserted = row == 1;
    Ok(is_inserted)
}

/// ```text
/// SELECT spender, channel_id, channel, total, still_on_create2 FROM spendable
/// WHERE spender = $1 AND channel_id = $2
/// ```
pub async fn fetch_spendable(
    pool: DbPool,
    spender: &Address,
    channel_id: &ChannelId,
) -> Result<Option<Spendable>, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare("SELECT spender, channel_id, channel, total, still_on_create2 FROM spendable WHERE spender = $1 AND channel_id = $2").await?;

    let row = client.query_opt(&statement, &[spender, channel_id]).await?;

    Ok(row.map(Spendable::from))
}

static GET_ALL_SPENDERS_STATEMENT: &str = "SELECT spender, channel_id, channel, total, still_on_create2 FROM spendable WHERE channel_id = $1";

// TODO: Include pagination
pub async fn get_all_spendables_for_channel(
    pool: DbPool,
    channel_id: &ChannelId,
) -> Result<Vec<Spendable>, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare(GET_ALL_SPENDERS_STATEMENT).await?;

    let rows = client.query(&statement, &[channel_id]).await?;
    let spendables: Vec<Spendable> = rows.into_iter().map(Spendable::from).collect();

    Ok(spendables)
}

static UPDATE_SPENDABLE_STATEMENT: &str = "INSERT INTO spendable(spender, channel_id, channel, total, still_on_create2) VALUES($1, $2, $3, $4, $5) ON CONFLICT ON CONSTRAINT spendable_pkey DO UPDATE SET total = $4, still_on_create2 = $5 WHERE spendable.spender = $1 AND spendable.channel_id = $2 RETURNING spender, channel_id, channel, total, still_on_create2";

// Updates spendable entry deposit or inserts a new spendable entry if it doesn't exist
pub async fn update_spendable(pool: DbPool, spendable: &Spendable) -> Result<Spendable, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare(UPDATE_SPENDABLE_STATEMENT).await?;

    let row = client
        .query_one(
            &statement,
            &[
                &spendable.spender,
                &spendable.channel.id(),
                &spendable.channel,
                &spendable.deposit.total,
                &spendable.deposit.still_on_create2,
            ],
        )
        .await?;

    Ok(Spendable::from(row))
}

#[cfg(test)]
mod test {
    use primitives::{
        spender::{Deposit, Spendable},
        util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN},
        UnifiedNum,
    };

    use crate::db::tests_postgres::{setup_test_migrations, DATABASE_POOL};

    use super::*;

    #[tokio::test]
    async fn it_inserts_and_fetches_spendable() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let spendable = Spendable {
            spender: ADDRESSES["user"],
            channel: DUMMY_CAMPAIGN.channel.clone(),
            deposit: Deposit {
                total: UnifiedNum::from(100_000_000),
                still_on_create2: UnifiedNum::from(500_000),
            },
        };
        let is_inserted = insert_spendable(database.pool.clone(), &spendable)
            .await
            .expect("Should succeed");

        assert!(is_inserted);

        let fetched_spendable = fetch_spendable(
            database.pool.clone(),
            &spendable.spender,
            &spendable.channel.id(),
        )
        .await
        .expect("Should fetch successfully");

        assert_eq!(Some(spendable), fetched_spendable);
    }
}
