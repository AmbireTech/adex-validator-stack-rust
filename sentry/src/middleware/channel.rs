use std::sync::Arc;

use axum::{
    extract::{Path, RequestParts},
    middleware::Next,
};
use serde::Deserialize;

use adapter::client::Locked;
use primitives::ChannelId;

use crate::{db::get_channel_by_id, response::ResponseError, Application, Auth};

/// This struct is required because of routes that have more parameters
/// apart from the `ChannelId`
#[derive(Debug, Deserialize)]
struct ChannelParam {
    pub id: ChannelId,
}

pub async fn channel_load<C: Locked + 'static, B>(
    request: axum::http::Request<B>,
    next: Next<B>,
) -> Result<axum::response::Response, ResponseError>
where
    B: Send,
{
    let app = request
        .extensions()
        .get::<Arc<Application<C>>>()
        .expect("Application should always be present")
        .clone();

    // running extractors requires a `RequestParts`
    let mut request_parts = RequestParts::new(request);

    let channel_param = request_parts
        .extract::<Path<ChannelParam>>()
        .await
        .map_err(|_| ResponseError::BadRequest("Bad Channel Id".to_string()))?;

    let channel = get_channel_by_id(&app.pool, &channel_param.id)
        .await?
        .ok_or(ResponseError::NotFound)?;

    let channel_context = app
        .config
        .find_chain_of(channel.token)
        .ok_or_else(|| {
            ResponseError::FailedValidation(
                "Channel token is not whitelisted in this validator".into(),
            )
        })?
        .with_channel(channel);

    // If this is an authenticated call
    // Check if the Channel context (Chain Id) aligns with the Authentication token Chain id
    match request_parts.extensions().get::<Auth>() {
            // If Chain Ids differ, the requester hasn't generated Auth token
            // to access the Channel in it's Chain Id.
            Some(auth) if auth.chain.chain_id != channel_context.chain.chain_id => {
                return Err(ResponseError::Forbidden("Authentication token is generated for different Chain and differs from the Channel's Chain".into()))
            }
            _ => {},
        }

    request_parts.extensions_mut().insert(channel_context);

    let request = request_parts.try_into_request().expect("Body extracted");

    Ok(next.run(request).await)
}

#[cfg(test)]
mod test {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware::from_fn,
        routing::get,
        Extension, Router,
    };
    use tower::Service;

    use adapter::{
        dummy::Dummy,
        ethereum::test_util::{GANACHE_1, GANACHE_1337},
    };
    use primitives::{
        test_util::{CAMPAIGNS, CREATOR, IDS},
        ChainOf, Channel,
    };

    use crate::{db::insert_channel, test_util::setup_dummy_app};

    use super::*;

    #[tokio::test]
    async fn test_channel_loading() {
        let app_guard = setup_dummy_app().await;
        let app = Arc::new(app_guard.app);

        let channel_context = CAMPAIGNS[0].of_channel();
        let channel = channel_context.context;

        let build_request = |id: ChannelId, auth: Option<Auth>| {
            let mut request = Request::builder()
                .uri(format!("/{id}/test"))
                .extension(app.clone());
            if let Some(auth) = auth {
                request = request.extension(auth);
            }

            request.body(Body::empty()).expect("Should build Request")
        };

        async fn handle(
            Extension(channel_context): Extension<ChainOf<Channel>>,
            Path((id, another)): Path<(ChannelId, String)>,
        ) -> String {
            assert_eq!(id, channel_context.context.id());
            assert_eq!(another, "test");
            "Ok".into()
        }

        let mut router = Router::new()
            .route("/:id/:another", get(handle))
            .layer(from_fn(channel_load::<Dummy, _>));

        // bad ChannelId
        {
            let mut request = build_request(channel.id(), None);
            *request.uri_mut() = "/WrongChannelId".parse().unwrap();

            let response = router
                .call(request)
                .await
                .expect("Should make request to Router");

            assert_eq!(StatusCode::BAD_REQUEST, response.status());
        }

        // non-existent Channel
        {
            let request = build_request(channel.id(), None);

            let response = router
                .call(request)
                .await
                .expect("Should make request to Router");

            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }

        // insert Channel
        insert_channel(&app.pool, &channel_context)
            .await
            .expect("Should insert Channel");

        // existing Channel
        {
            let request = build_request(channel.id(), None);

            let response = router
                .call(request)
                .await
                .expect("Should make request to Router");

            assert_eq!(response.status(), StatusCode::OK);
        }

        // existing Channel with Auth from a different Chain
        {
            let not_same_chain = Auth {
                era: 1,
                uid: IDS[&CREATOR],
                chain: GANACHE_1.clone(),
            };

            assert_ne!(channel_context.chain, not_same_chain.chain, "The chain of the Channel should be different than the chain of the Auth for this test!");

            let request = build_request(channel.id(), Some(not_same_chain));

            let response = router
                .call(request)
                .await
                .expect("Should make request to Router");

            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }

        // existing Channel with Auth with the same Chain
        {
            let same_chain = Auth {
                era: 1,
                uid: IDS[&CREATOR],
                chain: GANACHE_1337.clone(),
            };

            assert_eq!(
                channel_context.chain, same_chain.chain,
                "The chain of the Channel should be the same as the Auth for this test!"
            );

            let request = build_request(channel.id(), Some(same_chain));

            let response = router
                .call(request)
                .await
                .expect("Should make request to Router");

            assert_eq!(response.status(), StatusCode::OK);
        }
    }
}
