use std::convert::TryFrom;

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
) -> Result<Spendable, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare("SELECT spender, channel_id, channel, total, still_on_create2 FROM spendable WHERE spender = $1 AND channel_id = $2").await?;

    let row = client.query_one(&statement, &[spender, channel_id]).await?;

    Ok(Spendable::try_from(row)?)
}

#[cfg(test)]
mod test {
    use primitives::{
        spender::{Deposit, Spendable},
        util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN},
        UnifiedNum,
    };

    use crate::db::{
        tests_postgres::{setup_test_migrations, test_postgres_connection},
        POSTGRES_CONFIG,
    };

    use super::*;

    #[tokio::test]
    async fn it_inserts_and_fetches_spendable() {
        let test_pool = test_postgres_connection(POSTGRES_CONFIG.clone())
            .get()
            .await
            .unwrap();
        // let pool = test_pool.get().await.expect("Should get a DB pool");

        setup_test_migrations(test_pool.clone())
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
        let is_inserted = insert_spendable(test_pool.clone(), &spendable)
            .await
            .expect("Should succeed");

        assert!(is_inserted);

        let fetched_spendable = fetch_spendable(
            test_pool.clone(),
            &spendable.spender,
            &spendable.channel.id(),
        )
        .await
        .expect("Should fetch successfully");

        assert_eq!(spendable, fetched_spendable);
    }
}
