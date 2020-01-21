use crate::error::ValidatorWorker;
use chrono::{DateTime, Utc};
use futures::future::try_join_all;
use futures::future::TryFutureExt;
use primitives::adapter::Adapter;
use primitives::channel::SpecValidator;
use primitives::sentry::{
    ChannelListResponse, EventAggregateResponse, LastApprovedResponse, SuccessResponse,
    ValidatorMessageResponse,
};
use primitives::validator::MessageTypes;
use primitives::{Channel, Config, ToETHChecksum, ValidatorDesc, ValidatorId};
use reqwest::{Client, Response};
use slog::{error, Logger};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SentryApi<T: Adapter> {
    pub adapter: T,
    pub validator_url: String,
    pub client: Client,
    pub logger: Logger,
    pub channel: Channel,
    pub config: Config,
    pub propagate_to: Vec<(ValidatorDesc, String)>,
}

impl<T: Adapter + 'static> SentryApi<T> {
    pub fn init(
        adapter: T,
        channel: &Channel,
        config: &Config,
        logger: Logger,
    ) -> Result<Self, ValidatorWorker> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.fetch_timeout.into()))
            .build()
            .map_err(|e| ValidatorWorker::Failed(format!("building Client error: {}", e)))?;

        // validate that we are to validate the channel
        match channel.spec.validators.find(adapter.whoami()) {
            SpecValidator::Leader(v) | SpecValidator::Follower(v) => {
                let channel_id = format!("0x{}", hex::encode(&channel.id));
                let validator_url = format!("{}/channel/{}", v.url, channel_id);
                let propagate_to = channel
                    .spec
                    .validators
                    .iter()
                    .map(|validator| {
                        adapter
                            .get_auth(&validator.id)
                            .map(|auth| (validator.to_owned(), auth))
                            .map_err(|e| {
                                ValidatorWorker::Failed(format!(
                                    "Propagation error: get auth failed {}",
                                    e
                                ))
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(Self {
                    adapter,
                    validator_url,
                    client,
                    logger,
                    propagate_to,
                    channel: channel.to_owned(),
                    config: config.to_owned(),
                })
            }
            SpecValidator::None => Err(ValidatorWorker::Failed(
                "We can not find validator entry for whoami".to_string(),
            )),
        }
    }

    pub async fn propagate(&self, messages: &[&MessageTypes]) {
        let channel_id = format!("0x{}", hex::encode(&self.channel.id));
        if let Err(e) = try_join_all(self.propagate_to.iter().map(|(validator, auth_token)| {
            propagate_to(&channel_id, &auth_token, &self.client, &validator, messages)
        }))
        .await
        {
            error!(&self.logger, "Propagation error: {}", e; "module" => "sentry_interface", "in" => "SentryApi");
        }
    }

    pub async fn get_latest_msg(
        &self,
        from: &ValidatorId,
        message_types: &[&str],
    ) -> Result<Option<MessageTypes>, reqwest::Error> {
        let message_type = message_types.join("+");
        let url = format!(
            "{}/validator-messages/{}/{}?limit=1",
            self.validator_url,
            from.to_checksum(),
            message_type
        );
        let result = self
            .client
            .get(&url)
            .send()
            .and_then(|res: Response| res.json::<ValidatorMessageResponse>())
            .await?;

        Ok(result.validator_messages.first().map(|m| m.msg.clone()))
    }

    pub async fn get_our_latest_msg(
        &self,
        message_types: &[&str],
    ) -> Result<Option<MessageTypes>, reqwest::Error> {
        self.get_latest_msg(self.adapter.whoami(), message_types)
            .await
    }

    pub async fn get_last_approved(&self) -> Result<LastApprovedResponse, reqwest::Error> {
        self.client
            .get(&format!("{}/last-approved", self.validator_url))
            .send()
            .and_then(|res: Response| res.json::<LastApprovedResponse>())
            .await
    }

    pub async fn get_last_msgs(&self) -> Result<LastApprovedResponse, reqwest::Error> {
        self.client
            .get(&format!(
                "{}/last-approved?withHeartbeat=true",
                self.validator_url
            ))
            .send()
            .and_then(|res: Response| res.json::<LastApprovedResponse>())
            .await
    }

    pub async fn get_event_aggregates(
        &self,
        after: DateTime<Utc>,
    ) -> Result<EventAggregateResponse, Box<ValidatorWorker>> {
        let auth_token = self
            .adapter
            .get_auth(self.adapter.whoami())
            .map_err(|e| Box::new(ValidatorWorker::Failed(e.to_string())))?;

        let url = format!(
            "{}/events-aggregates?after={}",
            self.validator_url,
            after.timestamp_millis()
        );

        self.client
            .get(&url)
            .bearer_auth(&auth_token)
            .send()
            .map_err(|e| Box::new(ValidatorWorker::Failed(e.to_string())))
            .await?
            .json()
            .map_err(|e| Box::new(ValidatorWorker::Failed(e.to_string())))
            .await
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
        .await?
        .json()
        .await?;

    Ok(())
}

pub async fn all_channels(
    sentry_url: &str,
    whoami: &ValidatorId,
) -> Result<Vec<Channel>, reqwest::Error> {
    let url = sentry_url.to_owned();
    let first_page = fetch_page(url.clone(), 0, &whoami).await?;

    if first_page.total_pages < 2 {
        Ok(first_page.channels)
    } else {
        let mut all: Vec<ChannelListResponse> =
            try_join_all((1..first_page.total_pages).map(|i| fetch_page(url.clone(), i, &whoami)))
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
    validator: &ValidatorId,
) -> Result<ChannelListResponse, reqwest::Error> {
    let client = Client::new();

    let query = [
        format!("page={}", page),
        format!("validator={}", validator.to_checksum()),
    ]
    .join("&");

    client
        .get(&format!("{}/channel/list?{}", sentry_url, query))
        .send()
        .and_then(|res: Response| res.json::<ChannelListResponse>())
        .await
}
