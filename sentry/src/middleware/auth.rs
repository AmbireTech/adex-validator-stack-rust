use hyper::header::AUTHORIZATION;
use hyper::{Body, Request};
use redis::aio::SharedConnection;

use primitives::adapter::{Adapter, Session as AdapterSession};

use crate::{ResponseError, Session};

/// Check `Authorization` header for `Bearer` scheme with `Adapter::session_from_token`.
/// If the `Adapter` fails to create an `AdapterSession`, `ResponseError::BadRequest` will be returned.
pub(crate) async fn for_request(
    mut req: Request<Body>,
    adapter: impl Adapter,
    redis: SharedConnection,
) -> Result<Request<Body>, ResponseError> {
    let authorization = req.headers().get(AUTHORIZATION);

    let prefix = "Bearer ";

    let token = authorization
        .and_then(|hv| {
            hv.to_str()
                .map(|token_str| {
                    if token_str.starts_with(prefix) {
                        None
                    } else {
                        Some(token_str.replacen(prefix, "", 1))
                    }
                })
                .transpose()
        })
        .transpose()?;

    if let Some(ref token) = token {
        let adapter_session = match redis::cmd("GET")
            .arg(token)
            .query_async::<_, Option<String>>(&mut redis.clone())
            .await?
            .and_then(|session_str| {
                match serde_json::from_str::<AdapterSession>(&session_str) {
                    Ok(session) => Some(session),
                    Err(serde_error) => {
                        // log message instead
                        println!("{}", serde_error);
                        None
                    }
                }
            }) {
            Some(adapter_session) => adapter_session,
            None => {
                // If there was a problem with the Session or the Token, this will error
                // and a BadRequest response will be returned
                adapter.session_from_token(token)?
            }
        };

        redis::cmd("SET")
            .arg(token)
            .arg(serde_json::to_string(&adapter_session)?)
            .query_async(&mut redis.clone())
            .await?;

        let session = Session {
            era: adapter_session.era,
            uid: adapter_session.uid.to_hex_non_prefix_string(),
            ip: get_request_ip(&req),
        };

        req.extensions_mut().insert(session);
    }

    // @TODO: Check if we actually need this since we have the `adapter` for the check: `channelIfActive`
    req.extensions_mut().insert(adapter.whoami().clone());

    Ok(req)
}

fn get_request_ip(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get("true-client-ip")
        .or_else(|| req.headers().get("x-forwarded-for"))
        .and_then(|hv| hv.to_str().map(ToString::to_string).ok())
}
