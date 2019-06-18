use domain::Channel;
use futures::compat::Future01CompatExt;
use futures::future::{ok, try_join_all, FutureExt, TryFutureExt};
use futures::Future;
use futures_legacy::Future as LegacyFuture;
use reqwest::r#async::{Client, Response};
use serde::Deserialize;
use std::iter::once;

#[derive(Clone)]
// @TODO: make pub(crate)
pub struct SentryApi {
    pub sentry_url: String,
    pub client: Client,
}

impl SentryApi {
    pub fn all_channels(
        &self,
        validator: Option<String>,
    ) -> impl Future<Output = Result<Vec<Channel>, reqwest::Error>> {
        let first_page = self.clone().fetch_page(1, validator.clone());

        // call Sentry again and concat all the Channels in Future
        // fetching them until no more Channels are returned
        let handle = self.clone();
        first_page
            .and_then(move |response| {
                let first_page_future = ok(response.channels).boxed();

                if response.total_pages < 2 {
                    first_page_future
                } else {
                    let futures = (2..=response.total_pages)
                        .map(|page| {
                            handle
                                .clone()
                                .fetch_page(page, validator.clone())
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

    async fn fetch_page(
        self,
        page: u64,
        validator: Option<String>,
    ) -> Result<ChannelAllResponse, reqwest::Error> {
        let mut query = vec![format!("page={}", page)];

        if let Some(validator) = validator {
            query.push(format!("validator={}", validator));
        }

        let future = self
            .client
            // call Sentry and fetch first page, where validator = identity
            .get(format!("{}/channel/list?{}", self.sentry_url, query.join("&")).as_str())
            .send()
            .and_then(|mut res: Response| res.json::<ChannelAllResponse>());

        await!(future.compat())
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ChannelAllResponse {
    pub channels: Vec<Channel>,
    pub total_pages: u64,
}
