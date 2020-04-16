use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::future::{join_all, try_join_all, TryFutureExt};
use reqwest::{Client, Response};
use slog::Logger;

use primitives::adapter::{Adapter, AdapterErrorKind, Error as AdapterError};
use primitives::sentry::{
    ChannelListResponse, EventAggregateResponse, LastApprovedResponse, SuccessResponse,
    ValidatorMessageResponse,
};
use primitives::validator::MessageTypes;
use primitives::{Channel, ChannelId, Config, ToETHChecksum, ValidatorDesc, ValidatorId};

pub type PropagationResult<AE> = Result<ValidatorId, (ValidatorId, Error<AE>)>;

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

#[derive(Debug)]
pub enum Error<AE: AdapterErrorKind> {
    BuildingClient(reqwest::Error),
    Request(reqwest::Error),
    ValidatorAuthentication(AdapterError<AE>),
    MissingWhoamiInChannelValidators {
        channel: ChannelId,
        validators: Vec<ValidatorId>,
        whoami: ValidatorId,
    },
}

impl<AE: AdapterErrorKind> std::error::Error for Error<AE> {}

impl<AE: AdapterErrorKind> fmt::Display for Error<AE> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;

        match self {
            BuildingClient(err) => write!(f, "Building client: {}", err),
            Request(err) => write!(f, "Making a request: {}", err),
            ValidatorAuthentication(err) => {
                write!(f, "Getting authentication for validator: {}", err)
            }
            MissingWhoamiInChannelValidators {
                channel,
                validators,
                whoami,
            } => {
                let validator_ids = validators
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "We cannot find validator entry for whoami {:#?} in channel {:#?} with validators: {}",
                    whoami,
                    channel,
                    validator_ids
                )
            }
        }
    }
}

impl<A: Adapter + 'static> SentryApi<A> {
    pub fn init(
        adapter: A,
        channel: Channel,
        config: &Config,
        logger: Logger,
    ) -> Result<Self, Error<A::AdapterError>> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.fetch_timeout.into()))
            .build()
            .map_err(Error::BuildingClient)?;

        // validate that we are to validate the channel
        match channel.spec.validators.find(adapter.whoami()) {
            Some(ref spec_validator) => {
                let validator = spec_validator.validator();
                let validator_url = format!("{}/channel/{}", validator.url, channel.id);

                let propagate_to = channel
                    .spec
                    .validators
                    .iter()
                    .map(|validator| {
                        adapter
                            .get_auth(&validator.id)
                            .map(|auth| (validator.to_owned(), auth))
                            .map_err(Error::ValidatorAuthentication)
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(Self {
                    adapter,
                    validator_url,
                    client,
                    logger,
                    propagate_to,
                    channel,
                    config: config.to_owned(),
                })
            }
            None => Err(Error::MissingWhoamiInChannelValidators {
                channel: channel.id,
                validators: channel.spec.validators.iter().map(|v| v.id).collect(),
                whoami: *adapter.whoami(),
            }),
        }
    }

    pub async fn propagate(
        &self,
        messages: &[&MessageTypes],
    ) -> Vec<PropagationResult<A::AdapterError>> {
        join_all(self.propagate_to.iter().map(|(validator, auth_token)| {
            propagate_to::<A>(
                &self.channel.id,
                &auth_token,
                &self.client,
                &validator,
                messages,
            )
        }))
        .await
    }

    pub async fn get_latest_msg(
        &self,
        from: &ValidatorId,
        message_types: &[&str],
    ) -> Result<Option<MessageTypes>, Error<A::AdapterError>> {
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
            .map_err(Error::Request)
            .await?;

        Ok(result.validator_messages.first().map(|m| m.msg.clone()))
    }

    pub async fn get_our_latest_msg(
        &self,
        message_types: &[&str],
    ) -> Result<Option<MessageTypes>, Error<A::AdapterError>> {
        self.get_latest_msg(self.adapter.whoami(), message_types)
            .await
    }

    pub async fn get_last_approved(&self) -> Result<LastApprovedResponse, Error<A::AdapterError>> {
        self.client
            .get(&format!("{}/last-approved", self.validator_url))
            .send()
            .and_then(|res: Response| res.json::<LastApprovedResponse>())
            .map_err(Error::Request)
            .await
    }

    pub async fn get_last_msgs(&self) -> Result<LastApprovedResponse, Error<A::AdapterError>> {
        self.client
            .get(&format!(
                "{}/last-approved?withHeartbeat=true",
                self.validator_url
            ))
            .send()
            .and_then(|res: Response| res.json::<LastApprovedResponse>())
            .map_err(Error::Request)
            .await
    }

    pub async fn get_event_aggregates(
        &self,
        after: DateTime<Utc>,
    ) -> Result<EventAggregateResponse, Error<A::AdapterError>> {
        let auth_token = self
            .adapter
            .get_auth(self.adapter.whoami())
            .map_err(Error::ValidatorAuthentication)?;

        let url = format!(
            "{}/events-aggregates?after={}",
            self.validator_url,
            after.timestamp_millis()
        );

        self.client
            .get(&url)
            .bearer_auth(&auth_token)
            .send()
            .map_err(Error::Request)
            .await?
            .json()
            .map_err(Error::Request)
            .await
    }
}

async fn propagate_to<A: Adapter>(
    channel_id: &ChannelId,
    auth_token: &str,
    client: &Client,
    validator: &ValidatorDesc,
    messages: &[&MessageTypes],
) -> PropagationResult<A::AdapterError> {
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
        .await
        .map_err(|e| (validator.id, Error::Request(e)))?
        .json()
        .await
        .map_err(|e| (validator.id, Error::Request(e)))?;

    Ok(validator.id)
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
        let all: Vec<ChannelListResponse> =
            try_join_all((1..first_page.total_pages).map(|i| fetch_page(url.clone(), i, &whoami)))
                .await?;

        let result_all: Vec<Channel> = std::iter::once(first_page)
            .chain(all.into_iter())
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
