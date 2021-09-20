use chrono::Utc;
use primitives::{targeting::Rules, validator::MessageTypes, Channel, ChannelId, ValidatorId};
use std::str::FromStr;

pub use list_channels::list_channels;

use super::{DbPool, PoolError};

pub async fn get_channel_by_id(
    pool: &DbPool,
    id: &ChannelId,
) -> Result<Option<Channel>, PoolError> {
    let client = pool.get().await?;

    let select = client.prepare("SELECT id, creator, deposit_asset, deposit_amount, valid_until, targeting_rules, spec, exhausted FROM channels WHERE id = $1 LIMIT 1").await?;

    let results = client.query(&select, &[&id]).await?;

    Ok(results.get(0).map(Channel::from))
}

pub async fn get_channel_by_id_and_validator(
    pool: &DbPool,
    id: &ChannelId,
    validator_id: &ValidatorId,
) -> Result<Option<Channel>, PoolError> {
    let client = pool.get().await?;

    let validator = serde_json::Value::from_str(&format!(r#"[{{"id": "{}"}}]"#, validator_id))
        .expect("Not a valid json");
    let query = "SELECT id, creator, deposit_asset, deposit_amount, valid_until, targeting_rules, spec, exhausted FROM channels WHERE id = $1 AND spec->'validators' @> $2 LIMIT 1";
    let select = client.prepare(query).await?;

    let results = client.query(&select, &[&id, &validator]).await?;

    Ok(results.get(0).map(Channel::from))
}

pub async fn insert_channel(pool: &DbPool, channel: &Channel) -> Result<bool, PoolError> {
    let client = pool.get().await?;

    let stmt = client.prepare("INSERT INTO channels (id, creator, deposit_asset, deposit_amount, valid_until, targeting_rules, spec, exhausted) values ($1, $2, $3, $4, $5, $6, $7, $8)").await?;

    let row = client
        .execute(
            &stmt,
            &[
                &channel.id,
                &channel.creator,
                &channel.deposit_asset,
                &channel.deposit_amount,
                &channel.valid_until,
                &channel.targeting_rules,
                &channel.spec,
                &channel.exhausted,
            ],
        )
        .await?;

    let inserted = row == 1;
    Ok(inserted)
}

#[deprecated(note = "AIP#61 now uses the modify Campaign route for updating targeting rules")]
pub async fn update_targeting_rules(
    pool: &DbPool,
    channel_id: &ChannelId,
    targeting_rules: &Rules,
) -> Result<bool, PoolError> {
    let client = pool.get().await?;

    let stmt = client
        .prepare("UPDATE channels SET targeting_rules=$1 WHERE id=$2")
        .await?;
    let row = client
        .execute(&stmt, &[&targeting_rules, &channel_id])
        .await?;

    let updated = row == 1;
    Ok(updated)
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
            &[&channel.id, &from, &validator_message, &Utc::now()],
        )
        .await?;

    let inserted = row == 1;
    Ok(inserted)
}

#[deprecated = "No longer needed for V5"]
pub async fn update_exhausted_channel(
    pool: &DbPool,
    channel: &Channel,
    index: u32,
) -> Result<bool, PoolError> {
    let client = pool.get().await?;

    let stmt = client
        .prepare("UPDATE channels SET exhausted[$1] = true WHERE id = $2")
        .await?;
    // WARNING: By default PostgreSQL uses a one-based numbering convention for arrays, that is, an array of n elements starts with array[1] and ends with array[n].
    // this is why we add +1 to the index
    let row = client.execute(&stmt, &[&(index + 1), &channel.id]).await?;

    let updated = row == 1;
    Ok(updated)
}

mod list_channels {
    use chrono::{DateTime, Utc};
    use primitives::{sentry::ChannelListResponse, Channel, ValidatorId};
    use std::str::FromStr;
    use tokio_postgres::types::ToSql;

    use crate::db::{DbPool, PoolError, TotalCount};

    pub async fn list_channels(
        pool: &DbPool,
        skip: u64,
        limit: u32,
        creator: &Option<String>,
        validator: &Option<ValidatorId>,
        valid_until_ge: &DateTime<Utc>,
    ) -> Result<ChannelListResponse, PoolError> {
        let client = pool.get().await?;

        let validator = validator.as_ref().map(|validator_id| {
            serde_json::Value::from_str(&format!(r#"[{{"id": "{}"}}]"#, validator_id))
                .expect("Not a valid json")
        });
        let (where_clauses, params) =
            channel_list_query_params(creator, validator.as_ref(), valid_until_ge);
        let total_count_params = (where_clauses.clone(), params.clone());

        // To understand why we use Order by, see Postgres Documentation: https://www.postgresql.org/docs/8.1/queries-limit.html
        let statement = format!("SELECT id, creator, deposit_asset, deposit_amount, valid_until, targeting_rules, spec, exhausted FROM channels WHERE {} ORDER BY spec->>'created' DESC LIMIT {} OFFSET {}", where_clauses.join(" AND "), limit, skip);
        let stmt = client.prepare(&statement).await?;

        let rows = client.query(&stmt, params.as_slice()).await?;
        let channels = rows.iter().map(Channel::from).collect();

        let total_count =
            list_channels_total_count(pool, (&total_count_params.0, total_count_params.1)).await?;

        // fast ceil for total_pages
        let total_pages = if total_count == 0 {
            1
        } else {
            1 + ((total_count - 1) / limit as u64)
        };

        Ok(ChannelListResponse {
            total_pages,
            total: total_pages,
            page: skip / limit as u64,
            channels,
        })
    }

    async fn list_channels_total_count<'a>(
        pool: &DbPool,
        (where_clauses, params): (&'a [String], Vec<&'a (dyn ToSql + Sync)>),
    ) -> Result<u64, PoolError> {
        let client = pool.get().await?;

        let statement = format!(
            "SELECT COUNT(id)::varchar FROM channels WHERE {}",
            where_clauses.join(" AND ")
        );
        let stmt = client.prepare(&statement).await?;
        let row = client.query_one(&stmt, params.as_slice()).await?;

        Ok(row.get::<_, TotalCount>(0).0)
    }

    fn channel_list_query_params<'a>(
        creator: &'a Option<String>,
        validator: Option<&'a serde_json::Value>,
        valid_until_ge: &'a DateTime<Utc>,
    ) -> (Vec<String>, Vec<&'a (dyn ToSql + Sync)>) {
        let mut where_clauses = vec!["valid_until >= $1".to_string()];
        let mut params: Vec<&(dyn ToSql + Sync)> = vec![valid_until_ge];

        if let Some(creator) = creator {
            where_clauses.push(format!("creator = ${}", params.len() + 1));
            params.push(creator);
        }

        if let Some(validator) = validator {
            where_clauses.push(format!("spec->'validators' @> ${}", params.len() + 1));
            params.push(validator);
        }

        (where_clauses, params)
    }
}
