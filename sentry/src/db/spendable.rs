use std::convert::TryFrom;

use primitives::{channel_v5::Channel, Address, ChannelId, UnifiedNum};
use tokio_postgres::{Client, Error, Row};

#[derive(Debug, PartialEq, Eq)]
pub struct Spendable {
    spender: Address,
    channel: Channel,
    total: UnifiedNum,
    still_on_create2: UnifiedNum,
}

/// ```text
/// INSERT INTO spendable (spender, channel_id, channel, total, still_on_create2)
/// values ('0xce07CbB7e054514D590a0262C93070D838bFBA2e', '0x061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088', '{}', 10.00000000, 2.00000000);
/// ```
pub async fn insert_spendable(client: &Client, spendable: &Spendable) -> Result<bool, Error> {
    let stmt = client.prepare("INSERT INTO spendable (spender, channel_id, channel, total, still_on_create2) values ($1, $2, $3, $4, $5)").await?;

    let row = client
        .execute(
            &stmt,
            &[
                &spendable.spender,
                &spendable.channel.id(),
                &spendable.channel,
                &spendable.total,
                &spendable.still_on_create2,
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
    client: &Client,
    spender: &Address,
    channel_id: &ChannelId,
) -> Result<Spendable, Error> {
    let statement = client.prepare("SELECT spender, channel_id, channel, total, still_on_create2 FROM spendable WHERE spender = $1 AND channel_id = $2").await?;

    let row = client.query_one(&statement, &[spender, channel_id]).await?;

    Spendable::try_from(row)
}

impl TryFrom<Row> for Spendable {
    type Error = Error;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        Ok(Spendable {
            spender: row.try_get("spender")?,
            channel: row.try_get("channel")?,
            total: row.try_get("total")?,
            still_on_create2: row.try_get("still_on_create2")?,
        })
    }
}

#[cfg(test)]
mod test {
    use primitives::{
        util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN},
        UnifiedNum,
    };

    use crate::db::postgres_pool::{setup_test_migrations, TESTS_POOL};

    use super::*;

    #[tokio::test]
    async fn it_inserts_and_fetches_spendable() {
        let test_client = TESTS_POOL.get().await.unwrap();

        setup_test_migrations(&test_client)
            .await
            .expect("Migrations should succeed");

        let spendable = Spendable {
            spender: ADDRESSES["user"],
            channel: DUMMY_CAMPAIGN.channel.clone(),
            total: UnifiedNum::from(100_000_000),
            still_on_create2: UnifiedNum::from(500_000),
        };
        let is_inserted = insert_spendable(&test_client, &spendable)
            .await
            .expect("Should succeed");

        assert!(is_inserted);

        let fetched_spendable =
            fetch_spendable(&test_client, &spendable.spender, &spendable.channel.id())
                .await
                .expect("Should fetch successfully");

        assert_eq!(spendable, fetched_spendable);
    }
}
