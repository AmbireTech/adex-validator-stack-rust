use chrono::Utc;
use tokio_postgres::types::ToSql;

use primitives::{
    balances::UncheckedState,
    sentry::{MessageResponse, ValidatorMessage},
    validator::{ApproveState, Heartbeat, MessageTypes, NewState},
    Channel, ChannelId, ValidatorId,
};

use super::{DbPool, PoolError};

/// Inserts a new validator [`MessageTypes`] using the `from` [`ValidatorId`] and `received` at: [`Utc::now()`][Utc]
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

/// Retrieves [`ValidatorMessage`]s for a given [`Channel`],
/// filters them by the `message_types` and optionally,
/// filters them by the provided `from` [`ValidatorId`].
pub async fn get_validator_messages(
    pool: &DbPool,
    channel_id: &ChannelId,
    validator_id: &Option<ValidatorId>,
    message_types: &[String],
    limit: u64,
) -> Result<Vec<ValidatorMessage>, PoolError> {
    let client = pool.get().await?;

    let mut where_clauses: Vec<String> = vec!["channel_id = $1".to_string()];
    let mut params: Vec<&(dyn ToSql + Sync)> = vec![&channel_id];

    if let Some(validator_id) = validator_id {
        where_clauses.push(format!(r#""from" = ${}"#, params.len() + 1));
        params.push(validator_id);
    }

    add_message_types_params(&mut where_clauses, &mut params, message_types);

    let statement = format!(
        r#"SELECT "from", msg, received FROM validator_messages WHERE {} ORDER BY received DESC LIMIT {}"#,
        where_clauses.join(" AND "),
        limit
    );
    let select = client.prepare(&statement).await?;
    let results = client.query(&select, params.as_slice()).await?;
    let messages = results.iter().map(ValidatorMessage::from).collect();

    Ok(messages)
}

fn add_message_types_params<'a>(
    where_clauses: &mut Vec<String>,
    params: &mut Vec<&'a (dyn ToSql + Sync)>,
    message_types: &'a [String],
) {
    let mut msg_prep = vec![];
    for message_type in message_types.iter() {
        msg_prep.push(format!("${}", params.len() + 1));
        params.push(message_type);
    }

    if !msg_prep.is_empty() {
        where_clauses.push(format!("msg->>'type' IN ({})", msg_prep.join(",")));
    }
}

pub async fn latest_approve_state(
    pool: &DbPool,
    channel: &Channel,
) -> Result<Option<MessageResponse<ApproveState>>, PoolError> {
    let client = pool.get().await?;

    let select = client.prepare("SELECT \"from\", msg, received FROM validator_messages WHERE channel_id = $1 AND \"from\" = $2 AND msg ->> 'type' = 'ApproveState' ORDER BY received DESC LIMIT 1").await?;
    let rows = client
        .query(&select, &[&channel.id(), &channel.follower])
        .await?;

    rows.get(0)
        .map(MessageResponse::<ApproveState>::try_from)
        .transpose()
        .map_err(PoolError::Backend)
}

/// Returns the latest [`NewState`] message for this [`Channel`] and the provided `state_root`.
///
/// Ordered by: `received DESC`
pub async fn latest_new_state(
    pool: &DbPool,
    channel: &Channel,
    state_root: &str,
) -> Result<Option<MessageResponse<NewState<UncheckedState>>>, PoolError> {
    let client = pool.get().await?;

    let select = client.prepare("SELECT \"from\", msg, received FROM validator_messages WHERE channel_id = $1 AND \"from\" = $2 AND msg ->> 'type' = 'NewState' AND msg->> 'stateRoot' = $3 ORDER BY received DESC LIMIT 1").await?;
    let rows = client
        .query(&select, &[&channel.id(), &channel.leader, &state_root])
        .await?;

    rows.get(0)
        .map(MessageResponse::<NewState<UncheckedState>>::try_from)
        .transpose()
        .map_err(PoolError::Backend)
}

/// Returns the latest 2 [`Heartbeat`] messages for this [`Channel`] received `from` the [`ValidatorId`].
///
/// Ordered by: `received DESC`
pub async fn latest_heartbeats(
    pool: &DbPool,
    channel_id: &ChannelId,
    validator_id: &ValidatorId,
) -> Result<Vec<MessageResponse<Heartbeat>>, PoolError> {
    let client = pool.get().await?;

    let select = client.prepare("SELECT \"from\", msg, received FROM validator_messages WHERE channel_id = $1 AND \"from\" = $2 AND msg ->> 'type' = 'Heartbeat' ORDER BY received DESC LIMIT 2").await?;
    let rows = client.query(&select, &[&channel_id, &validator_id]).await?;

    rows.iter()
        .map(MessageResponse::<Heartbeat>::try_from)
        .collect::<Result<_, _>>()
        .map_err(PoolError::Backend)
}
