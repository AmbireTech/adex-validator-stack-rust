use primitives::{ChainOf, Channel, ChannelId};

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

/// Used to insert/get Channel when creating a Campaign
/// If channel already exists it will return it instead.
/// This call should never trigger a `SqlState::UNIQUE_VIOLATION`
///
/// ```sql
/// INSERT INTO channels (id, leader, follower, guardian, token, nonce, chain_id, created)
/// VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
/// ON CONFLICT ON CONSTRAINT channels_pkey DO UPDATE SET created=EXCLUDED.created
/// RETURNING leader, follower, guardian, token, nonce
/// ```
pub async fn insert_channel(
    pool: &DbPool,
    channel_chain: &ChainOf<Channel>,
) -> Result<Channel, PoolError> {
    let client = pool.get().await?;
    let chain_id = channel_chain.chain.chain_id;
    let channel = channel_chain.context;

    // We use `EXCLUDED.created` in order to have to DO UPDATE otherwise it does not return the fields
    // when there is a CONFLICT
    let stmt = client.prepare("INSERT INTO channels (id, leader, follower, guardian, token, nonce, chain_id, created) VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
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
                &chain_id,
            ],
        )
        .await?;

    Ok(Channel::from(&row))
}

mod list_channels {
    use primitives::{
        sentry::{
            channel_list::ChannelListResponse,
            Pagination,
        },
        Channel, ChainId, ValidatorId,
    };

    use crate::db::{DbPool, PoolError, TotalCount};

    /// Lists the `Channel`s in `ASC` order.
    /// This makes sure that if a new `Channel` is added
    // while we are scrolling through the pages it will not alter the `Channel`s ordering
    pub async fn list_channels(
        pool: &DbPool,
        skip: u64,
        limit: u32,
        validator: Option<ValidatorId>,
        chains: &[ChainId],
    ) -> Result<ChannelListResponse, PoolError> {
        let client = pool.get().await?;
        let mut where_clauses = vec![];
        if !chains.is_empty() {
            where_clauses.push(format!(
                "chain_id IN ({})",
            chains
                    .iter()
                    .map(|id| id.to_u32().to_string())
                    .collect::<Vec<String>>()
                    .join(",")
            ));
        }

        // To understand why we use Order by, see Postgres Documentation: https://www.postgresql.org/docs/8.1/queries-limit.html
        let rows = match validator {
            Some(validator) => {
                where_clauses.push("(leader = $1 OR follower = $1)".to_string());

                let statement = format!("SELECT leader, follower, guardian, token, nonce, created FROM channels WHERE {} ORDER BY created ASC LIMIT {} OFFSET {}",
        where_clauses.join(" AND "), limit, skip);
                let stmt = client.prepare(&statement).await?;

                client.query(&stmt, &[&validator.to_string()]).await?
            }
            None => {
                let statement = if !where_clauses.is_empty() {
                    format!("SELECT id, leader, follower, guardian, token, nonce, created FROM channels WHERE {} ORDER BY created ASC LIMIT {} OFFSET {}",
        where_clauses.join(" AND "), limit, skip)
                } else {
                    format!("SELECT id, leader, follower, guardian, token, nonce, created FROM channels ORDER BY created ASC LIMIT {} OFFSET {}",
        limit, skip)
                };

                let stmt = client.prepare(&statement).await?;

                client.query(&stmt, &[]).await?
            }
        };

        let channels = rows.iter().map(Channel::from).collect();

        let total_count = list_channels_total_count(pool, validator, chains).await?;
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
                page: skip / limit as u64,
            },
        })
    }

    async fn list_channels_total_count<'a>(
        pool: &DbPool,
        validator: Option<ValidatorId>,
        chains: &[ChainId],
    ) -> Result<u64, PoolError> {
        let client = pool.get().await?;

        let mut where_clauses = vec![];
        if !chains.is_empty() {
            where_clauses.push(format!(
                "chain_id IN ({})",
                chains
                    .iter()
                    .map(|id| id.to_u32().to_string())
                    .collect::<Vec<String>>()
                    .join(",")
            ));
        }

        let row = match validator {
            Some(validator) => {
                where_clauses.push("(leader = $1 OR follower = $1)".to_string());

                let statement = format!(
                    "SELECT COUNT(id)::varchar FROM channels WHERE {}",
                    where_clauses.join(" AND ")
                );
                let stmt = client.prepare(&statement).await?;

                client.query_one(&stmt, &[&validator.to_string()]).await?
            }
            None => {
                let statement = if !where_clauses.is_empty() {
                    format!(
                        "SELECT COUNT(id)::varchar FROM channels WHERE {}",
                        where_clauses.join(" AND ")
                    )
                } else {
                    "SELECT COUNT(id)::varchar FROM channels".to_string()
                };
                let stmt = client.prepare(&statement).await?;

                client.query_one(&stmt, &[]).await?
            }
        };

        Ok(row.get::<_, TotalCount>(0).0)
    }
}

#[cfg(test)]
mod test {
    use primitives::{test_util::DUMMY_CAMPAIGN, config::GANACHE_CONFIG};

    use crate::{
        db::{
            insert_channel,
            tests_postgres::{setup_test_migrations, DATABASE_POOL},
        },
    };

    use super::list_channels::list_channels;

    #[tokio::test]
    async fn insert_and_list_channels_return_channels() {
        let database = DATABASE_POOL.get().await.expect("Should get database");
        setup_test_migrations(database.pool.clone())
            .await
            .expect("Should setup migrations");

        let channel_chain = GANACHE_CONFIG
            .find_chain_of(DUMMY_CAMPAIGN.channel.token)
            .expect("Channel token should be whitelisted in config!");
        let channel_context = channel_chain.with_channel(DUMMY_CAMPAIGN.channel);

        let actual_channel = {
            let insert = insert_channel(&database.pool, &channel_context)
                .await
                .expect("Should insert Channel");

            // once inserted, the channel should only be returned by the function
            let only_select = insert_channel(&database.pool, &channel_context)
                .await
                .expect("Should run insert with RETURNING on the Channel");

            assert_eq!(insert, only_select);

            only_select
        };

        let response = list_channels(&database.pool, 0, 10, None, &[channel_context.chain.chain_id])
            .await
            .expect("Should list Channels");

        assert_eq!(1, response.channels.len());
        assert_eq!(DUMMY_CAMPAIGN.channel, actual_channel);
    }
}
