use futures::compat::Future01CompatExt;
use futures::future::FutureExt;
use futures_legacy::Future as LegacyFuture;
use reqwest::r#async::{Client, Response};
use serde::Deserialize;

use domain::{Channel, RepositoryFuture};

use crate::domain::channel::ChannelRepository;
use crate::infrastructure::persistence::api::ApiPersistenceError;

pub struct ApiChannelRepository {
    pub client: Client,
}

impl ChannelRepository for ApiChannelRepository {
    fn all(&self, identity: &str) -> RepositoryFuture<Vec<Channel>> {
        let list_page_url = |page: u32| {
            format!(
                "http://localhost:8005/channel/list?validator={}&page={}",
                identity, page
            )
        };
        let first_page = self
            .client
            // call Sentry and fetch first page, where validator = params.identifier
            .get(&list_page_url(1))
            .send()
            .and_then(|mut res: Response| res.json::<AllResponse>())
            .map(|response| response.channels)
            // @TODO: Error handling
            .map_err(|_error| ApiPersistenceError::Reading.into());

        // call Sentry again and concat all the Channels in 1 Stream
        // fetching them until no more Channels are returned
        // @TODO: fetch the rest of the results

        first_page.compat().boxed()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AllResponse {
    pub channels: Vec<Channel>,
    pub total_pages: u64,
}
