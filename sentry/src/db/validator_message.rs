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
        where_clauses.push(format!(r#""from" = ${}"#, params.len() + 1));
        params.push(validator_id);
    }

    add_message_types_params(&mut where_clauses, &mut params, message_types);

    pool
        .run(move |connection| {
            async move {
                let statement = format!(r#"SELECT "from", msg, received FROM validator_messages WHERE {} ORDER BY received DESC LIMIT {}"#, where_clauses.join(" AND "), limit);
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
