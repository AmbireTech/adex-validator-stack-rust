use chrono::Utc;
use primitives::{validator::MessageTypes, Channel, ChannelId, ValidatorId};

pub use list_channels::list_channels;

use super::{DbPool, PoolError};

pub async fn get_channel_by_id(
    pool: &DbPool,
    id: &ChannelId,
) -> Result<Option<Channel>, PoolError> {
    let client = pool.get().await?;

    let select = client
        .prepare(
            "SELECT leader, follower, guardian, token, nonce FROM channels WHERE id = $1 LIMIT 1",
        )
        .await?;

    let row = client.query_opt(&select, &[&id]).await?;

    Ok(row.as_ref().map(Channel::from))
}

pub async fn get_channel_by_id_and_validator(
    pool: &DbPool,
    id: ChannelId,
    validator: ValidatorId,
) -> Result<Option<Channel>, PoolError> {
    let client = pool.get().await?;

    let query = "SELECT leader, follower, guardian, token, nonce FROM channels WHERE id = $1 AND (leader = $2 OR follower = $2) LIMIT 1";
    let select = client.prepare(query).await?;

    let row = client.query_opt(&select, &[&id, &validator]).await?;

    Ok(row.as_ref().map(Channel::from))
}

/// Used to insert/get Channel when creating a Campaign
/// If channel already exists it will return it instead.
/// This call should never trigger a `SqlState::UNIQUE_VIOLATION`
///
/// ```sql
/// INSERT INTO channels (id, leader, follower, guardian, token, nonce, created)
/// VALUES ($1, $2, $3, $4, $5, $6, NOW())
/// ON CONFLICT ON CONSTRAINT channels_pkey DO UPDATE SET created=EXCLUDED.created
/// RETURNING leader, follower, guardian, token, nonce
/// ```
pub async fn insert_channel(pool: &DbPool, channel: Channel) -> Result<Channel, PoolError> {
    let client = pool.get().await?;

    // We use `EXCLUDED.created` in order to have to DO UPDATE otherwise it does not return the fields
    // when there is a CONFLICT
    let stmt = client.prepare("INSERT INTO channels (id, leader, follower, guardian, token, nonce, created) VALUES ($1, $2, $3, $4, $5, $6, NOW())
  ON CONFLICT ON CONSTRAINT channels_pkey DO UPDATE SET created=EXCLUDED.created RETURNING leader, follower, guardian, token, nonce").await?;

    let row = client
        .query_one(
            &stmt,
            &[
                &channel.id(),
                &channel.leader,
                &channel.follower,
                &channel.guardian,
                &channel.token,
                &channel.nonce,
            ],
        )
        .await?;

    Ok(Channel::from(&row))
}

pub async fn insert_validator_messages(
    pool: &DbPool,
    channel: &Channel,
    from: &ValidatorId,
    validator_message: &MessageTypes,
) -> Result<bool, PoolError> {
    let client = pool.get().await?;

    let stmt = client.prepare("INSERT INTO validator_messages (channel_id, \"from\", msg, received) values ($1, $2, $3, $4)").await?;

    let row = client
        .execute(
            &stmt,
            &[&channel.id(), &from, &validator_message, &Utc::now()],
        )
        .await?;

    let inserted = row == 1;
    Ok(inserted)
}

mod list_channels {
    use primitives::{
        channel_v5::Channel,
        sentry::{channel_list::ChannelListResponse, Pagination},
        ValidatorId,
    };
    use std::str::FromStr;
    use tokio_postgres::types::{accepts, FromSql, Type};

    use crate::db::{DbPool, PoolError};

    struct TotalCount(pub u64);
    impl<'a> FromSql<'a> for TotalCount {
        fn from_sql(
            ty: &Type,
            raw: &'a [u8],
        ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
            let str_slice = <&str as FromSql>::from_sql(ty, raw)?;

            Ok(Self(u64::from_str(str_slice)?))
        }

        // Use a varchar or text, since otherwise `int8` fails deserialization
        accepts!(VARCHAR, TEXT);
    }

    /// Lists the `Channel`s in `ASC` order.
    /// This makes sure that if a new `Channel` is added
    // while we are scrolling through the pages it will not alter the `Channel`s ordering
    pub async fn list_channels(
        pool: &DbPool,
        skip: u64,
        limit: u32,
        validator: Option<ValidatorId>,
    ) -> Result<ChannelListResponse, PoolError> {
        let client = pool.get().await?;

        // To understand why we use Order by, see Postgres Documentation: https://www.postgresql.org/docs/8.1/queries-limit.html
        let rows = match validator {
            Some(validator) => {
                let where_clause = "(leader = $1 OR follower = $1)".to_string();

                let statement = format!("SELECT leader, follower, guardian, token, nonce, created FROM channels WHERE {} ORDER BY created ASC LIMIT {} OFFSET {}",
        where_clause, limit, skip);
                let stmt = client.prepare(&statement).await?;

                client.query(&stmt, &[&validator.to_string()]).await?
            }
            None => {
                let statement = format!("SELECT id, leader, follower, guardian, token, nonce, created FROM channels ORDER BY created ASC LIMIT {} OFFSET {}",
        limit, skip);
                let stmt = client.prepare(&statement).await?;

                client.query(&stmt, &[]).await?
            }
        };

        let channels = rows.iter().map(Channel::from).collect();

        let total_count = list_channels_total_count(pool, validator).await?;

        // fast ceil for total_pages
        let total_pages = if total_count == 0 {
            1
        } else {
            1 + ((total_count - 1) / limit as u64)
        };

        Ok(ChannelListResponse {
            channels,
            pagination: Pagination {
                total_pages,
                total: total_pages,
                page: skip / limit as u64,
            },
        })
    }

    async fn list_channels_total_count<'a>(
        pool: &DbPool,
        validator: Option<ValidatorId>,
    ) -> Result<u64, PoolError> {
        let client = pool.get().await?;

        let row = match validator {
            Some(validator) => {
                let where_clause = "(leader = $1 OR follower = $1)".to_string();

                let statement = format!(
                    "SELECT COUNT(id)::varchar FROM channels WHERE {}",
                    where_clause
                );
                let stmt = client.prepare(&statement).await?;

                client.query_one(&stmt, &[&validator.to_string()]).await?
            }
            None => {
                let statement = "SELECT COUNT(id)::varchar FROM channels";
                let stmt = client.prepare(statement).await?;

                client.query_one(&stmt, &[]).await?
            }
        };

        Ok(row.get::<_, TotalCount>(0).0)
    }
}

#[cfg(test)]
mod test {
    use primitives::util::tests::prep_db::DUMMY_CAMPAIGN;

    use crate::db::{
        insert_channel,
        tests_postgres::{setup_test_migrations, DATABASE_POOL},
    };

    use super::list_channels::list_channels;

    #[tokio::test]
    async fn insert_and_list_channels_return_channels() {
        let database = DATABASE_POOL.get().await.expect("Should get database");
        setup_test_migrations(database.pool.clone())
            .await
            .expect("Should setup migrations");

        let actual_channel = {
            let insert = insert_channel(&database.pool, DUMMY_CAMPAIGN.channel)
                .await
                .expect("Should insert Channel");

            // once inserted, the channel should only be returned by the function
            let only_select = insert_channel(&database.pool, DUMMY_CAMPAIGN.channel)
                .await
                .expect("Should run insert with RETURNING on the Channel");

            assert_eq!(insert, only_select);

            only_select
        };

        let response = list_channels(&database.pool, 0, 10, None)
            .await
            .expect("Should list Channels");

        assert_eq!(1, response.channels.len());
        assert_eq!(DUMMY_CAMPAIGN.channel, actual_channel);
    }
}
