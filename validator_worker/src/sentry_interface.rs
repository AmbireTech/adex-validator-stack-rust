use crate::error::ValidatorWorkerError;
use futures::compat::Future01CompatExt;
use futures::future::{ok, try_join_all, FutureExt, TryFutureExt};
use futures::Future;
use futures_legacy::Future as LegacyFuture;
use primitives::adapter::Adapter;
use primitives::sentry::{
    ChannelAllResponse, EventAggregateResponse, LastApprovedResponse, SuccessResponse,
    ValidatorMessageResponse,
};
use primitives::validator::MessageTypes;
use primitives::{Channel, Config, ValidatorDesc};
use reqwest::header::AUTHORIZATION;
use reqwest::r#async::{Client, Response};
use serde::Deserialize;
use std::error::Error;
use std::iter::once;
use std::pin::Pin;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct SentryApi<T: Adapter> {
    pub validator: ValidatorDesc,
    pub adapter: T,
    pub sentry_url: String,
    pub client: Client,
    pub logging: bool,
    pub channel: Channel,
    pub config: Config,
}

impl<T: Adapter + 'static> SentryApi<T> {
    pub fn new(
        adapter: T,
        channel: &Channel,
        config: &Config,
        logging: bool,
    ) -> Result<Self, ValidatorWorkerError> {
        let whoami = adapter.whoami();
        let validator = channel
            .spec
            .validators
            .into_iter()
            .find(|&v| v.id == whoami);

        let client = Client::builder()
            .timeout(Duration::from_secs(config.fetch_timeout.into()))
            .build()
            .unwrap();

        match validator {
            Some(v) => {
                let sentry_url = format!("{}/channel/{}", v.url, channel.id);

                Ok(Self {
                    validator: v.clone().to_owned(),
                    adapter,
                    sentry_url,
                    client,
                    logging,
                    channel: channel.to_owned(),
                    config: config.to_owned(),
                })
            }
            None => Err(ValidatorWorkerError::InvalidValidatorEntry(
                "we can not find validator entry for whoami".to_string(),
            )),
        }
    }

    pub fn propagate(&self, messages: Vec<MessageTypes>) {
        let serialised_messages: Vec<String> = messages
            .into_iter()
            .map(|message| match message {
                MessageTypes::NewState(new_state) => serde_json::to_string(&new_state).unwrap(),
                MessageTypes::ApproveState(approve_state) => {
                    serde_json::to_string(&approve_state).unwrap()
                }
                MessageTypes::Heartbeat(heartbeat) => serde_json::to_string(&heartbeat).unwrap(),
                MessageTypes::RejectState(reject_state) => {
                    serde_json::to_string(&reject_state).unwrap()
                }
                MessageTypes::Accounting(accounting) => serde_json::to_string(&accounting).unwrap(),
            })
            .collect();

        for validator in self.channel.spec.validators.into_iter() {
            let auth_token = self.adapter.get_auth(&validator).unwrap();
            match propagate_to(
                &auth_token,
                self.config.propagation_timeout,
                &validator,
                &serialised_messages,
            ) {
                Ok(_) => return,
                Err(e) => handle_http_error(e, &validator.url),
            }
        }
    }

    pub async fn get_latest_msg(
        &self,
        from: String,
        message_type: String,
    ) -> Result<ValidatorMessageResponse, reqwest::Error> {
        let future = self
            .client
            .get(&format!(
                "{}/validator-messages/{}/{}?limit=1",
                self.sentry_url, from, message_type
            ))
            .send()
            .and_then(|mut res: Response| res.json::<ValidatorMessageResponse>())
            .compat();

        await!(future)
    }

    pub async fn get_our_latest_msg(
        &self,
        message_type: String,
    ) -> Result<ValidatorMessageResponse, reqwest::Error> {
        let whoami = self.adapter.whoami();
        await!(self.get_latest_msg(whoami, message_type))
    }

    pub async fn get_last_approved(&self) -> Result<LastApprovedResponse, reqwest::Error> {
        let future = self
            .client
            .get(&format!("{}/last-approved", self.sentry_url))
            .send()
            .and_then(|mut res: Response| res.json::<LastApprovedResponse>());

        await!(future.compat())
    }

    pub async fn get_last_msgs(&self) -> Result<LastApprovedResponse, reqwest::Error> {
        let future = self
            .client
            .get(&format!(
                "{}/last-approved?withHearbeat=true",
                self.sentry_url
            ))
            .send()
            .and_then(|mut res: Response| res.json::<LastApprovedResponse>());

        await!(future.compat())
    }

    pub async fn get_event_aggregates(
        &self,
        after: Option<u32>,
    ) -> Result<EventAggregateResponse, reqwest::Error> {
        let whoami = self.adapter.whoami();
        let validator = self
            .channel
            .spec
            .validators
            .into_iter()
            .find(|&v| v.id == whoami);
        let auth_token = self.adapter.get_auth(validator.unwrap()).unwrap();

        let url = match after {
            Some(duration) => format!("{}/events-aggregates?after={}", self.sentry_url, duration),
            None => format!("{}/events-aggregates", self.sentry_url),
        };

        let future = self
            .client
            .get(&url)
            .header("authorization", auth_token.to_string())
            .send()
            .and_then(|mut res: Response| res.json::<EventAggregateResponse>());

        await!(future.compat())
    }
}

fn propagate_to(
    auth_token: &str,
    timeout: u32,
    validator: &ValidatorDesc,
    messages: &[String],
) -> Result<(), reqwest::Error> {
    // create client with timeout
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout.into()))
        .build()?;
    let url = validator.url.to_string();

    let response: SuccessResponse = client
        .post(&url)
        .header(AUTHORIZATION, auth_token.to_string())
        .json(messages)
        .send()?
        .json()?;

    Ok(())
}

fn handle_http_error(e: reqwest::Error, url: &str) {
    if e.is_http() {
        match e.url() {
            None => println!("No Url given"),
            Some(url) => println!("Problem making request to: {}", url),
        }
    }
    // Inspect the internal error and output it
    if e.is_serialization() {
        let serde_error = match e.get_ref() {
            None => return,
            Some(err) => err,
        };
        println!("problem parsing information {}", serde_error);
    }

    if e.is_client_error() {
        println!("erorr sending http request for validator {}", url)
    }

    if e.is_redirect() {
        println!("server redirecting too many times or making loop");
    }
}

pub async fn all_channels(
    sentry_url: &str,
    adapter: impl Adapter + 'static,
) -> Result<Vec<Channel>, ()> {
    let validator = adapter.whoami();
    let url = sentry_url.to_owned();
    let first_page = await!(fetch_page(url.clone(), 0, validator.clone())).unwrap();
    if first_page.total_pages < 2 {
        Ok(first_page.channels)
    } else {
        let mut all: Vec<ChannelAllResponse> = await!(try_join_all(
            (0..first_page.total_pages).map(|i| fetch_page(url.clone(), i, validator.clone()))
        ))
        .unwrap();
        all.push(first_page);
        let result_all: Vec<Channel> = all
            .into_iter()
            .flat_map(|ch| ch.channels.into_iter())
            .collect();
        Ok(result_all)
    }
}

pub async fn fetch_page(
    sentry_url: String,
    page: u64,
    validator: String,
) -> Result<ChannelAllResponse, reqwest::Error> {
    let client = Client::new();

    let mut query = vec![format!("page={}", page)];
    query.push(format!("validator={}", validator.to_string()));

    let future = client
        .get(format!("{}/channel/list?{}", sentry_url, query.join("&")).as_str())
        .send()
        .and_then(|mut res: Response| res.json::<ChannelAllResponse>());

    await!(future.compat())
}
