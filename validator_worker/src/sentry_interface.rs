use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, Utc};
use futures::future::{join_all, try_join_all, TryFutureExt};
use reqwest::{Client, Response};
use slog::Logger;

use primitives::{
    adapter::{Adapter, AdapterErrorKind, Error as AdapterError},
    balances::UncheckedState,
    channel_v5::Channel,
    channel::Channel as ChannelOld,
    sentry::{
        ChannelListResponse, EventAggregateResponse, LastApprovedResponse, SuccessResponse,
        ValidatorMessageResponse,
    },
    util::ApiUrl,
    validator::MessageTypes,
    {ChannelId, Config, ToETHChecksum, ValidatorId},
};
use thiserror::Error;

pub type PropagationResult<AE> = Result<ValidatorId, (ValidatorId, Error<AE>)>;
/// Propagate the Validator messages to these `Validator`s
pub type Validators = HashMap<ValidatorId, (ApiUrl, AuthToken)>;
pub type AuthToken = String;

#[derive(Debug, Clone)]
pub struct SentryApi<A: Adapter> {
    pub adapter: A,
    pub client: Client,
    pub logger: Logger,
    pub config: Config,
    pub whoami_url: ApiUrl,
    pub channel: Channel,
    pub propagate_to: Validators,
}

#[derive(Debug, Error)]
pub enum Error<AE: AdapterErrorKind + 'static> {
    #[error("Building client: {0}")]
    BuildingClient(reqwest::Error),
    #[error("Making a request: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Getting authentication for validator: {0}")]
    ValidatorAuthentication(#[from] AdapterError<AE>),
    #[error(
        "Missing validator URL entry for whoami {whoami:#?} in the Channel {channel:#?} propagation list"
    )]
    WhoamiUrlMissing {
        channel: ChannelId,
        whoami: ValidatorId,
    },
    #[error("Failed to parse validator url: {0}")]
    ValidatorUrl(#[from] primitives::util::api::ParseError),
}

impl<A: Adapter + 'static> SentryApi<A> {
    pub fn init(
        adapter: A,
        logger: Logger,
        config: Config,
        (channel, propagate_to): (Channel, Validators),
    ) -> Result<Self, Error<A::AdapterError>> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.fetch_timeout.into()))
            .build()
            .map_err(Error::BuildingClient)?;

        let (whoami_url, _whoami_token) =
            propagate_to
                .get(&adapter.whoami())
                .cloned()
                .ok_or_else(|| Error::WhoamiUrlMissing {
                    channel: channel.id(),
                    whoami: adapter.whoami(),
                })?;

        Ok(Self {
            adapter,
            client,
            logger,
            config,
            whoami_url,
            channel,
            propagate_to,
        })
    }

    pub async fn propagate(
        &self,
        messages: &[&MessageTypes],
    ) -> Vec<PropagationResult<A::AdapterError>> {
        join_all(
            self.propagate_to
                .iter()
                .map(|(validator, (url, auth_token))| {
                    propagate_to::<A>(
                        self.channel.id(),
                        &self.client,
                        *validator,
                        url,
                        auth_token,
                        messages,
                    )
                }),
        )
        .await
    }

    pub async fn get_latest_msg(
        &self,
        from: &ValidatorId,
        message_types: &[&str],
    ) -> Result<Option<MessageTypes>, Error<A::AdapterError>> {
        let message_type = message_types.join("+");

        let endpoint = self.whoami_url.join(&format!(
            "/validator-messages/{}/{}?limit=1",
            from.to_checksum(),
            message_type
        ))?;

        let result = self
            .client
            .get(endpoint)
            .send()
            .await?
            .json::<ValidatorMessageResponse>()
            .await?;

        Ok(result.validator_messages.into_iter().next().map(|m| m.msg))
    }

    pub async fn get_our_latest_msg(
        &self,
        message_types: &[&str],
    ) -> Result<Option<MessageTypes>, Error<A::AdapterError>> {
        self.get_latest_msg(&self.adapter.whoami(), message_types)
            .await
    }

    pub async fn get_last_approved(
        &self,
    ) -> Result<LastApprovedResponse<UncheckedState>, Error<A::AdapterError>> {
        self.client
            .get(
                self.whoami_url
                    .join("last-approved")
                    .expect("Should not error while creating endpoint"),
            )
            .send()
            .await?
            .json()
            .await
            .map_err(Error::Request)
    }

    pub async fn get_last_msgs(
        &self,
    ) -> Result<LastApprovedResponse<UncheckedState>, Error<A::AdapterError>> {
        self.client
            .get(
                self.whoami_url
                    .join("last-approved?withHeartbeat=true")
                    .expect("Should not error while creating endpoint"),
            )
            .send()
            .and_then(|res: Response| res.json::<LastApprovedResponse<UncheckedState>>())
            .map_err(Error::Request)
            .await
    }

    pub async fn get_event_aggregates(
        &self,
        after: DateTime<Utc>,
    ) -> Result<EventAggregateResponse, Error<A::AdapterError>> {
        let auth_token = self
            .adapter
            .get_auth(&self.adapter.whoami())
            .map_err(Error::ValidatorAuthentication)?;

        let url = self
            .whoami_url
            .join(&format!(
                "events-aggregates?after={}",
                after.timestamp_millis()
            ))
            .expect("Should not error when creating endpoint");

        self.client
            .get(url)
            .bearer_auth(&auth_token)
            .send()
            .await?
            .json()
            .map_err(Error::Request)
            .await
    }
}

async fn propagate_to<A: Adapter>(
    channel_id: ChannelId,
    client: &Client,
    validator: ValidatorId,
    validator_url: &ApiUrl,
    auth_token: &str,
    messages: &[&MessageTypes],
) -> PropagationResult<A::AdapterError> {
    let endpoint = validator_url.join(&format!(
        "channel/{}/validator-messages",
        channel_id
    )).expect("Should not error when creating endpoint url");

    let mut body = HashMap::new();
    body.insert("messages", messages);

    let _response: SuccessResponse = client
        .post(endpoint)
        .bearer_auth(&auth_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| (validator, Error::Request(e)))?
        .json()
        .await
        .map_err(|e| (validator, Error::Request(e)))?;

    Ok(validator)
}

pub async fn all_channels(
    sentry_url: &str,
    whoami: &ValidatorId,
) -> Result<Vec<ChannelOld>, reqwest::Error> {
    let url = sentry_url.to_owned();
    let first_page = fetch_page(url.clone(), 0, whoami).await?;

    if first_page.total_pages < 2 {
        Ok(first_page.channels)
    } else {
        let all: Vec<ChannelListResponse> =
            try_join_all((1..first_page.total_pages).map(|i| fetch_page(url.clone(), i, whoami)))
                .await?;

        let result_all: Vec<ChannelOld> = std::iter::once(first_page)
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

pub mod campaigns {
    use chrono::Utc;
    use futures::future::try_join_all;
    use primitives::{
        sentry::campaign::{CampaignListQuery, CampaignListResponse},
        util::ApiUrl,
        Campaign, ValidatorId,
    };
    use reqwest::Client;

    /// Fetches all `Campaign`s from `sentry` by going through all pages and collecting the `Campaign`s into a single `Vec`
    pub async fn all_campaigns(
        sentry_url: &ApiUrl,
        whoami: ValidatorId,
    ) -> Result<Vec<Campaign>, reqwest::Error> {
    let first_page = fetch_page(sentry_url, 0, whoami).await?;

        if first_page.pagination.total_pages < 2 {
            Ok(first_page.campaigns)
        } else {
            let all = try_join_all(
                (1..first_page.pagination.total_pages).map(|i| fetch_page(sentry_url, i, whoami)),
            )
            .await?;

            let result_all = std::iter::once(first_page)
                .chain(all.into_iter())
                .flat_map(|response| response.campaigns.into_iter())
                .collect();
            Ok(result_all)
        }
    }

    async fn fetch_page(
        sentry_url: &ApiUrl,
        page: u64,
        validator: ValidatorId,
    ) -> Result<CampaignListResponse, reqwest::Error> {
        let client = Client::new();
        let query = CampaignListQuery {
            page,
            active_to_ge: Utc::now(),
            creator: None,
            validator: Some(validator),
        };

        let endpoint = sentry_url
            .join(&format!(
                "campaign/list?{}",
                serde_urlencoded::to_string(query).expect("Should not fail to serialize")
            ))
            .expect("Should not fail to create endpoint URL");

        client.get(endpoint).send().await?.json().await
    }
}
