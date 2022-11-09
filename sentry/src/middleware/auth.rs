use std::sync::Arc;

use axum::{
    http::{
        header::{AUTHORIZATION, REFERER},
        Request,
    },
    middleware::Next,
};

use adapter::{prelude::*, primitives::Session as AdapterSession};
use primitives::{analytics::AuthenticateAs, ValidatorId};

use crate::{response::ResponseError, Application, Auth, Session};

pub async fn is_admin<C: Locked + 'static, B>(
    request: axum::http::Request<B>,
    next: Next<B>,
) -> Result<axum::response::Response, ResponseError> {
    let auth = request
        .extensions()
        .get::<Auth>()
        .ok_or(ResponseError::Unauthorized)?;

    let config = &request
        .extensions()
        .get::<Arc<Application<C>>>()
        .expect("Application should always be present")
        .config;

    if !config.sentry.admins.contains(auth.uid.as_address()) {
        return Err(ResponseError::Unauthorized);
    }
    Ok(next.run(request).await)
}

pub async fn authentication_required<C: Locked + 'static, B>(
    request: axum::http::Request<B>,
    next: Next<B>,
) -> Result<axum::response::Response, ResponseError> {
    if request.extensions().get::<Auth>().is_some() {
        Ok(next.run(request).await)
    } else {
        Err(ResponseError::Unauthorized)
    }
}

/// Creates a [`Session`] and additionally [`Auth`] if a Bearer token was provided.
///
/// Check `Authorization` header for `Bearer` scheme with `Adapter::session_from_token`.
/// If the `Adapter` fails to create an `AdapterSession`, `ResponseError::BadRequest` will be returned.
pub async fn authenticate<C: Locked + 'static, B>(
    mut request: axum::http::Request<B>,
    next: Next<B>,
) -> Result<axum::response::Response, ResponseError> {
    let (adapter, redis) = {
        let app = request
            .extensions()
            .get::<Arc<Application<C>>>()
            .expect("Application should always be present");

        (app.adapter.clone(), app.redis.clone())
    };

    let referrer = request
        .headers()
        .get(REFERER)
        .and_then(|hv| hv.to_str().ok().map(ToString::to_string));

    let session = Session {
        ip: get_request_ip(&request),
        country: None,
        referrer_header: referrer,
        os: None,
    };
    request.extensions_mut().insert(session);

    let authorization = request.headers().get(AUTHORIZATION);

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
            uid: ValidatorId::from(adapter_session.uid),
            chain: adapter_session.chain,
        };

        request.extensions_mut().insert(auth);
    }

    Ok(next.run(request).await)
}

pub async fn authenticate_as_advertiser<B>(
    mut request: axum::http::Request<B>,
    next: Next<B>,
) -> Result<axum::response::Response, ResponseError> {
    let auth_uid = request
        .extensions()
        .get::<Auth>()
        .ok_or(ResponseError::Unauthorized)?
        .uid;

    let previous = request
        .extensions_mut()
        .insert(AuthenticateAs::Advertiser(auth_uid));

    assert!(
        previous.is_none(),
        "Should not contain previous value of AuthenticateAs"
    );

    Ok(next.run(request).await)
}

pub async fn authenticate_as_publisher<B>(
    mut request: axum::http::Request<B>,
    next: Next<B>,
) -> Result<axum::response::Response, ResponseError> {
    let auth_uid = request
        .extensions()
        .get::<Auth>()
        .ok_or(ResponseError::Unauthorized)?
        .uid;

    let previous = request
        .extensions_mut()
        .insert(AuthenticateAs::Publisher(auth_uid));

    assert!(
        previous.is_none(),
        "Should not contain previous value of AuthenticateAs"
    );

    Ok(next.run(request).await)
}

/// Get's the Request IP from either `true-client-ip` or `x-forwarded-for`,
/// splits the IPs separated by `,` (comma) and returns the first one.
fn get_request_ip<B>(req: &Request<B>) -> Option<String> {
    req.headers()
        .get("true-client-ip")
        .or_else(|| req.headers().get("x-forwarded-for"))
        .and_then(|hv| {
            hv.to_str()
                // filter out empty headers
                .map(ToString::to_string)
                .ok()
                .filter(|ip| !ip.is_empty())
        })
        .and_then(|token| {
            token
                .split(',')
                .next()
                // filter out empty IP
                .filter(|ip| !ip.is_empty())
                .map(ToString::to_string)
        })
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware::from_fn,
        routing::get,
        Extension, Router,
    };
    use tower::{Service, ServiceBuilder};

    use adapter::{
        dummy::{Dummy, HeaderToken},
        ethereum::test_util::GANACHE_1,
    };
    use primitives::test_util::{ADVERTISER, DUMMY_AUTH, FOLLOWER, IDS, LEADER, PUBLISHER};

    use crate::{
        test_util::{body_to_string, setup_dummy_app},
        Session,
    };

    use super::*;

    #[tokio::test]
    async fn test_is_admin() {
        let app_guard = setup_dummy_app().await;
        let app = Arc::new(app_guard.app);

        let admin = {
            assert!(
                app.config.sentry.admins.contains(&LEADER),
                "Should contain the Leader as an Admin for this test!"
            );
            IDS[&LEADER]
        };

        let not_admin = {
            assert!(
                !app.config.sentry.admins.contains(&FOLLOWER),
                "Should not contain the Follower as an Admin for this test!"
            );

            IDS[&FOLLOWER]
        };

        async fn handle() -> String {
            "Ok".into()
        }

        let mut router = Router::new()
            .route("/", get(handle))
            .layer(from_fn(is_admin::<Dummy, _>));

        // No Auth - Unauthorized
        {
            let no_auth = Request::builder()
                .extension(app.clone())
                .body(Body::empty())
                .expect("should never fail!");

            let response = router
                .call(no_auth)
                .await
                .expect("Should make request to Router");
            assert_eq!(StatusCode::UNAUTHORIZED, response.status());
        }

        // Not an Admin - Unauthorized
        {
            let not_admin_request = Request::builder()
                .extension(app.clone())
                .extension(Auth {
                    era: 1,
                    uid: not_admin,
                    chain: GANACHE_1.clone(),
                })
                .body(Body::empty())
                .expect("should never fail!");

            let response = router
                .call(not_admin_request)
                .await
                .expect("Should make request to Router");
            assert_eq!(StatusCode::UNAUTHORIZED, response.status());
        }

        // An Admin - Ok
        {
            let not_admin_request = Request::builder()
                .extension(app.clone())
                .extension(Auth {
                    era: 1,
                    uid: admin,
                    chain: GANACHE_1.clone(),
                })
                .body(Body::empty())
                .expect("should never fail!");

            let response = router
                .call(not_admin_request)
                .await
                .expect("Should make request to Router");
            assert_eq!(StatusCode::OK, response.status());
        }
    }

    #[tokio::test]
    async fn no_authentication_or_incorrect_value_should_not_add_session() {
        let app_guard = setup_dummy_app().await;
        let app = Arc::new(app_guard.app);

        async fn handle() -> String {
            "Ok".into()
        }

        let mut router = Router::new()
            .route("/", get(handle))
            .layer(from_fn(authenticate::<Dummy, _>));

        {
            let no_auth_req = Request::builder()
                .extension(app.clone())
                .body(Body::empty())
                .expect("should never fail!");

            let no_auth = router
                .call(no_auth_req)
                .await
                .expect("Handling the Request shouldn't have failed");

            assert!(
                no_auth.extensions().get::<Auth>().is_none(),
                "There shouldn't be a Auth in the extensions"
            );
        }

        // there is a Header, but it has wrong format
        {
            let incorrect_auth_req = Request::builder()
                .header(AUTHORIZATION, "Wrong Header")
                .extension(app.clone())
                .body(Body::empty())
                .expect("should never fail!");

            let incorrect_auth = router
                .call(incorrect_auth_req)
                .await
                .expect("Handling the Request shouldn't have failed");

            assert!(
                incorrect_auth.extensions().get::<Auth>().is_none(),
                "There shouldn't be an Auth in the extensions"
            );
        }

        // Token doesn't exist in the Adapter nor in Redis
        {
            let non_existent_token_req = Request::builder()
                .header(AUTHORIZATION, "Bearer wrong-token")
                .extension(app.clone())
                .body(Body::empty())
                .unwrap();

            let response = router
                .call(non_existent_token_req)
                .await
                .expect("Handling the Request shouldn't have failed");

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let response_body =
                serde_json::from_str::<HashMap<String, String>>(&body_to_string(response).await)
                    .expect("Should deserialize");
            assert_eq!("Authentication: Dummy Authentication token format should be in the format: `{Auth Token}:chain_id:{Chain Id}` but 'wrong-token' was provided", response_body["message"])
        }
    }

    #[tokio::test]
    async fn session_from_correct_authentication_token() {
        let app_guard = setup_dummy_app().await;
        let app = Arc::new(app_guard.app);

        let header_token = HeaderToken {
            token: DUMMY_AUTH[&LEADER].clone(),
            chain_id: GANACHE_1.chain_id,
        };

        async fn handle(
            Extension(auth): Extension<Auth>,
            Extension(session): Extension<Session>,
        ) -> String {
            assert_eq!(Some("120.0.0.1".to_string()), session.ip);
            assert_eq!(*LEADER, auth.uid.to_address());

            "Ok".into()
        }

        let mut router = Router::new()
            .route("/", get(handle))
            .layer(from_fn(authenticate::<Dummy, _>));

        let auth_header = format!("Bearer {header_token}");
        let request = Request::builder()
            .header(AUTHORIZATION, auth_header)
            .header("true-client-ip", "120.0.0.1")
            .extension(app.clone())
            .body(Body::empty())
            .unwrap();

        // The handle takes care of the assertions for the Extensions for us
        let response = router
            .call(request)
            .await
            .expect("Valid requests should succeed");

        assert_eq!("Ok", body_to_string(response).await);
    }

    #[test]
    fn test_get_request_ip_headers() {
        let build_request = |header: &str, ips: &str| -> Request<Body> {
            Request::builder()
                .header(header, ips)
                .body(Body::empty())
                .unwrap()
        };

        // No set headers
        {
            let request = Request::builder().body(Body::empty()).unwrap();
            let no_headers = get_request_ip(&request);
            assert_eq!(None, no_headers);
        }

        // Empty headers
        {
            let true_client_ip = build_request("true-client-ip", "");
            let x_forwarded_for = build_request("x-forwarded-for", "");

            let actual_true_client = get_request_ip(&true_client_ip);
            let actual_x_forwarded = get_request_ip(&x_forwarded_for);

            assert_eq!(None, actual_true_client);
            assert_eq!(None, actual_x_forwarded);
        }

        // Empty IPs `","`
        {
            let true_client_ip = build_request("true-client-ip", ",");
            let x_forwarded_for = build_request("x-forwarded-for", ",");

            let actual_true_client = get_request_ip(&true_client_ip);
            let actual_x_forwarded = get_request_ip(&x_forwarded_for);

            assert_eq!(None, actual_true_client);
            assert_eq!(None, actual_x_forwarded);
        }

        // "true-client-ip" - Single IP
        {
            let ips = "120.0.0.1";
            let true_client_ip = build_request("true-client-ip", ips);
            let actual_ips = get_request_ip(&true_client_ip);

            assert_eq!(Some(ips.to_string()), actual_ips);
        }

        // "x-forwarded-for" - Multiple IPs
        {
            let ips = "192.168.0.1,120.0.0.1,10.0.0.10";
            let true_client_ip = build_request("x-forwarded-for", ips);
            let actual_ips = get_request_ip(&true_client_ip);

            assert_eq!(Some("192.168.0.1".to_string()), actual_ips);
        }
    }

    #[tokio::test]
    async fn test_authenticate_as_advertiser_and_publisher() {
        let build_request = |auth: Option<Auth>| {
            let mut request = Request::builder().uri(format!("/"));

            if let Some(auth) = auth {
                request = request.extension(auth);
            }

            request.body(Body::empty()).expect("Should build Request")
        };

        #[derive(Debug, Clone, Copy)]
        pub enum TestFor {
            Advertiser,
            Publisher,
        }

        async fn handle(
            Extension(auth_as): Extension<AuthenticateAs>,
            Extension(test_for): Extension<TestFor>,
        ) -> StatusCode {
            match (auth_as, test_for) {
                (AuthenticateAs::Advertiser(_addr), TestFor::Advertiser)
                | (AuthenticateAs::Publisher(_addr), TestFor::Publisher) => StatusCode::OK,
                _ => StatusCode::BAD_REQUEST,
            }
        }

        // Advertiser
        {
            let mut router = Router::new().route(
                "/",
                get(handle).route_layer(
                    ServiceBuilder::new()
                        .layer(Extension(TestFor::Advertiser))
                        .layer(from_fn(authenticate_as_advertiser)),
                ),
            );

            // No Auth, should return Unauthorized
            {
                let request = build_request(None);

                let response = router
                    .call(request)
                    .await
                    .expect("Should make request to Router");

                assert_eq!(
                    StatusCode::UNAUTHORIZED,
                    response.status(),
                    "Should be unauthorized"
                )
            }

            // AuthenticateAs::Advertiser - OK
            {
                let request = build_request(Some(Auth {
                    era: 1,
                    uid: IDS[&ADVERTISER],
                    chain: GANACHE_1.clone(),
                }));

                let response = router
                    .call(request)
                    .await
                    .expect("Should make request to Router");

                assert_eq!(
                    StatusCode::OK,
                    response.status(),
                    "Should be authenticated as Advertiser!"
                )
            }
        }

        // Publisher
        {
            let mut router = Router::new().route(
                "/",
                get(handle).route_layer(
                    ServiceBuilder::new()
                        .layer(Extension(TestFor::Publisher))
                        .layer(from_fn(authenticate_as_publisher)),
                ),
            );

            // No Auth, should return Unauthorized
            {
                let request = build_request(None);

                let response = router
                    .call(request)
                    .await
                    .expect("Should make request to Router");

                assert_eq!(
                    StatusCode::UNAUTHORIZED,
                    response.status(),
                    "Should be unauthorized"
                )
            }

            // AuthenticateAs::Publisher - OK
            {
                let request = build_request(Some(Auth {
                    era: 1,
                    uid: IDS[&PUBLISHER],
                    chain: GANACHE_1.clone(),
                }));

                let response = router
                    .call(request)
                    .await
                    .expect("Should make request to Router");

                assert_eq!(
                    StatusCode::OK,
                    response.status(),
                    "Should be authenticated as Publisher!"
                )
            }
        }
    }

    #[tokio::test]
    #[should_panic]
    async fn test_authenticate_as_advertiser_panic() {
        async fn handle(Extension(_auth_as): Extension<AuthenticateAs>) -> String {
            "It should have panicked at this point".into()
        }

        let mut router = Router::new().route(
            "/",
            get(handle).route_layer(from_fn(authenticate_as_advertiser)),
        );

        let auth = Auth {
            era: 1,
            uid: IDS[&ADVERTISER],
            chain: GANACHE_1.clone(),
        };

        let request = Request::builder()
            .uri(format!("/"))
            .extension(auth.clone())
            .extension(AuthenticateAs::Advertiser(auth.uid))
            .body(Body::empty())
            .expect("Should build Request");

        let _response = router
            .call(request)
            .await
            .expect("Should make request to Router");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_authenticate_as_publisher_panic() {
        async fn handle(Extension(_auth_as): Extension<AuthenticateAs>) -> String {
            "It should have panicked at this point".into()
        }

        let mut router = Router::new().route(
            "/",
            get(handle).route_layer(from_fn(authenticate_as_publisher)),
        );

        let auth = Auth {
            era: 1,
            uid: IDS[&PUBLISHER],
            chain: GANACHE_1.clone(),
        };

        let request = Request::builder()
            .uri(format!("/"))
            .extension(auth.clone())
            .extension(AuthenticateAs::Publisher(auth.uid))
            .body(Body::empty())
            .expect("Should build Request");

        let _response = router
            .call(request)
            .await
            .expect("Should make request to Router");
    }
}
