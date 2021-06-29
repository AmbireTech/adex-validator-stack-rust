use chrono::{DateTime, Utc};
use futures::pin_mut;
use primitives::{
    sentry::{EventAggregate, MessageResponse},
    validator::{ApproveState, Heartbeat, NewState},
    Address, BigNum, Channel, ChannelId, ValidatorId,
};
use std::{convert::TryFrom, ops::Add};
use tokio_postgres::{
    binary_copy::BinaryCopyInWriter,
    types::{ToSql, Type},
};

use super::{DbPool, PoolError};

pub async fn latest_approve_state(
    pool: &DbPool,
    channel: &Channel,
) -> Result<Option<MessageResponse<ApproveState>>, PoolError> {
    let client = pool.get().await?;

    let select = client.prepare("SELECT \"from\", msg, received FROM validator_messages WHERE channel_id = $1 AND \"from\" = $2 AND msg ->> 'type' = 'ApproveState' ORDER BY received DESC LIMIT 1").await?;
    let rows = client
        .query(
            &select,
            &[&channel.id, &channel.spec.validators.follower().id],
        )
        .await?;

    rows.get(0)
        .map(MessageResponse::<ApproveState>::try_from)
        .transpose()
        .map_err(PoolError::Backend)
}

pub async fn latest_new_state(
    pool: &DbPool,
    channel: &Channel,
    state_root: &str,
) -> Result<Option<MessageResponse<NewState>>, PoolError> {
    let client = pool.get().await?;

    let select = client.prepare("SELECT \"from\", msg, received FROM validator_messages WHERE channel_id = $1 AND \"from\" = $2 AND msg ->> 'type' = 'NewState' AND msg->> 'stateRoot' = $3 ORDER BY received DESC LIMIT 1").await?;
    let rows = client
        .query(
            &select,
            &[
                &channel.id,
                &channel.spec.validators.leader().id,
                &state_root,
            ],
        )
        .await?;

    rows.get(0)
        .map(MessageResponse::<NewState>::try_from)
        .transpose()
        .map_err(PoolError::Backend)
}

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

pub async fn list_event_aggregates(
    pool: &DbPool,
    channel_id: &ChannelId,
    limit: u32,
    from: &Option<ValidatorId>,
    after: &Option<DateTime<Utc>>,
) -> Result<Vec<EventAggregate>, PoolError> {
    let client = pool.get().await?;

    let (mut where_clauses, mut params) = (vec![], Vec::<&(dyn ToSql + Sync)>::new());
    let id = channel_id.to_string();
    params.push(&id);
    where_clauses.push(format!("channel_id = ${}", params.len()));

    if let Some(from) = from {
        where_clauses.push(format!("earner = '{}'", from.to_string()));
        params.push(&"IMPRESSION");
        where_clauses.push(format!("event_type = ${}", params.len()));
    } else {
        where_clauses.push("earner is NOT NULL".to_string());
    }

    if let Some(after) = after {
        params.push(after);
        where_clauses.push(format!("created > ${}", params.len()));
    }

    let where_clause = if !where_clauses.is_empty() {
        where_clauses.join(" AND ").to_string()
    } else {
        "".to_string()
    };
    let statement = format!(
                    "
                        WITH aggregates AS (
                            SELECT
                                channel_id,
                                created,
                                event_type,
                                jsonb_build_object(
                                    'eventCounts',
                                    jsonb_object_agg(
                                        jsonb_build_object(
                                            earner, count
                                        )
                                    ),
                                    'eventPayouts',
                                    jsonb_object_agg(
                                        jsonb_build_object(
                                            earner, payout
                                        )
                                    )
                                )
                                as data
                            FROM event_aggregates WHERE {} GROUP BY channel_id, event_type, created ORDER BY created DESC LIMIT {}
                        ) SELECT channel_id, created, jsonb_object_agg(event_type , data) as events FROM aggregates GROUP BY channel_id, created
                    ", where_clause, limit);

    let stmt = client.prepare(&statement).await?;
    let rows = client.query(&stmt, params.as_slice()).await?;

    let event_aggregates = rows.iter().map(EventAggregate::from).collect();

    Ok(event_aggregates)
}

#[derive(Debug)]
struct EventData {
    id: ChannelId,
    event_type: String,
    earner: Option<Address>,
    event_count: BigNum,
    event_payout: BigNum,
}

pub async fn insert_event_aggregate(
    pool: &DbPool,
    channel_id: &ChannelId,
    event: &EventAggregate,
) -> Result<bool, PoolError> {
    let mut data: Vec<EventData> = Vec::new();

    for (event_type, aggr) in &event.events {
        if let Some(event_counts) = &aggr.event_counts {
            let mut total_event_counts: BigNum = 0.into();
            let mut total_event_payouts: BigNum = 0.into();
            for (earner, event_count) in event_counts {
                let event_payout = aggr.event_payouts[earner].clone();

                data.push(EventData {
                    id: channel_id.to_owned(),
                    event_type: event_type.clone(),
                    earner: Some(*earner),
                    event_count: event_count.to_owned(),
                    event_payout: event_payout.clone(),
                });

                // total sum
                total_event_counts = event_count.add(&total_event_counts);
                total_event_payouts = event_payout.add(total_event_payouts);
            }

            data.push(EventData {
                id: channel_id.to_owned(),
                event_type: event_type.clone(),
                earner: None,
                event_count: total_event_counts,
                event_payout: total_event_payouts,
            });
        }
    }

    let client = pool.get().await?;

    let mut err: Option<tokio_postgres::Error> = None;
    let sink = client.copy_in("COPY event_aggregates(channel_id, created, event_type, count, payout, earner) FROM STDIN BINARY").await?;

    let created = Utc::now(); // time discrepancy

    let writer = BinaryCopyInWriter::new(
        sink,
        &[
            Type::VARCHAR,
            Type::TIMESTAMPTZ,
            Type::VARCHAR,
            Type::VARCHAR,
            Type::VARCHAR,
            Type::VARCHAR,
        ],
    );
    pin_mut!(writer);
    for item in data {
        if let Err(e) = writer
            .as_mut()
            .write(&[
                &item.id,
                &created,
                &item.event_type,
                &item.event_count,
                &item.event_payout,
                &item.earner,
            ])
            .await
        {
            err = Some(e);
            break;
        }
    }

    match err {
        Some(e) => Err(PoolError::Backend(e)),
        None => {
            writer.finish().await?;
            Ok(true)
        }
    }
}
