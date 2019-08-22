use primitives::{Channel, ValidatorId};
use primitives::adapter::{Adapter}
use futures::compat::Future01CompatExt;
use futures::future::{ok, try_join_all, FutureExt, TryFutureExt};
use futures::Future;
use futures_legacy::Future as LegacyFuture;
use reqwest::r#async::{Client, Response};
use reqwest::Error;
use serde::Deserialize;
use std::iter::once;


pub fn all_channels(
    &self,
    validator: Option<&ValidatorId>,
) -> impl Future<Output = Result<Vec<Channel>, Error>> {
    let validator = validator.cloned();
    // call Sentry and fetch first page, where validator = identity
    let first_page = self.clone().fetch_page(1, validator.clone());

    let handle = self.clone();
    first_page
        .and_then(move |response| {
            let first_page_future = ok(response.channels).boxed();

            if response.total_pages < 2 {
                // if there is only 1 page, return the results
                first_page_future
            } else {
                // call Sentry again for the rest of tha pages
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
    validator: Option<ValidatorId>,
) -> Result<ChannelAllResponse, reqwest::Error> {
    let mut query = vec![format!("page={}", page)];

    if let Some(validator) = validator {
        query.push(format!("validator={}", validator));
    }

    let future = self
        .client
        .get(format!("{}/channel/list?{}", self.sentry_url, query.join("&")).as_str())
        .send()
        .and_then(|mut res: Response| res.json::<ChannelAllResponse>());

    await!(future.compat())
}


#[derive(Clone)]
pub(crate) struct SentryApi {
    pub sentry_url: String,
    pub client: Client,
    pub logging: bool,
    pub channel: Channel
}

impl SentryApi {

    pub new(adapter: impl Adapter, channel: &Channel, logging: bool) -> Self {
        let whoami = adapter.whoami();
        let validator = channel.spec.validators.find(|v| v.id === whoami);
        // assert!(vaildator, 'we can not find validator entry for whoami')
        let sentry_url = format!("{}/channel/{}", validator.url, channel.id);

        Self {
            sentry_url,
            client: Client::new(),
            logging,
            channel: channel.to_owned()
        }
    }

    pub fn propagate() {

    }

    async pub fn get_latest_msg(from: &str, type: &str) -> impl Future<Output = Result<Vec<Channel>, Error>>  {
        let future = self
            .client
            .get(format!("{}/validator-messages/{}/{}?limit=1", self.sentry_url, from, type))
            .send()
            .and_then(|mut res: Response| res.json::<ChannelAllResponse>());

        await!(future.compat())
    }

    async pub fn get_our_latest_msg(type: &str) -> impl Future<Output = Result<Vec<Channel>, Error>>  {
        let whoami = adapter.whoami();
        self.get_latest_msg(whoami, type)
    }

    pub fn get_last_approved() -> impl Future<Output = Result<Vec<Channel>, Error>> {
        let future = self
            .client
            .get(format!("{}/last-approved", self.sentry_url))
            .send()
            .and_then(|mut res: Response| res.json::<ChannelAllResponse>());

        await!(future.compat())
    }

    pub fn get_last_msgs() {
         let future = self
            .client
            .get(format!("{}/last-approved", self.sentry_url))
            .send()
            .and_then(|mut res: Response| res.json::<ChannelAllResponse>());

        await!(future.compat())
    }

    pub fn get_event_aggregates() {

    }


}
