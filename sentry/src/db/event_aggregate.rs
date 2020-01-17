use crate::db::DbPool;
use bb8::RunError;
// use bb8_postgres::tokio_postgres::types::{ToSql, FromSql};
use chrono::{DateTime, Utc};
use primitives::sentry::EventAggregate;
use primitives::{ChannelId, ValidatorId};
use primitives::BigNum;
use std::ops::Add;
use postgres_types::{ToSql, FromSql};

pub async fn list_event_aggregates(
    pool: &DbPool,
    limit: u32,
    from: &Option<ValidatorId>,
    after: &Option<DateTime<Utc>>,
) -> Result<Vec<EventAggregate>, RunError<bb8_postgres::tokio_postgres::Error>> {
    let (mut where_clauses, mut params) = (vec![], Vec::<&(dyn ToSql + Sync)>::new());
    if let Some(from) = from {
        let key_counts = format!(
            "events->'IMPRESSION'->'eventPayouts'->'{}'",
            from.to_string()
        );
        where_clauses.push(format!("{} IS NOT NULL", key_counts));
    }
    if let Some(after) = after {
        params.push(after);
        where_clauses.push(format!("created > {}", params.len()));
    }

    let event_aggregates = pool
        .run(move |connection| {
            async move {
                let where_clause = if !where_clauses.is_empty() {
                    format!("WHERE {}", where_clauses.join(" AND "))
                } else {
                    "".to_string()
                };
                let statement = format!("SELECT channel_id, created, events FROM event_aggregates {} ORDER BY created DESC LIMIT {}", where_clause, limit);
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
}


pub async fn insert_event_aggregate(
    pool: &DbPool,
    channel_id: &ChannelId,
    event: &EventAggregate,
) -> Result<bool, RunError<bb8_postgres::tokio_postgres::Error>> {
    let mut values = Vec::new();
    let mut index = 0;
    let id = channel_id.to_string();

    let mut data: Vec<EventData> = Vec::new();

    for (event_type, aggr) in &event.events {
        if let Some(event_counts) = &aggr.event_counts {
            let mut total_event_counts: BigNum = 0.into();
            let mut total_event_payouts: BigNum = 0.into();
            for (earner, event_count) in event_counts {
                let event_payout = aggr.event_payouts[earner].clone();
                data.extend(vec![
                    EventData {
                        id: id.clone(),
                        event_type: event_type.clone(),
                        earner: Some(earner.clone()),
                        event_count: event_count.to_string(),
                        event_payout: event_payout.to_string(),
                    }
                    
                ]);

                // total sum 
                total_event_counts = event_count.add(&total_event_counts);
                total_event_payouts = total_event_payouts.add(event_payout);

                //
                // this is a work around for bulk inserts
                // rust-postgres does not have native support for bulk inserts
                // so we have to manually build up a query string dynamically based on
                // how many things we want to insert
                // i.e.
                // INSERT INTO event_aggregates (_, _) VALUES ($1, $2), ($3, $4), ($5, $6)

                values.push(format!(
                    "(${}, ${}, ${}, ${}, ${})",
                    index + 1,
                    index + 2,
                    index + 3,
                    index + 4,
                    index + 5
                ));
                index += 5;
            }
            // extend with
            data.extend(vec![
                EventData {
                    id: id.clone(),
                    event_type: event_type.clone(),
                    earner: None,
                    event_count: total_event_counts.to_string(),
                    event_payout: total_event_payouts.to_string(),
                }
            ]);

            values.push(format!(
                "(${}, ${}, ${}, ${}, ${})",
                index + 1,
                index + 2,
                index + 3,
                index + 4,
                index + 5
            ));
            index += 5;


        }
    }

    let inserts: Vec<&(dyn ToSql + Sync)> = data.iter().map(|x| x as &(dyn ToSql + Sync)).collect();

    //    the created field is supplied by postgres Default
    let query = format!("INSERT INTO event_aggregates (channel_id, event_type, earner, event_counts, event_payouts) values {}", values.join(" ,"));

    let result = pool
        .run(move |connection| {
            async move {
                match connection.prepare(&query).await {
                    Ok(stmt) => match connection.execute(&stmt, &inserts.as_slice()).await {
                        Ok(row) => {
                            let inserted = row == (index / 5);
                            Ok((inserted, connection))
                        }
                        Err(e) => Err((e, connection)),
                    },
                    Err(e) => Err((e, connection)),
                }
            }
        })
        .await?;

    Ok(result)
}
