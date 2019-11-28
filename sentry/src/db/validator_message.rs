use crate::db::DbPool;
use bb8::RunError;
use bb8_postgres::tokio_postgres::types::ToSql;
use primitives::sentry::ValidatorMessage;
use primitives::{ChannelId, ValidatorId};

pub async fn get_validator_messages(
    pool: &DbPool,
    channel_id: &ChannelId,
    validator_id: &Option<ValidatorId>,
    message_types: &[String],
    limit: u64,
) -> Result<Vec<ValidatorMessage>, RunError<bb8_postgres::tokio_postgres::Error>> {
    let mut where_clauses: Vec<String> = vec!["channel_id = $1".to_string()];
    let mut params: Vec<&(dyn ToSql + Sync)> = vec![&channel_id];

    if let Some(validator_id) = validator_id {
        where_clauses.push(format!("from = ${}", params.len() + 1));
        params.push(validator_id);
    }

    let message_types = message_types.iter().map(|s| format!("'{}'", s)).collect::<Vec<String>>().join(",");
    if !message_types.is_empty() {
        where_clauses.push(format!("msg->>'type' IN (${})", params.len() + 1));
        params.push(dbg!(&message_types));
    }

    pool
        .run(move |connection| {
            async move {
                let statement = format!("SELECT \"from\", msg, received FROM validator_messages WHERE {} ORDER BY received DESC LIMIT {}", where_clauses.join(" AND "), limit);
                match connection.prepare(&statement).await {
                    Ok(select) => match connection.query(&select, params.as_slice()).await {
                        Ok(results) => {
                            let messages = results.iter().map(ValidatorMessage::from).collect();
                            Ok((messages, connection))},
                        Err(e) => Err((e, connection)),
                    },
                    Err(e) => Err((e, connection)),
                }
            }
        })
        .await
}
