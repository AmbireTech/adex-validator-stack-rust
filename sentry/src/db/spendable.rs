use crate::db::TotalCount;
use chrono::Utc;
use primitives::{sentry::Pagination, spender::Spendable, Address, ChannelId};

use super::{DbPool, PoolError};

/// ```text
/// INSERT INTO spendable (spender, channel_id, total, still_on_create2, created)
/// values ('0xce07CbB7e054514D590a0262C93070D838bFBA2e', '0x061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088', 10.00000000, 2.00000000, NOW());
/// ```
pub async fn insert_spendable(pool: DbPool, spendable: &Spendable) -> Result<bool, PoolError> {
    let client = pool.get().await?;
    let stmt = client.prepare("INSERT INTO spendable (spender, channel_id, total, still_on_create2, created) values ($1, $2, $3, $4, $5)").await?;

    let row = client
        .execute(
            &stmt,
            &[
                &spendable.spender,
                &spendable.channel.id(),
                &spendable.deposit.total,
                &spendable.deposit.still_on_create2,
                &Utc::now(),
            ],
        )
        .await?;

    let is_inserted = row == 1;
    Ok(is_inserted)
}

/// ```text
/// SELECT spender, total, still_on_create2, channels.leader, channels.follower, channels.guardian, channels.token, channels.nonce FROM spendable INNER JOIN channels ON channels.id = spendable.channel_id WHERE spender = $1 AND channel_id = $2
/// ```
pub async fn fetch_spendable(
    pool: DbPool,
    spender: &Address,
    channel_id: &ChannelId,
) -> Result<Option<Spendable>, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare("SELECT spender, total, still_on_create2, spendable.created, channels.leader, channels.follower, channels.guardian, channels.token, channels.nonce FROM spendable INNER JOIN channels ON channels.id = spendable.channel_id WHERE spender = $1 AND channel_id = $2").await?;

    let row = client.query_opt(&statement, &[spender, channel_id]).await?;

    Ok(row.as_ref().map(Spendable::from))
}

pub async fn get_all_spendables_for_channel(
    pool: DbPool,
    channel_id: &ChannelId,
    skip: u64,
    limit: u64,
) -> Result<(Vec<Spendable>, Pagination), PoolError> {
    let client = pool.get().await?;
    let query = format!("SELECT spender, total, still_on_create2, spendable.created, channels.leader, channels.follower, channels.guardian, channels.token, channels.nonce FROM spendable INNER JOIN channels ON channels.id = spendable.channel_id WHERE channel_id = $1 ORDER BY spendable.created ASC LIMIT {} OFFSET {}", limit, skip);

    let statement = client.prepare(&query).await?;

    let rows = client.query(&statement, &[channel_id]).await?;
    let spendables = rows.iter().map(Spendable::from).collect();

    let total_count = list_spendable_total_count(&pool, channel_id).await?;

    // fast ceil for total_pages
    let total_pages = if total_count == 0 {
        1
    } else {
        1 + ((total_count - 1) / limit as u64)
    };

    let pagination = Pagination {
        total_pages,
        page: skip / limit as u64,
    };

    Ok((spendables, pagination))
}

static UPDATE_SPENDABLE_STATEMENT: &str = "WITH inserted_spendable AS (INSERT INTO spendable(spender, channel_id, total, still_on_create2, created) VALUES($1, $2, $3, $4, $5) ON CONFLICT ON CONSTRAINT spendable_pkey DO UPDATE SET total = $3, still_on_create2 = $4 WHERE spendable.spender = $1 AND spendable.channel_id = $2 RETURNING *) SELECT inserted_spendable.*, channels.leader, channels.follower, channels.guardian, channels.token, channels.nonce FROM inserted_spendable INNER JOIN channels ON inserted_spendable.channel_id = channels.id";

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
                &spendable.deposit.total,
                &spendable.deposit.still_on_create2,
                &Utc::now(),
            ],
        )
        .await?;

    Ok(Spendable::from(&row))
}

async fn list_spendable_total_count<'a>(
    pool: &DbPool,
    channel_id: &ChannelId,
) -> Result<u64, PoolError> {
    let client = pool.get().await?;

    let statement = "SELECT COUNT(spendable)::varchar FROM spendable WHERE channel_id = $1";
    let stmt = client.prepare(statement).await?;
    let row = client.query_one(&stmt, &[&channel_id]).await?;

    Ok(row.get::<_, TotalCount>(0).0)
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use primitives::{
        spender::Spendable,
        test_util::{ADVERTISER, CREATOR, FOLLOWER, GUARDIAN, GUARDIAN_2, PUBLISHER},
        util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN},
        Deposit, UnifiedNum,
    };

    use crate::db::{
        insert_channel,
        tests_postgres::{setup_test_migrations, DATABASE_POOL},
    };
    use tokio::time::{sleep, Duration};

    use super::*;

    #[tokio::test]
    async fn it_inserts_and_fetches_and_updates_spendable() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let spendable = Spendable {
            spender: ADDRESSES["user"],
            channel: DUMMY_CAMPAIGN.channel,
            deposit: Deposit {
                total: UnifiedNum::from(100_000_000),
                still_on_create2: UnifiedNum::from(500_000),
            },
        };

        insert_channel(&database.pool, spendable.channel)
            .await
            .expect("Should insert Channel before creating spendable");

        let is_inserted = insert_spendable(database.pool.clone(), &spendable)
            .await
            .expect("Should succeed");
        sleep(Duration::from_millis(100)).await;

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

    fn new_spendable_with(spender: &Address) -> Spendable {
        Spendable {
            spender: *spender,
            channel: DUMMY_CAMPAIGN.channel,
            deposit: Deposit {
                total: UnifiedNum::from(100_000_000),
                still_on_create2: UnifiedNum::from(500_000),
            },
        }
    }

    #[tokio::test]
    async fn insert_and_get_single_spendable_for_channel() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let channel = DUMMY_CAMPAIGN.channel;

        insert_channel(&database, channel)
            .await
            .expect("Should insert");

        // Test for 0 records
        let (spendables, pagination) =
            get_all_spendables_for_channel(database.clone(), &channel.id(), 0, 2)
                .await
                .expect("should get result");
        assert!(spendables.is_empty());
        assert_eq!(pagination.total_pages, 1);

        // Test for 1 pages
        let spendable_user = new_spendable_with(&FOLLOWER);

        insert_spendable(database.pool.clone(), &spendable_user)
            .await
            .expect("should insert spendable");
        sleep(Duration::from_millis(100)).await;

        let (spendables, pagination) =
            get_all_spendables_for_channel(database.clone(), &channel.id(), 0, 2)
                .await
                .expect("should get result");
        let expected_spendables = vec![spendable_user.clone()];
        let expected_pagination = Pagination {
            page: 0,
            total_pages: 1,
        };
        pretty_assertions::assert_eq!(spendables, expected_spendables);
        pretty_assertions::assert_eq!(pagination, expected_pagination);
    }

    #[tokio::test]
    async fn gets_multiple_pages_of_spendables_for_channel() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let channel = DUMMY_CAMPAIGN.channel;

        insert_channel(&database, channel)
            .await
            .expect("Should insert");

        let create_spendables: Vec<(Address, Spendable)> = vec![
            (*PUBLISHER, new_spendable_with(&PUBLISHER)),
            (*ADVERTISER, new_spendable_with(&ADVERTISER)),
            (*CREATOR, new_spendable_with(&CREATOR)),
            (*GUARDIAN, new_spendable_with(&GUARDIAN)),
            (*GUARDIAN_2, new_spendable_with(&GUARDIAN_2)),
        ];

        // insert all spendables
        for (address, spendable) in create_spendables.iter() {
            insert_spendable(database.pool.clone(), spendable)
                .await
                .expect(&format!(
                    "Failed to insert spendable for {:?} with Spendable: {:?}",
                    address, spendable
                ));
            // use sleep to make all spendables with different time
            // they will follow the order in which they were defined in the variable
            sleep(Duration::from_millis(100)).await;
        }

        let spendables = create_spendables.into_iter().collect::<HashMap<_, _>>();

        let expected_pages = vec![
            (
                vec![&spendables[&PUBLISHER], &spendables[&ADVERTISER]],
                Pagination {
                    page: 0,
                    total_pages: 3,
                },
            ),
            (
                vec![&spendables[&CREATOR], &spendables[&GUARDIAN]],
                Pagination {
                    page: 1,
                    total_pages: 3,
                },
            ),
            (
                vec![&spendables[&GUARDIAN_2]],
                Pagination {
                    page: 2,
                    total_pages: 3,
                },
            ),
        ];

        for (expected_spendables, expected_pagination) in expected_pages.iter() {
            // for page = 0; skip = 0
            // for page = 1; skip = 2
            // etc.
            let limit = 2;
            let skip = expected_pagination.page * limit;

            let debug_msg = format!(
                "{:?} page = {} with skip = {} & limit = {}",
                channel.id(),
                expected_pagination.page,
                skip,
                limit
            );

            let (spendables, pagination) =
                get_all_spendables_for_channel(database.clone(), &channel.id(), skip, limit)
                    .await
                    .expect(&format!("could not fetch spendables {}", debug_msg));

            pretty_assertions::assert_eq!(
                &pagination,
                expected_pagination,
                "Unexpected pagination for {}",
                debug_msg
            );
            pretty_assertions::assert_eq!(&spendables, expected_spendables);
        }
    }
}
