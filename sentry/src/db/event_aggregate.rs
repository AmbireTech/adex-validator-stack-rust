use crate::db::DbPool;
use bb8::RunError;
use bb8_postgres::tokio_postgres::binary_copy::BinaryCopyInWriter;
use bb8_postgres::tokio_postgres::types::Type;
use bb8_postgres::tokio_postgres::Error;
use chrono::{DateTime, Utc};
use futures::pin_mut;
use postgres_types::{FromSql, ToSql};
use primitives::sentry::EventAggregate;
use primitives::BigNum;
use primitives::{ChannelId, ValidatorId};
use std::ops::Add;

pub async fn list_event_aggregates(
    pool: &DbPool,
    channel_id: &ChannelId,
    limit: u32,
    from: &Option<ValidatorId>,
    after: &Option<DateTime<Utc>>,
) -> Result<Vec<EventAggregate>, RunError<bb8_postgres::tokio_postgres::Error>> {
    let (mut where_clauses, mut params) = (vec![], Vec::<&(dyn ToSql + Sync)>::new());
    let id = channel_id.to_string();
    where_clauses.push(format!("channel_id = '{}'", id));

    if let Some(from) = from {
        where_clauses.push(format!("earner = '{}'", from.to_string()));
        where_clauses.push("event_type = 'IMPRESSION'".to_string());
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
                                            earner, event_counts
                                        )
                                    ),
                                    'eventPayouts',
                                    jsonb_object_agg(
                                        jsonb_build_object(
                                            earner, event_payouts
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

#[derive(Debug, ToSql, FromSql)]
struct EventData {
    id: String,
    event_type: String,
    earner: Option<String>,
    event_count: String,
    event_payout: String,
    created: DateTime<Utc>,
}

pub async fn insert_event_aggregate(
    pool: &DbPool,
    channel_id: &ChannelId,
    event: &EventAggregate,
) -> Result<bool, RunError<bb8_postgres::tokio_postgres::Error>> {
    let id = channel_id.to_string();
    let created = Utc::now();

    let mut data: Vec<EventData> = Vec::new();

    for (event_type, aggr) in &event.events {
        if let Some(event_counts) = &aggr.event_counts {
            let mut total_event_counts: BigNum = 0.into();
            let mut total_event_payouts: BigNum = 0.into();
            for (earner, event_count) in event_counts {
                let event_payout = aggr.event_payouts[earner].clone();
                data.extend(vec![EventData {
                    id: id.clone(),
                    event_type: event_type.clone(),
                    earner: Some(earner.clone()),
                    event_count: event_count.to_string(),
                    event_payout: event_payout.to_string(),
                    created,
                }]);

                // total sum
                total_event_counts = event_count.add(&total_event_counts);
                total_event_payouts = total_event_payouts.add(event_payout);
            }

            data.extend(vec![EventData {
                id: id.clone(),
                event_type: event_type.clone(),
                earner: None,
                event_count: total_event_counts.to_string(),
                event_payout: total_event_payouts.to_string(),
                created,
            }]);
        }
    }

    let result = pool
        .run(move |connection| {
            async move {
                let mut err: Option<Error> = None;
                let sink = match connection.copy_in("COPY event_aggregates(channel_id, created, event_type, event_counts, event_payouts, earner) FROM STDIN BINARY").await {
                    Ok(sink) => sink,
                    Err(e) => return Err((e, connection))
                };

                let writer = BinaryCopyInWriter::new(sink, &[Type::VARCHAR, Type::TIMESTAMPTZ, Type::VARCHAR, Type::VARCHAR, Type::VARCHAR, Type::VARCHAR]);
                pin_mut!(writer);
                for item in data {
                    if let Err(e) = writer.as_mut().write(&[&item.id, &item.created, &item.event_type, &item.event_count, &item.event_payout, &item.earner]).await {
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
