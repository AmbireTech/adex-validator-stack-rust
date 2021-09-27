use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, Utc};
use futures::future::{join_all, TryFutureExt};
use reqwest::{Client, Response};
use slog::Logger;

use primitives::{
    adapter::Adapter,
    balances::{CheckedState, UncheckedState},
    channel_v5::Channel,
    sentry::{
        AccountingResponse, EventAggregateResponse, LastApprovedResponse, SuccessResponse,
        ValidatorMessageResponse,
    },
    spender::Spender,
    util::ApiUrl,
    validator::MessageTypes,
    Address, Campaign, {ChannelId, Config, ValidatorId},
};
use thiserror::Error;

pub type PropagationResult = Result<ValidatorId, (ValidatorId, Error)>;
/// Propagate the Validator messages to these `Validator`s
pub type Validators = HashMap<ValidatorId, Validator>;
pub type AuthToken = String;

#[derive(Debug, Clone)]
pub struct Validator {
    /// Sentry API url
    pub url: ApiUrl,
    /// Authentication token
    pub token: AuthToken,
}

#[derive(Debug, Clone)]
pub struct SentryApi<A: Adapter> {
    pub adapter: A,
    pub client: Client,
    pub logger: Logger,
    pub config: Config,
    pub whoami: Validator,
    pub propagate_to: Validators,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Building client: {0}")]
    BuildingClient(reqwest::Error),
    #[error("Making a request: {0}")]
    Request(#[from] reqwest::Error),
    #[error(
        "Missing validator URL & Auth token entry for whoami {whoami:#?} in the propagation list"
    )]
    WhoamiMissing { whoami: ValidatorId },
    #[error("Failed to parse validator url: {0}")]
    ValidatorUrl(#[from] primitives::util::api::ParseError),
}

impl<A: Adapter + 'static> SentryApi<A> {
    pub fn init(
        adapter: A,
        logger: Logger,
        config: Config,
        propagate_to: Validators,
    ) -> Result<Self, Error> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.fetch_timeout.into()))
            .build()
            .map_err(Error::BuildingClient)?;

        let whoami = propagate_to
            .get(&adapter.whoami())
            .cloned()
            .ok_or_else(|| Error::WhoamiMissing {
                whoami: adapter.whoami(),
            })?;

        Ok(Self {
            adapter,
            client,
            logger,
            config,
            whoami,
            propagate_to,
        })
    }

    pub async fn propagate(
        &self,
        channel: ChannelId,
        messages: &[&MessageTypes],
    ) -> Vec<PropagationResult> {
        join_all(self.propagate_to.iter().map(|(validator_id, validator)| {
            propagate_to::<A>(&self.client, channel, (*validator_id, validator), messages)
        }))
        .await
    }

    pub async fn get_latest_msg(
        &self,
        channel: ChannelId,
        from: ValidatorId,
        message_types: &[&str],
    ) -> Result<Option<MessageTypes>, Error> {
        let message_type = message_types.join("+");

        let endpoint = self
            .whoami
            .url
            .join(&format!(
                "v5/channel/{}/validator-messages/{}/{}?limit=1",
                channel, from, message_type
            ))
            .expect("Should not error when creating endpoint url");

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
        channel: ChannelId,
        message_types: &[&str],
    ) -> Result<Option<MessageTypes>, Error> {
        self.get_latest_msg(channel, self.adapter.whoami(), message_types)
            .await
    }

    pub async fn get_last_approved(
        &self,
        channel: ChannelId,
    ) -> Result<LastApprovedResponse<UncheckedState>, Error> {
        self.client
            .get(
                self.whoami
                    .url
                    .join(&format!("v5/channel/{}/last-approved", channel))
                    .expect("Should not error while creating endpoint"),
            )
            .send()
            .await?
            .json()
            .await
            .map_err(Error::Request)
    }

    pub async fn get_last_msgs(&self) -> Result<LastApprovedResponse<UncheckedState>, Error> {
        self.client
            .get(
                self.whoami
                    .url
                    .join("last-approved?withHeartbeat=true")
                    .expect("Should not error while creating endpoint"),
            )
            .send()
            .and_then(|res: Response| res.json::<LastApprovedResponse<UncheckedState>>())
            .map_err(Error::Request)
            .await
    }

    // TODO: Pagination & use of `AllSpendersResponse`
    pub async fn get_all_spenders(
        &self,
        channel: ChannelId,
    ) -> Result<HashMap<Address, Spender>, Error> {
        let url = self
            .whoami
            .url
            .join(&format!("v5/channel/{}/spender/all", channel))
            .expect("Should not error when creating endpoint");

        self.client
            .get(url)
            .bearer_auth(&self.whoami.token)
            .send()
            .await?
            // TODO: Should be `AllSpendersResponse` and should have pagination!
            .json()
            .map_err(Error::Request)
            .await
    }

    /// Get the accounting from Sentry
    /// `Balances` should always be in `CheckedState`
    pub async fn get_accounting(
        &self,
        channel: ChannelId,
    ) -> Result<AccountingResponse<CheckedState>, Error> {
        let url = self
            .whoami
            .url
            .join(&format!("v5/channel/{}/accounting", channel))
            .expect("Should not error when creating endpoint");

        self.client
            .get(url)
            .bearer_auth(&self.whoami.token)
            .send()
            .await?
            .json::<AccountingResponse<CheckedState>>()
            .map_err(Error::Request)
            .await
    }

    /// Fetches all `Campaign`s from `sentry` by going through all pages and collecting the `Campaign`s into a single `Vec`
    pub async fn all_campaigns(&self) -> Result<Vec<Campaign>, Error> {
        Ok(
            campaigns::all_campaigns(self.client.clone(), &self.whoami.url, self.adapter.whoami())
                .await?,
        )
    }

    pub async fn all_channels(&self) -> Result<Vec<Channel>, Error> {
        Ok(
            channels::all_channels(self.client.clone(), &self.whoami.url, self.adapter.whoami())
                .await?,
        )
    }

    #[deprecated = "V5 no longer needs event aggregates"]
    pub async fn get_event_aggregates(
        &self,
        after: DateTime<Utc>,
    ) -> Result<EventAggregateResponse, Error> {
        let url = self
            .whoami
            .url
            .join(&format!(
                "events-aggregates?after={}",
                after.timestamp_millis()
            ))
            .expect("Should not error when creating endpoint");

        self.client
            .get(url)
            .bearer_auth(&self.whoami.token)
            .send()
            .await?
            .json()
            .map_err(Error::Request)
            .await
    }
}

async fn propagate_to<A: Adapter>(
    client: &Client,
    channel_id: ChannelId,
    (validator_id, validator): (ValidatorId, &Validator),
    messages: &[&MessageTypes],
) -> PropagationResult {
    let endpoint = validator
        .url
        .join(&format!("v5/channel/{}/validator-messages", channel_id))
        .expect("Should not error when creating endpoint url");

    let mut body = HashMap::new();
    body.insert("messages", messages);

    let _response: SuccessResponse = client
        .post(endpoint)
        .bearer_auth(&validator.token)
        .json(&body)
        .send()
        .await
        .map_err(|e| (validator_id, Error::Request(e)))?
        .json()
        .await
        .map_err(|e| (validator_id, Error::Request(e)))?;

    Ok(validator_id)
}

mod channels {
    use futures::{future::try_join_all, TryFutureExt};
    use primitives::{
        channel_v5::Channel,
        sentry::channel_list::{ChannelListQuery, ChannelListResponse},
        util::ApiUrl,
        ValidatorId,
    };
    use reqwest::{Client, Response};

    pub async fn all_channels(
        client: Client,
        sentry_url: &ApiUrl,
        whoami: ValidatorId,
    ) -> Result<Vec<Channel>, reqwest::Error> {
        let first_page = fetch_page(&client, sentry_url, 0, whoami).await?;

        if first_page.pagination.total_pages < 2 {
            Ok(first_page.channels)
        } else {
            let all: Vec<ChannelListResponse> = try_join_all(
                (1..first_page.pagination.total_pages)
                    .map(|i| fetch_page(&client, sentry_url, i, whoami)),
            )
            .await?;

            let result_all: Vec<Channel> = std::iter::once(first_page)
                .chain(all.into_iter())
                .flat_map(|ch| ch.channels.into_iter())
                .collect();
            Ok(result_all)
        }
    }

    async fn fetch_page(
        client: &Client,
        sentry_url: &ApiUrl,
        page: u64,
        validator: ValidatorId,
    ) -> Result<ChannelListResponse, reqwest::Error> {
        let query = ChannelListQuery {
            page,
            creator: None,
            validator: Some(validator),
        };

        let endpoint = sentry_url
            .join(&format!(
                "v5/channel/list?{}",
                serde_urlencoded::to_string(query).expect("Should not fail to serialize")
            ))
            .expect("Should not fail to create endpoint URL");

        client
            .get(endpoint)
            .send()
            .and_then(|res: Response| res.json::<ChannelListResponse>())
            .await
    }
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
        client: Client,
        sentry_url: &ApiUrl,
        whoami: ValidatorId,
    ) -> Result<Vec<Campaign>, reqwest::Error> {
        let first_page = fetch_page(&client, sentry_url, 0, whoami).await?;

        if first_page.pagination.total_pages < 2 {
            Ok(first_page.campaigns)
        } else {
            let all = try_join_all(
                (1..first_page.pagination.total_pages)
                    .map(|i| fetch_page(&client, sentry_url, i, whoami)),
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
        client: &Client,
        sentry_url: &ApiUrl,
        page: u64,
        validator: ValidatorId,
    ) -> Result<CampaignListResponse, reqwest::Error> {
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
