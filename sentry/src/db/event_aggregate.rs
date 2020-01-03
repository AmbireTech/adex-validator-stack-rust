use crate::db::DbPool;
use bb8::RunError;
use bb8_postgres::tokio_postgres::types::ToSql;
use chrono::{DateTime, Utc};
use primitives::sentry::EventAggregate;
use primitives::ValidatorId;

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
