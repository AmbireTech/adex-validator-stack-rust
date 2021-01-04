use std::error;

use async_trait::async_trait;
use hyper::header::{AUTHORIZATION, REFERER};
use hyper::{Body, Request};
use redis::aio::MultiplexedConnection;

use primitives::adapter::{Adapter, Session as AdapterSession};

use crate::{middleware::Middleware, Application, Auth, ResponseError, Session};

#[derive(Debug)]
pub struct Authenticate;

#[async_trait]
impl<A: Adapter + 'static> Middleware<A> for Authenticate {
    async fn call<'a>(
        &self,
        request: Request<Body>,
        application: &'a Application<A>,
    ) -> Result<Request<Body>, ResponseError> {
        for_request(request, &application.adapter, application.redis.clone())
            .await
            .map_err(|error| {
                slog::error!(&application.logger, "{}", &error; "module" => "middleware-auth");

                ResponseError::Unauthorized
            })
    }
}

#[derive(Debug)]
pub struct AuthRequired;

#[async_trait]
impl<A: Adapter + 'static> Middleware<A> for AuthRequired {
    async fn call<'a>(
        &self,
        request: Request<Body>,
        _application: &'a Application<A>,
    ) -> Result<Request<Body>, ResponseError> {
        if request.extensions().get::<Session>().is_some() {
            Ok(request)
        } else {
            Err(ResponseError::Unauthorized)
        }
    }
}

/// Check `Authorization` header for `Bearer` scheme with `Adapter::session_from_token`.
/// If the `Adapter` fails to create an `AdapterSession`, `ResponseError::BadRequest` will be returned.
async fn for_request(
    mut req: Request<Body>,
    adapter: &impl Adapter,
    redis: MultiplexedConnection,
) -> Result<Request<Body>, Box<dyn error::Error>> {
    let referrer = req
        .headers()
        .get(REFERER)
        .map(|hv| hv.to_str().ok().map(ToString::to_string))
        .flatten();

    let session = Session {
        ip: get_request_ip(&req),
        country: None,
        referrer_header: referrer,
        os: None,
    };
    req.extensions_mut().insert(session);

    let authorization = req.headers().get(AUTHORIZATION);

    let prefix = "Bearer ";

    let token = authorization
        .and_then(|hv| {
            hv.to_str()
                .map(|token_str| token_str.strip_prefix(prefix))
                .transpose()
        })
        .transpose()?;

    if let Some(token) = token {
        let adapter_session = match redis::cmd("GET")
            .arg(token)
            .query_async::<_, Option<String>>(&mut redis.clone())
            .await?
            .and_then(|session_str| serde_json::from_str::<AdapterSession>(&session_str).ok())
        {
            Some(adapter_session) => adapter_session,
            None => {
                // If there was a problem with the Session or the Token, this will error
                // and a BadRequest response will be returned
                let adapter_session = adapter.session_from_token(token).await?;

                // save the Adapter Session to Redis for the next request
                // if serde errors on deserialization this will override the value inside
                redis::cmd("SET")
                    .arg(token)
                    .arg(serde_json::to_string(&adapter_session)?)
                    .query_async(&mut redis.clone())
                    .await?;

                adapter_session
            }
        };

        let auth = Auth {
            era: adapter_session.era,
            uid: adapter_session.uid,
        };

        req.extensions_mut().insert(auth);
    }

    Ok(req)
}

fn get_request_ip(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get("true-client-ip")
        .or_else(|| req.headers().get("x-forwarded-for"))
        .and_then(|hv| hv.to_str().map(ToString::to_string).ok())
        .map(|token| token.split(',').next().map(ToString::to_string))
        .flatten()
}

#[cfg(test)]
mod test {
    use hyper::Request;

    use adapter::DummyAdapter;
    use primitives::adapter::DummyAdapterOptions;
    use primitives::config::configuration;
    use primitives::util::tests::prep_db::{AUTH, IDS};

    use crate::db::redis_connection;

    use super::*;

    async fn setup() -> (DummyAdapter, MultiplexedConnection) {
        let adapter_options = DummyAdapterOptions {
            dummy_identity: IDS["leader"],
            dummy_auth: IDS.clone(),
            dummy_auth_tokens: AUTH.clone(),
        };
        let config = configuration("development", None).expect("Dev config should be available");
        let mut redis = redis_connection().await.expect("Couldn't connect to Redis");
        // run `FLUSHALL` to clean any leftovers of other tests
        let _ = redis::cmd("FLUSHALL")
            .query_async::<_, String>(&mut redis)
            .await;
        (DummyAdapter::init(adapter_options, &config), redis)
    }

    #[tokio::test]
    async fn no_authentication_or_incorrect_value_should_not_add_session() {
        let no_auth_req = Request::builder()
            .body(Body::empty())
            .expect("should never fail!");

        let (dummy_adapter, redis) = setup().await;
        let no_auth = for_request(no_auth_req, &dummy_adapter, redis.clone())
            .await
            .expect("Handling the Request shouldn't have failed");

        assert!(
            no_auth.extensions().get::<Auth>().is_none(),
            "There shouldn't be a Session in the extensions"
        );

        // there is a Header, but it has wrong format
        let incorrect_auth_req = Request::builder()
            .header(AUTHORIZATION, "Wrong Header")
            .body(Body::empty())
            .unwrap();
        let incorrect_auth = for_request(incorrect_auth_req, &dummy_adapter, redis.clone())
            .await
            .expect("Handling the Request shouldn't have failed");
        assert!(
            incorrect_auth.extensions().get::<Auth>().is_none(),
            "There shouldn't be a Session in the extensions"
        );

        // Token doesn't exist in the Adapter nor in Redis
        let non_existent_token_req = Request::builder()
            .header(AUTHORIZATION, "Bearer wrong-token")
            .body(Body::empty())
            .unwrap();
        match for_request(non_existent_token_req, &dummy_adapter, redis).await {
            Err(error) => {
                assert!(error.to_string().contains("no session token for this auth: wrong-token"), "Wrong error received");
            }
            _ => panic!("We shouldn't get a success response nor a different Error than BadRequest for this call"),
        };
    }

    #[tokio::test]
    async fn session_from_correct_authentication_token() {
        let (dummy_adapter, redis) = setup().await;

        let token = AUTH["leader"].clone();
        let auth_header = format!("Bearer {}", token);
        let req = Request::builder()
            .header(AUTHORIZATION, auth_header)
            .body(Body::empty())
            .unwrap();

        let altered_request = for_request(req, &dummy_adapter, redis)
            .await
            .expect("Valid requests should succeed");

        let auth = altered_request
            .extensions()
            .get::<Auth>()
            .expect("There should be a Session set inside the request");

        assert_eq!(IDS["leader"], auth.uid);

        let session = altered_request
            .extensions()
            .get::<Session>()
            .expect("There should be a Session set inside the request");
        assert!(session.ip.is_none());
    }
}
