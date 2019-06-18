use std::iter::once;
use std::sync::Arc;

use futures::compat::Future01CompatExt;
use futures::future::{ok, try_join_all};
use futures::{FutureExt, TryFutureExt};
use futures_legacy::Future as LegacyFuture;
use reqwest::r#async::{Client, Response};
use serde::Deserialize;

use domain::{Channel, RepositoryFuture};

use crate::domain::channel::ChannelRepository;
use crate::infrastructure::persistence::api::ApiPersistenceError;

#[derive(Clone)]
pub struct ApiChannelRepository {
    pub client: Client,
}

impl ApiChannelRepository {
    fn fetch_page(&self, page: u64, identity: &str) -> RepositoryFuture<ChannelAllResponse> {
        self.client
            // call Sentry and fetch first page, where validator = identity
            .get(
                format!(
                    "http://localhost:8005/channel/list?validator={}&page={}",
                    identity, page
                )
                .as_str(),
            )
            .send()
            .and_then(|mut res: Response| res.json::<ChannelAllResponse>())
            // @TODO: Error handling
            .map_err(|_error| ApiPersistenceError::Reading.into())
            .compat()
            .boxed()
    }
}

impl ChannelRepository for ApiChannelRepository {
    fn all(&self, identity: &str) -> RepositoryFuture<Vec<Channel>> {
        let identity = Arc::new(identity.to_string());
        let handle = self.clone();

        let first_page = handle.fetch_page(1, &identity.clone());

        // call Sentry again and concat all the Channels in Future
        // fetching them until no more Channels are returned
        first_page
            .and_then(move |response| {
                let first_page_future = ok(response.channels).boxed();

                if response.total_pages < 2 {
                    first_page_future
                } else {
                    let identity = identity.clone();
                    let futures = (2..=response.total_pages)
                        .map(|page| {
                            handle
                                .fetch_page(page, &identity)
                                .map(|response_result| {
                                    response_result.and_then(|response| Ok(response.channels))
                                })
                                .boxed()
                        })
                        .chain(once(first_page_future));

                    try_join_all(futures)
                        .map(|result_all| {
                            result_all
                                .and_then(|all| Ok(all.into_iter().flatten().collect::<Vec<_>>()))
                        })
                        .boxed()
                }
            })
            .boxed()
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ChannelAllResponse {
    pub channels: Vec<Channel>,
    pub total_pages: u64,
}
