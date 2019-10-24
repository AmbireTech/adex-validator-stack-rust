use crate::error::ValidatorWorker;
use async_std::sync::RwLock;
use chrono::{DateTime, Utc};
use futures::compat::Future01CompatExt;
use futures::future::try_join_all;
use futures_legacy::Future as LegacyFuture;
use primitives::adapter::Adapter;
use primitives::channel::SpecValidator;
use primitives::sentry::{
    ChannelAllResponse, EventAggregateResponse, LastApprovedResponse, SuccessResponse,
    ValidatorMessageResponse,
};
use primitives::validator::MessageTypes;
use primitives::{Channel, Config, ValidatorDesc, ValidatorId};
use reqwest::r#async::{Client, Response};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SentryApi<T: Adapter> {
    pub adapter: Arc<RwLock<T>>,
    pub validator_url: String,
    pub client: Client,
    pub logging: bool,
    pub channel: Channel,
    pub config: Config,
    pub whoami: ValidatorId,
}

impl<T: Adapter + 'static> SentryApi<T> {
    pub fn init(
        adapter: Arc<RwLock<T>>,
        channel: &Channel,
        config: &Config,
        logging: bool,
        whoami: &ValidatorId,
    ) -> Result<Self, ValidatorWorker> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.fetch_timeout.into()))
            .build()
            .unwrap();

        // validate that we are to validate the channel
        match channel.spec.validators.find(whoami.clone()) {
            SpecValidator::Leader(v) | SpecValidator::Follower(v) => {
                let channel_id = format!("0x{}", hex::encode(&channel.id));
                let validator_url = format!("{}/channel/{}", v.url, channel_id);

                Ok(Self {
                    adapter,
                    validator_url,
                    client,
                    logging,
                    channel: channel.to_owned(),
                    config: config.to_owned(),
                    whoami: whoami.to_owned(),
                })
            }
            SpecValidator::None => Err(ValidatorWorker::Failed(
                "we can not find validator entry for whoami".to_string(),
            )),
        }
    }

    pub async fn propagate(&self, messages: &[&MessageTypes]) {
        let mut adapter = self.adapter.write().await;

        let channel_id = format!("0x{}", hex::encode(&self.channel.id));

        for validator in self.channel.spec.validators.into_iter() {
            let auth_token = adapter.get_auth(&validator.id);

            if let Err(e) = auth_token {
                println!("propagate error: get auth failed {}", e);
                continue;
            }

            if let Err(e) = propagate_to(
                &channel_id,
                &auth_token.unwrap(),
                &self.client,
                &validator,
                messages,
            )
            .await
            {
                handle_http_error(e, &validator.url);
            }
        }
        // drop RwLock write access
        drop(adapter);
    }

    pub async fn get_latest_msg(
        &self,
        from: String,
        message_types: &[&str],
    ) -> Result<Option<MessageTypes>, reqwest::Error> {
        let message_type = message_types.join("+");
        let future = self
            .client
            .get(&format!(
                "{}/validator-messages/{}/{}?limit=1",
                self.validator_url, from, message_type
            ))
            .send()
            .and_then(|mut res: Response| res.json::<ValidatorMessageResponse>())
            .compat();

        let response = future.await?;
        match response {
            ValidatorMessageResponse::ValidatorMessages(data) => {
                if !data.is_empty() {
                    return Ok(Some(data[0].msg.clone()));
                }
                Ok(None)
            }
        }
    }

    pub async fn get_our_latest_msg(
        &self,
        message_types: &[&str],
    ) -> Result<Option<MessageTypes>, reqwest::Error> {
        self.get_latest_msg(self.whoami.to_hex_prefix_string(), message_types)
            .await
    }

    pub async fn get_last_approved(&self) -> Result<LastApprovedResponse, reqwest::Error> {
        let future = self
            .client
            .get(&format!("{}/last-approved", self.validator_url))
            .send()
            .and_then(|mut res: Response| res.json::<LastApprovedResponse>());

        future.compat().await
    }

    pub async fn get_last_msgs(&self) -> Result<LastApprovedResponse, reqwest::Error> {
        let future = self
            .client
            .get(&format!(
                "{}/last-approved?withHearbeat=true",
                self.validator_url
            ))
            .send()
            .and_then(|mut res: Response| res.json::<LastApprovedResponse>());

        future.compat().await
    }

    pub async fn get_event_aggregates(
        &self,
        after: DateTime<Utc>,
    ) -> Result<EventAggregateResponse, Box<ValidatorWorker>> {
        let auth_token = self
            .adapter
            .write()
            .await
            .get_auth(&self.whoami)
            .map_err(|e| Box::new(ValidatorWorker::Failed(e.to_string())))?;

        let url = format!(
            "{}/events-aggregates?after={}",
            self.validator_url,
            after.timestamp_millis()
        );

        let result: EventAggregateResponse = self
            .client
            .get(&url)
            .bearer_auth(&auth_token)
            .send()
            .map_err(|e| Box::new(ValidatorWorker::Failed(e.to_string())))
            .compat()
            .await?
            .json()
            .map_err(|e| Box::new(ValidatorWorker::Failed(e.to_string())))
            .compat()
            .await?;

        println!("get_event_aggregates: {:?}", result);

        Ok(result)
    }
}

async fn propagate_to(
    channel_id: &str,
    auth_token: &str,
    client: &Client,
    validator: &ValidatorDesc,
    messages: &[&MessageTypes],
) -> Result<(), reqwest::Error> {
    let url = format!(
        "{}/channel/{}/validator-messages",
        validator.url, channel_id
    );
    let mut body = HashMap::new();
    body.insert("messages", messages);

    let _response: SuccessResponse = client
        .post(&url)
        .bearer_auth(&auth_token)
        .json(&body)
        .send()
        .compat()
        .await?
        .json()
        .compat()
        .await?;

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
    whoami: String,
) -> Result<Vec<Channel>, reqwest::Error> {
    println!("whoami all_channels {}", whoami);
    let url = sentry_url.to_owned();
    let first_page = fetch_page(url.clone(), 0, whoami.clone()).await?;

    if first_page.total_pages < 2 {
        Ok(first_page.channels)
    } else {
        let mut all: Vec<ChannelAllResponse> = try_join_all(
            (1..first_page.total_pages).map(|i| fetch_page(url.clone(), i, whoami.clone())),
        )
        .await?;

        all.push(first_page);
        let result_all: Vec<Channel> = all
            .into_iter()
            .flat_map(|ch| ch.channels.into_iter())
            .collect();
        Ok(result_all)
    }
}

async fn fetch_page(
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

    future.compat().await
}
