use crate::db::DbPool;
use bb8::RunError;
use bb8_postgres::tokio_postgres::binary_copy::BinaryCopyInWriter;
use bb8_postgres::tokio_postgres::types::{ToSql, Type};
use bb8_postgres::tokio_postgres::Error;
use chrono::{DateTime, Utc};
use futures::pin_mut;
use primitives::sentry::{
    ApproveStateValidatorMessage, EventAggregate, HeartbeatValidatorMessage,
    NewStateValidatorMessage,
};
use primitives::BigNum;
use primitives::{Channel, ChannelId, ValidatorId};
use std::ops::Add;

pub async fn latest_approve_state(
    pool: &DbPool,
    channel: &Channel,
) -> Result<Option<ApproveStateValidatorMessage>, RunError<bb8_postgres::tokio_postgres::Error>> {
    pool
        .run(move |connection| {
            async move {
                match connection.prepare("SELECT \"from\", msg, received FROM validator_messages WHERE channel_id = $1 AND \"from\" = $2 AND msg ->> 'type' = 'ApproveState' ORDER BY received DESC LIMIT 1").await {
                    Ok(select) => match connection.query(&select, &[&channel.id, &channel.spec.validators.follower().id]).await {
                        Ok(rows) => Ok((rows.get(0).map(ApproveStateValidatorMessage::from), connection)),
                        Err(e) => Err((e, connection)),
                    },
                    Err(e) => Err((e, connection)),
                }
            }
        })
        .await
}

pub async fn latest_new_state(
    pool: &DbPool,
    channel: &Channel,
    state_root: &str,
) -> Result<Option<NewStateValidatorMessage>, RunError<bb8_postgres::tokio_postgres::Error>> {
    pool
    .run(move |connection| {
        async move {
            match connection.prepare("SELECT \"from\", msg, received FROM validator_messages WHERE channel_id = $1 AND \"from\" = $2 AND msg ->> 'type' = 'NewState' AND msg->> 'stateRoot' = $3 ORDER BY received DESC LIMIT 1").await {
                Ok(select) => match connection.query(&select, &[&channel.id, &channel.spec.validators.leader().id, &state_root]).await {
                    Ok(rows) => Ok((rows.get(0).map(NewStateValidatorMessage::from), connection)),
                    Err(e) => Err((e, connection)),
                },
                Err(e) => Err((e, connection)),
            }
        }
    })
    .await
}

pub async fn latest_heartbeats(
    pool: &DbPool,
    channel_id: &ChannelId,
    validator_id: &ValidatorId,
) -> Result<Vec<HeartbeatValidatorMessage>, RunError<bb8_postgres::tokio_postgres::Error>> {
    pool
    .run(move |connection| {
        async move {
            match connection.prepare("SELECT \"from\", msg, received FROM validator_messages WHERE channel_id = $1 AND \"from\" = $2 AND msg ->> 'type' = 'Heartbeat' ORDER BY received DESC LIMIT 2").await {
                Ok(select) => match connection.query(&select, &[&channel_id, &validator_id]).await {
                    Ok(rows) => Ok((rows.iter().map(HeartbeatValidatorMessage::from).collect(), connection)),
                    Err(e) => Err((e, connection)),
                },
                Err(e) => Err((e, connection)),
            }
        }
    })
    .await
}

pub async fn list_event_aggregates(
    pool: &DbPool,
    channel_id: &ChannelId,
    limit: u32,
    from: &Option<ValidatorId>,
    after: &Option<DateTime<Utc>>,
) -> Result<Vec<EventAggregate>, RunError<bb8_postgres::tokio_postgres::Error>> {
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

    let event_aggregates = pool
        .run(move |connection| {
            async move {
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

                match connection.prepare(&statement).await {
                    Ok(stmt) => {
                        match connection.query(&stmt, params.as_slice()).await {
                            Ok(rows) => {
                                let event_aggregates = rows.iter().map(EventAggregate::from).collect();

                                Ok((event_aggregates, connection))
                            },
                            Err(e) => Err((e, connection)),
                        }
                    },
                    Err(e) => Err((e, connection)),
                }
            }
        })
        .await?;

    Ok(event_aggregates)
}

#[derive(Debug)]
struct EventData {
    id: ChannelId,
    event_type: String,
    earner: Option<ValidatorId>,
    event_count: BigNum,
    event_payout: BigNum,
}

pub async fn insert_event_aggregate(
    pool: &DbPool,
    channel_id: &ChannelId,
    event: &EventAggregate,
) -> Result<bool, RunError<bb8_postgres::tokio_postgres::Error>> {
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

    let result = pool
        .run(move |connection| {
            async move {
                let mut err: Option<Error> = None;
                let sink = match connection.copy_in("COPY event_aggregates(channel_id, created, event_type, count, payout, earner) FROM STDIN BINARY").await {
                    Ok(sink) => sink,
                    Err(e) => return Err((e, connection))
                };

                let created = Utc::now(); // time discrepancy

                let writer = BinaryCopyInWriter::new(sink, &[Type::VARCHAR, Type::TIMESTAMPTZ, Type::VARCHAR, Type::VARCHAR, Type::VARCHAR, Type::VARCHAR]);
                pin_mut!(writer);
                for item in data {
                    if let Err(e) = writer.as_mut().write(&[&item.id, &created, &item.event_type, &item.event_count, &item.event_payout, &item.earner]).await {
                        err = Some(e);
                        break;
                    }
                }

                match err {
                    Some(e) => Err((e, connection)),
                    None  =>  {
                        if let Err(e) = writer.finish().await {
                            return Err((e, connection));
                        };
                        Ok((true, connection))
                    }
                }
            }
        })
        .await?;

    Ok(result)
}
