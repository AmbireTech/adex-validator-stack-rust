use crate::db::DbPool;
use bb8::RunError;
use bb8_postgres::tokio_postgres::types::ToSql;
use chrono::{DateTime, Utc, Date};
use primitives::sentry::EventAggregate;
use primitives::{ValidatorId, ChannelId};

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

pub async fn insert_event_aggregate(
    pool: &DbPool,
    channel_id: &ChannelId,
    event: &EventAggregate
) -> Result<bool, RunError<bb8_postgres::tokio_postgres::Error>>  {

    let mut inserts : Vec<String> = Vec::new();
    let mut values = Vec::new();
    let mut index = 0;
    let id = channel_id.to_string();

    for (event_type, aggr) in event.events {
        if let Some(event_counts) = aggr.event_counts {
            for (earner, value) in event_counts {
                let data =  vec![id.clone(), event_type.clone(), earner, value.to_string(), aggr.event_payouts[&earner].to_string()];
                inserts.extend(data);
                //
                // this is a work around for bulk inserts
                // rust-postgres does not have native support for bulk inserts
                // so we have to manually build up a query string dynamically based on
                // how many things we want to insert
                //
                values.push(format!("(${}, ${}, ${}, ${}, ${})", index+1, index+2, index+3, index+4, index+5));
                index += 5;
            }
        }
    }

    //    the created field is supplied by postgres Default
     pool
        .run(
            move |connection | {
                async move {
                    match connection.prepare(&format!("INSERT INTO event_aggregates (channel_id, event_type, earner, event_counts, event_payouts) values {}", values.join(" ,"))).await {
                        Ok(stmt) => match connection.execute(&stmt, &inserts.as_slice()).await {
                            Ok(row) => {
                                let inserted = row == (index / 5);
                                Ok((inserted, connection))
                            },
                            Err(e) => Err((e, connection)),
                        },
                        Err(e) => Err((e, connection)),
                    }
                }
            }
        ).await
}
