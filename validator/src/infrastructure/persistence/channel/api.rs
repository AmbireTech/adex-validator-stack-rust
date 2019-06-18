use futures::compat::Future01CompatExt;
use futures_legacy::Future as LegacyFuture;
use reqwest::r#async::{Client, Response};
use serde::Deserialize;

use domain::{Channel, RepositoryFuture};

use crate::domain::channel::ChannelRepository;
use crate::infrastructure::persistence::api::ApiPersistenceError;
use futures::future::{ok, try_join_all};
use futures::{FutureExt, TryFutureExt};
use std::iter::once;
use std::sync::Arc;

pub struct ApiChannelRepository {
    pub client: Client,
}

impl ChannelRepository for ApiChannelRepository {
    fn all(&self, identity: &str) -> RepositoryFuture<Vec<Channel>> {
        let identity = Arc::new(identity.to_string());
        let first_page = self
            .client
            // call Sentry and fetch first page, where validator = identity
            .get(
                format!(
                    "http://localhost:8005/channel/list?validator={}&page={}",
                    identity.clone(),
                    1
                )
                .as_str(),
            )
            .send()
            .and_then(|mut res: Response| res.json::<ChannelAllResponse>())
            // @TODO: Error handling
            .map_err(|_error| ApiPersistenceError::Reading.into())
            .compat();

        // call Sentry again and concat all the Channels in Future
        // fetching them until no more Channels are returned
        let client = self.client.clone();
        first_page
            .and_then(move |response| {
                let futures = ok(response.channels).boxed();

                if response.total_pages < 2 {
                    futures
                } else {
                    let futures = (2..=response.total_pages)
                        .map(|page| {
                            client
                                .get(
                                    format!(
                                        "http://localhost:8005/channel/list?validator={}&page={}",
                                        identity.clone(),
                                        page
                                    )
                                    .as_str(),
                                )
                                .send()
                                .and_then(move |mut res: Response| res.json::<ChannelAllResponse>())
                                // @TODO: Error handling
                                .map_err(|_error| ApiPersistenceError::Reading.into())
                                .map(|response| response.channels)
                                .compat()
                                .boxed()
                        })
                        .chain(once(futures));

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
