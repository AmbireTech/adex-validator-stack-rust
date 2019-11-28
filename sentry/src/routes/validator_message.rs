use crate::db::get_validator_messages;
use crate::{success_response, Application, ResponseError};
use hyper::{Body, Request, Response};
use primitives::adapter::Adapter;
use primitives::sentry::ValidatorMessageResponse;
use primitives::{Channel, DomainError, ValidatorId};
use serde::Deserialize;
use std::convert::TryFrom;

#[derive(Deserialize)]
pub struct ValidatorMessagesListQuery {
    limit: Option<u64>,
}

pub fn extract_params(from_path: &str) -> Result<(Option<ValidatorId>, Vec<String>), DomainError> {
    // trim the `/` at the beginning & end if there is one or more
    // and split the rest of the string at the `/`
    let split: Vec<&str> = from_path.trim_matches('/').split('/').collect();

    if split.len() > 2 {
        return Err(DomainError::InvalidArgument(
            "Too many parameters".to_string(),
        ));
    }

    let validator_id = split
        .get(0)
        // filter an empty string
        .filter(|string| !string.is_empty())
        // then try to map it to ValidatorId
        .map(|string| ValidatorId::try_from(*string))
        // Transpose in order to check for an error from the conversion
        .transpose()?;

    let message_types = split
        .get(1)
        .filter(|string| !string.is_empty())
        .map(|string| string.split('+').map(|s| s.to_string()).collect());

    Ok((validator_id, message_types.unwrap_or_default()))
}

pub async fn list_validator_messages<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
    validator_id: &Option<ValidatorId>,
    message_types: &[String],
) -> Result<Response<Body>, ResponseError> {
    let query =
        serde_urlencoded::from_str::<ValidatorMessagesListQuery>(&req.uri().query().unwrap_or(""))?;

    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel");

    let config_limit = app.config.msgs_find_limit as u64;
    let limit = query
        .limit
        .filter(|n| *n >= 1)
        .unwrap_or(config_limit)
        .min(config_limit);

    let validator_messages =
        get_validator_messages(&app.pool, &channel.id, validator_id, message_types, limit).await?;

    let response = ValidatorMessageResponse { validator_messages };

    Ok(success_response(serde_json::to_string(&response)?))
}
