use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    time::Duration,
};

use futures::future::{join_all, try_join_all, TryFutureExt};
use reqwest::{Client, Method};
use slog::Logger;

use adapter::{prelude::*, Adapter};
use primitives::{
    balances::{CheckedState, UncheckedState},
    sentry::{
        AccountingResponse, AllSpendersResponse, LastApprovedResponse, SuccessResponse,
        ValidatorMessageResponse,
    },
    spender::Spender,
    util::ApiUrl,
    validator::MessageTypes,
    Address, Channel, ChannelId, Config, ValidatorId,
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

#[derive(Debug, Error)]
pub enum Error {
    #[error("Building client: {0}")]
    BuildingClient(reqwest::Error),
    #[error("Making a request: {0}")]
    Request(#[from] reqwest::Error),
    /// Error returned when the passed [`Validators`] to [`SentryApi::init()`] do not contain
    /// the _Who am I_ a record of the [`Adapter::whoami()`]
    #[error(
        "Missing validator URL & Auth token entry for whoami {whoami:#?} in the propagation list"
    )]
    WhoamiMissing { whoami: ValidatorId },
}

#[derive(Debug)]
pub struct SentryApi<C: Unlocked, P = Validators> {
    pub adapter: Adapter<C, UnlockedState>,
    pub client: Client,
    pub logger: Logger,
    pub config: Config,
    pub whoami: Validator,
    /// If set with [`Validators`], `propagate_to` should contain the `whoami` [`Validator`].
    pub propagate_to: P,
}

impl<C: Unlocked, P: Clone> Clone for SentryApi<C, P> {
    fn clone(&self) -> Self {
        Self {
            adapter: self.adapter.clone(),
            client: self.client.clone(),
            logger: self.logger.clone(),
            config: self.config.clone(),
            whoami: self.whoami.clone(),
            propagate_to: self.propagate_to.clone(),
        }
    }
}

impl<C: Unlocked + 'static> SentryApi<C, ()> {
    pub fn new(
        adapter: Adapter<C, UnlockedState>,
        logger: Logger,
        config: Config,
        whoami_validator: Validator,
    ) -> Result<SentryApi<C, ()>, Error> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.fetch_timeout.into()))
            .build()
            .map_err(Error::BuildingClient)?;

        Ok(SentryApi {
            adapter,
            client,
            logger,
            config,
            whoami: whoami_validator,
            propagate_to: (),
        })
    }

    /// Initialize the [`SentryApi`] and makes sure that [`Adapter::whoami()`] is present in [`Validators`].
    /// Sets the _Who am I_ [`ApiUrl`] and the Authentication Token for calls requiring authentication.
    pub fn init(
        adapter: Adapter<C, UnlockedState>,
        logger: Logger,
        config: Config,
        propagate_to: Validators,
    ) -> Result<SentryApi<C, Validators>, Error> {
        let whoami = propagate_to
            .get(&adapter.whoami())
            .cloned()
            .ok_or_else(|| Error::WhoamiMissing {
                whoami: adapter.whoami(),
            })?;

        let sentry_api = SentryApi::new(adapter, logger, config, whoami)?;

        Ok(sentry_api.with_propagate(propagate_to))
    }

    /// If the _Who am I_ Validator is not found in `propagate_to` it will add it.
    /// Propagation should happen to all validators Sentry instances including _Who am I_
    /// i.e. the current validator
    pub fn with_propagate(self, mut propagate_to: Validators) -> SentryApi<C, Validators> {
        let _ = propagate_to
            .entry(self.adapter.whoami())
            .or_insert_with(|| self.whoami.clone());

        SentryApi {
            adapter: self.adapter,
            client: self.client,
            logger: self.logger,
            config: self.config,
            whoami: self.whoami,
            propagate_to,
        }
    }
}

impl<C: Unlocked + 'static, P> SentryApi<C, P> {
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

    /// Get's the last approved state and requesting a [`primitives::validator::Heartbeat`], see [`LastApprovedResponse`]
    pub async fn get_last_approved(
        &self,
        channel: ChannelId,
    ) -> Result<LastApprovedResponse<UncheckedState>, Error> {
        self.client
            .get(
                self.whoami
                    .url
                    .join(&format!(
                        "v5/channel/{}/last-approved?withHeartbeat=true",
                        channel
                    ))
                    .expect("Should not error while creating endpoint"),
            )
            .send()
            .await?
            .json()
            .await
            .map_err(Error::Request)
    }

    /// page always starts from 0
    pub async fn get_spenders_page(
        &self,
        channel: &ChannelId,
        page: u64,
    ) -> Result<AllSpendersResponse, Error> {
        let url = self
            .whoami
            .url
            .join(&format!("v5/channel/{}/spender/all?page={}", channel, page))
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

    pub async fn get_all_spenders(
        &self,
        channel: ChannelId,
    ) -> Result<HashMap<Address, Spender>, Error> {
        let first_page = self.get_spenders_page(&channel, 0).await?;

        if first_page.pagination.total_pages < 2 {
            Ok(first_page.spenders)
        } else {
            let all: Vec<AllSpendersResponse> = try_join_all(
                (1..first_page.pagination.total_pages).map(|i| self.get_spenders_page(&channel, i)),
            )
            .await?;

            let result_all: HashMap<Address, Spender> = std::iter::once(first_page)
                .chain(all.into_iter())
                .flat_map(|p| p.spenders)
                .collect();

            Ok(result_all)
        }
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

        let response = self
            .client
            .get(url)
            .bearer_auth(&self.whoami.token)
            .send()
            .await?;

        assert_eq!(reqwest::StatusCode::OK, response.status());

        response
            .json::<AccountingResponse<CheckedState>>()
            .map_err(Error::Request)
            .await
    }

    /// Fetches all `Campaign`s from the _Who am I_ Sentry.
    /// It builds the `Channel`s to be processed alongside all the `Validator`s' url & auth token.
    pub async fn collect_channels(&self) -> Result<(HashSet<Channel>, Validators), Error> {
        let all_campaigns_timeout = Duration::from_millis(self.config.all_campaigns_timeout as u64);
        let client = reqwest::Client::builder()
            .timeout(all_campaigns_timeout)
            .build()?;

        let campaigns =
            campaigns::all_campaigns(client, &self.whoami, Some(self.adapter.whoami())).await?;
        let channels = campaigns
            .iter()
            .map(|campaign| campaign.channel)
            .collect::<HashSet<_>>();

        let validators = campaigns
            .into_iter()
            .fold(Validators::new(), |mut acc, campaign| {
                for validator_desc in campaign.validators.iter() {
                    // if Validator is already there, we can just skip it
                    // remember, the campaigns are ordered by `created DESC`
                    // so we will always get the latest Validator url first
                    match acc.entry(validator_desc.id) {
                        Entry::Occupied(_) => continue,
                        Entry::Vacant(entry) => {
                            // try to parse the url of the Validator Desc
                            let validator_url = validator_desc.url.parse::<ApiUrl>();
                            // and also try to find the Auth token in the config

                            // if there was an error with any of the operations, skip this `ValidatorDesc`
                            let auth_token = self.adapter.get_auth(validator_desc.id);

                            // only if `ApiUrl` parsing is `Ok` & Auth Token is found in the `Adapter`
                            if let (Ok(url), Ok(auth_token)) = (validator_url, auth_token) {
                                // add an entry for propagation
                                entry.insert(Validator {
                                    url,
                                    token: auth_token,
                                });
                            }
                            // otherwise it will try to do the same things on the next encounter of this `ValidatorId`
                        }
                    }
                }

                acc
            });

        Ok((channels, validators))
    }
}

impl<C: Unlocked + 'static> SentryApi<C> {
    pub async fn propagate(
        &self,
        channel: ChannelId,
        messages: &[&MessageTypes],
    ) -> Vec<PropagationResult> {
        join_all(self.propagate_to.iter().map(|(validator_id, validator)| {
            propagate_to::<C>(
                &self.client,
                self.config.propagation_timeout,
                channel,
                (*validator_id, validator),
                messages,
            )
        }))
        .await
    }
}

async fn propagate_to<C: Unlocked>(
    client: &Client,
    timeout: u32,
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
        .request(Method::POST, endpoint)
        .timeout(Duration::from_millis(timeout.into()))
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

pub mod channels {
    use futures::{future::try_join_all, TryFutureExt};
    use primitives::{
        sentry::channel_list::{ChannelListQuery, ChannelListResponse},
        util::ApiUrl,
        Channel, ValidatorId,
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
        sentry::campaign::{CampaignListQuery, CampaignListResponse, ValidatorParam},
        Campaign, ValidatorId,
    };
    use reqwest::Client;

    use super::Validator;

    /// Fetches all `Campaign`s from `sentry` by going through all pages and collecting the `Campaign`s into a single `Vec`
    /// You can filter by `&validator=0x...` when passing `for_validator`.
    /// This will return campaigns that include the provided `for_validator` validator.
    pub async fn all_campaigns(
        client: Client,
        whoami: &Validator,
        for_validator: Option<ValidatorId>,
    ) -> Result<Vec<Campaign>, reqwest::Error> {
        let first_page = fetch_page(&client, whoami, 0, for_validator).await?;

        if first_page.pagination.total_pages < 2 {
            Ok(first_page.campaigns)
        } else {
            let all = try_join_all(
                (1..first_page.pagination.total_pages)
                    .map(|i| fetch_page(&client, whoami, i, for_validator)),
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
        whoami: &Validator,
        page: u64,
        for_validator: Option<ValidatorId>,
    ) -> Result<CampaignListResponse, reqwest::Error> {
        let query = CampaignListQuery {
            page,
            active_to_ge: Utc::now(),
            creator: None,
            validator: for_validator.map(ValidatorParam::Validator),
        };

        let endpoint = whoami
            .url
            .join(&format!(
                "v5/campaign/list?{}",
                serde_urlencoded::to_string(query).expect("Should not fail to serialize")
            ))
            .expect("Should not fail to create endpoint URL");

        client.get(endpoint).send().await?.json().await
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use adapter::dummy::{Adapter, Dummy, Options};
    use primitives::{
        config::{configuration, Environment},
        sentry::Pagination,
        util::tests::{
            discard_logger,
            prep_db::{ADDRESSES, DUMMY_CAMPAIGN, DUMMY_VALIDATOR_LEADER, IDS},
        },
        UnifiedNum,
    };
    use std::str::FromStr;
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn test_get_all_spenders() {
        let server = MockServer::start().await;
        let test_spender = Spender {
            total_deposited: UnifiedNum::from(100_000_000),
            spender_leaf: None,
        };
        let mut all_spenders = HashMap::new();
        all_spenders.insert(ADDRESSES["user"], test_spender.clone());
        all_spenders.insert(ADDRESSES["publisher"], test_spender.clone());
        all_spenders.insert(ADDRESSES["publisher2"], test_spender.clone());
        all_spenders.insert(ADDRESSES["creator"], test_spender.clone());
        all_spenders.insert(ADDRESSES["tester"], test_spender.clone());

        let first_page_response = AllSpendersResponse {
            spenders: vec![
                (
                    ADDRESSES["user"],
                    all_spenders.get(&ADDRESSES["user"]).unwrap().to_owned(),
                ),
                (
                    ADDRESSES["publisher"],
                    all_spenders
                        .get(&ADDRESSES["publisher"])
                        .unwrap()
                        .to_owned(),
                ),
            ]
            .into_iter()
            .collect(),
            pagination: Pagination {
                page: 0,
                total_pages: 3,
            },
        };

        let second_page_response = AllSpendersResponse {
            spenders: vec![
                (
                    ADDRESSES["publisher2"],
                    all_spenders
                        .get(&ADDRESSES["publisher2"])
                        .unwrap()
                        .to_owned(),
                ),
                (
                    ADDRESSES["creator"],
                    all_spenders.get(&ADDRESSES["creator"]).unwrap().to_owned(),
                ),
            ]
            .into_iter()
            .collect(),
            pagination: Pagination {
                page: 1,
                total_pages: 3,
            },
        };

        let third_page_response = AllSpendersResponse {
            spenders: vec![(
                ADDRESSES["tester"],
                all_spenders.get(&ADDRESSES["tester"]).unwrap().to_owned(),
            )]
            .into_iter()
            .collect(),
            pagination: Pagination {
                page: 2,
                total_pages: 3,
            },
        };

        Mock::given(method("GET"))
            .and(path(format!(
                "/v5/channel/{}/spender/all",
                DUMMY_CAMPAIGN.channel.id()
            )))
            .and(query_param("page", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&first_page_response))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!(
                "/v5/channel/{}/spender/all",
                DUMMY_CAMPAIGN.channel.id()
            )))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&second_page_response))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!(
                "/v5/channel/{}/spender/all",
                DUMMY_CAMPAIGN.channel.id()
            )))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&third_page_response))
            .mount(&server)
            .await;

        let mut validators = Validators::new();
        validators.insert(
            DUMMY_VALIDATOR_LEADER.id,
            Validator {
                url: ApiUrl::from_str(&server.uri()).expect("Should parse"),
                token: AuthToken::default(),
            },
        );
        let mut config = configuration(Environment::Development, None).expect("Should get Config");
        config.spendable_find_limit = 2;

        let adapter = Adapter::with_unlocked(Dummy::init(Options {
            dummy_identity: IDS["leader"],
            dummy_auth_tokens: Default::default(),
        }));
        let logger = discard_logger();

        let sentry =
            SentryApi::init(adapter, logger, config, validators).expect("Should build sentry");

        let mut res = sentry
            .get_all_spenders(DUMMY_CAMPAIGN.channel.id())
            .await
            .expect("should get response");

        // Checks for page 1
        let res_user = res.remove(&ADDRESSES["user"]);
        let res_publisher = res.remove(&ADDRESSES["publisher"]);
        assert!(res_user.is_some() && res_publisher.is_some());
        assert_eq!(res_user.unwrap(), test_spender);
        assert_eq!(res_publisher.unwrap(), test_spender);

        // Checks for page 2
        let res_publisher2 = res.remove(&ADDRESSES["publisher2"]);
        let res_creator = res.remove(&ADDRESSES["creator"]);
        assert!(res_publisher2.is_some() && res_creator.is_some());
        assert_eq!(res_publisher2.unwrap(), test_spender);
        assert_eq!(res_creator.unwrap(), test_spender);

        // Checks for page 3
        let res_tester = res.remove(&ADDRESSES["tester"]);
        assert!(res_tester.is_some());
        assert_eq!(res_tester.unwrap(), test_spender);

        // There should be no remaining elements
        assert_eq!(res.len(), 0)
    }
}
