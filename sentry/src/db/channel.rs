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
    use futures::{pin_mut, TryStreamExt};
    use primitives::{
        sentry::{channel_list::ChannelListResponse, Pagination},
        ChainId, Channel, ValidatorId,
    };
    use tokio_postgres::{types::ToSql, Row};

    use crate::db::{DbPool, PoolError, TotalCount};

    /// Lists the `Channel`s in `ASC` order.
    ///
    /// This makes sure that if a new [`Channel`] is added
    /// while we are scrolling through the pages it will not alter the [`Channel`]s ordering.
    pub async fn list_channels(
        pool: &DbPool,
        skip: u64,
        limit: u32,
        validator: Option<ValidatorId>,
        chains: &[ChainId],
    ) -> Result<ChannelListResponse, PoolError> {
        let client = pool.get().await?;

        let mut where_clauses = vec![];
        let mut params: Vec<Box<(dyn ToSql + Send + Sync)>> = vec![];
        let mut params_total: Vec<Box<(dyn ToSql + Send + Sync)>> = vec![];

        if !chains.is_empty() {
            let (chain_params, chain_params_total): (
                Vec<Box<dyn ToSql + Send + Sync>>,
                Vec<Box<dyn ToSql + Send + Sync>>,
            ) = chains
                .iter()
                .map(|chain_id| (Box::new(*chain_id) as _, Box::new(*chain_id) as _))
                .unzip();

            // prepare the query parameters, they are 1-indexed!
            let params_prepared = (1..=chain_params.len())
                .map(|param_num| format!("${param_num}"))
                .collect::<Vec<_>>()
                .join(",");

            params.extend(chain_params);
            params_total.extend(chain_params_total);

            where_clauses.push(format!("chain_id IN ({})", params_prepared));
        }

        match validator {
            Some(validator) => {
                // params are 1-indexed
                where_clauses.push(format!(
                    "(leader = ${validator_param} OR follower = ${validator_param})",
                    validator_param = params.len() + 1
                ));
                // then add the new param to the list!
                params.push(Box::new(validator) as _);
                params_total.push(Box::new(validator) as _);
            }
            _ => {}
        }

        // To understand why we use Order by, see Postgres Documentation: https://www.postgresql.org/docs/8.1/queries-limit.html
        let statement = if !where_clauses.is_empty() {
            format!("SELECT id, leader, follower, guardian, token, nonce, created FROM channels WHERE {} ORDER BY created ASC LIMIT {} OFFSET {}",
where_clauses.join(" AND "), limit, skip)
        } else {
            format!("SELECT id, leader, follower, guardian, token, nonce, created FROM channels ORDER BY created ASC LIMIT {} OFFSET {}",
limit, skip)
        };

        let stmt = client.prepare(&statement).await?;

        let rows: Vec<Row> = client.query_raw(&stmt, params).await?.try_collect().await?;

        let channels = rows.iter().map(Channel::from).collect();

        let total_count = list_channels_total_count(pool, (where_clauses, params_total)).await?;

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
        (where_clauses, params): (Vec<String>, Vec<Box<dyn ToSql + Send + Sync>>),
    ) -> Result<u64, PoolError> {
        let client = pool.get().await?;

        let statement = if !where_clauses.is_empty() {
            format!(
                "SELECT COUNT(id)::varchar FROM channels WHERE {}",
                where_clauses.join(" AND ")
            )
        } else {
            format!("SELECT COUNT(id)::varchar FROM channels")
        };

        let stmt = client.prepare(&statement).await?;

        let stream = client.query_raw(&stmt, params).await?;
        pin_mut!(stream);
        let row = stream
            .try_next()
            .await?
            .expect("Query should always return exactly 1 row!");

        Ok(row.get::<_, TotalCount>(0).0)
    }
}

#[cfg(test)]
mod test {
    use adapter::ethereum::test_util::{GANACHE_1, GANACHE_INFO_1};
    use primitives::{
        config::GANACHE_CONFIG, sentry::Pagination, test_util::DUMMY_CAMPAIGN, ChainOf, Channel,
    };

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

        let channel_1337 = GANACHE_CONFIG
            .find_chain_of(DUMMY_CAMPAIGN.channel.token)
            .expect("Channel token should be whitelisted in config!")
            .with_channel(DUMMY_CAMPAIGN.channel);

        let channel_1 = {
            let token_info = GANACHE_INFO_1.tokens["Mocked TOKEN 1"].clone();

            let channel_1 = Channel {
                token: token_info.address,
                ..DUMMY_CAMPAIGN.channel
            };

            ChainOf::new(GANACHE_1.clone(), token_info).with_channel(channel_1)
        };

        assert_ne!(
            channel_1337.chain.chain_id, channel_1.chain.chain_id,
            "The two channels should be on different Chains!"
        );

        // Insert channel on chain #1
        insert_channel(&database.pool, &channel_1)
            .await
            .expect("Should insert Channel");

        // try to insert the same channel twice
        let actual_channel_1337 = {
            let insert = insert_channel(&database.pool, &channel_1337)
                .await
                .expect("Should insert Channel");

            // once inserted, the channel should only be returned by the function
            let only_select = insert_channel(&database.pool, &channel_1337)
                .await
                .expect("Should run insert with RETURNING on the Channel");

            assert_eq!(insert, only_select);

            only_select
        };

        // List Channels with Chain #1337
        {
            // Check the response using only that channel's ChainId
            let response =
                list_channels(&database.pool, 0, 10, None, &[channel_1337.chain.chain_id])
                    .await
                    .expect("Should list Channels");

            assert_eq!(1, response.channels.len());
            assert_eq!(
                response.channels[0], actual_channel_1337,
                "Only the single channel of Chain #1337 should be returned"
            );
        }

        // Cist channels with Chain #1 and Chain #1337
        {
            let response = list_channels(
                &database.pool,
                0,
                10,
                None,
                &[channel_1337.chain.chain_id, channel_1.chain.chain_id],
            )
            .await
            .expect("Should list Channels");

            assert_eq!(2, response.channels.len());
            assert_eq!(
                Pagination {
                    total_pages: 1,
                    page: 0,
                },
                response.pagination
            );
            pretty_assertions::assert_eq!(
                response.channels,
                vec![channel_1.context, actual_channel_1337],
                "All channels in ASC order should be returned"
            );
        }
    }
}
