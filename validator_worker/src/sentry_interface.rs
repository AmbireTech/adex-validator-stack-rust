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
        validator_messages::{
            MessageTypesFilter, ValidatorMessagesCreateRequest, ValidatorMessagesListResponse,
        },
        AccountingResponse, AllSpendersResponse, LastApprovedResponse, SuccessResponse,
    },
    spender::Spender,
    util::ApiUrl,
    validator::{MessageType, MessageTypes},
    Address, ChainId, ChainOf, Channel, ChannelId, Config, ValidatorId,
};
use thiserror::Error;

pub type PropagationResult = Result<ValidatorId, (ValidatorId, Error)>;
pub type ChainsValidators = HashMap<ChainId, Validators>;
/// Propagate the Validator messages to these `Validator`s
/// This map contains the Validator Auth token & Url for a specific Chain
pub type Validators = HashMap<ValidatorId, Validator>;
pub type AuthToken = String;

#[derive(Debug, Clone, PartialEq, Eq)]
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
        "Missing validator URL & Auth token entry for whoami {whoami:#?} on chain {chain_id:#?} in the propagation list"
    )]
    WhoamiMissing {
        whoami: ValidatorId,
        chain_id: ChainId,
    },
    #[error("We can propagate only to Chains which are whiteslisted for this validator.")]
    ChainNotWhitelisted { chain_id: ChainId },
    #[error("Failed to generate authentication token using the Adapter for {for_chain:?}")]
    AuthenticationToken { for_chain: ChainId },
    #[error("Not all channel validators were found in the propagation list")]
    PropagationValidatorsNotFound {
        channel: Vec<ValidatorId>,
        found: HashMap<ValidatorId, Validator>,
    },
}

#[derive(Debug)]
pub struct SentryApi<C: Unlocked, P = ChainsValidators> {
    pub adapter: Adapter<C, UnlockedState>,
    pub client: Client,
    pub logger: Logger,
    pub config: Config,
    /// For all the calls that do not have information about the Chains
    pub sentry_url: ApiUrl,
    /// Whilelisted chains for which this validator (_Who Am I_) can operate on.
    ///
    /// Since the validator might have different urls for old vs new Campaigns,
    /// we can override the URL based on the campaign, see [`crate::Worker`].
    /// Auth token for this validator is generated for each Chain on [`SentryApi::new`]
    pub whoami: HashMap<ChainId, Validator>,
    /// If set with [`Validators`], `propagate_to` should contain the `whoami` [`Validator`] in each Chain!
    /// use [`SentryApi::init`] or [`SentryApi::with_propagate`] instead
    pub propagate_to: P,
}

impl<C: Unlocked, P: Clone> Clone for SentryApi<C, P> {
    fn clone(&self) -> Self {
        Self {
            adapter: self.adapter.clone(),
            client: self.client.clone(),
            logger: self.logger.clone(),
            config: self.config.clone(),
            sentry_url: self.sentry_url.clone(),
            whoami: self.whoami.clone(),
            propagate_to: self.propagate_to.clone(),
        }
    }
}

impl<C: Unlocked + 'static> SentryApi<C, ()> {
    /// `sentry_url` is the default URL to which the current _Who am I_ validator should make requests.
    /// It is used to populate the config Chains with Authentication Token & [`ApiUrl`].
    /// This value can be overwritten using `propagate_to`,
    /// if any of the passed validators has the same [`ValidatorId`].
    pub fn new(
        adapter: Adapter<C, UnlockedState>,
        logger: Logger,
        config: Config,
        sentry_url: ApiUrl,
    ) -> Result<SentryApi<C, ()>, Error> {
        let client = Client::builder()
            .timeout(config.fetch_timeout)
            .build()
            .map_err(Error::BuildingClient)?;

        let whoami = config
            .chains
            .values()
            .map(
                |chain_info| match adapter.get_auth(chain_info.chain.chain_id, adapter.whoami()) {
                    Ok(auth_token) => {
                        let validator = Validator {
                            url: sentry_url.clone(),
                            token: auth_token,
                        };

                        Ok((chain_info.chain.chain_id, validator))
                    }
                    Err(_adapter_err) => Err(Error::AuthenticationToken {
                        for_chain: chain_info.chain.chain_id,
                    }),
                },
            )
            .collect::<Result<HashMap<_, _>, _>>()?;

        Ok(SentryApi {
            adapter,
            client,
            logger,
            config,
            sentry_url,
            whoami,
            propagate_to: (),
        })
    }

    /// Initialize the [`SentryApi`] and makes sure that [`Adapter::whoami()`] is present in each chain [`Validators`].
    /// Sets the _Who am I_ [`ApiUrl`] and the Authentication Token for a specific Chain for calls that require authentication.
    pub fn init(
        adapter: Adapter<C, UnlockedState>,
        logger: Logger,
        config: Config,
        sentry_url: ApiUrl,
        propagate_to: ChainsValidators,
    ) -> Result<SentryApi<C, ChainsValidators>, Error> {
        let sentry_api = SentryApi::new(adapter, logger, config, sentry_url)?;

        sentry_api.with_propagate(propagate_to)
    }

    /// If the _Who am I_ Validator is not found in `propagate_to` it will be added.
    /// Propagation should happen to all validators Sentry instances including _Who am I_
    /// i.e. the current validator.
    /// If a Chain in propagate_to is not setup ([`SentryApi::whoami`]) for this instance, an error is returned.
    pub fn with_propagate(
        self,
        mut propagate_to: ChainsValidators,
    ) -> Result<SentryApi<C, ChainsValidators>, Error> {
        for (chain_id, validators) in propagate_to.iter_mut() {
            // validate that the chain is whiteslited
            let whoami_validator = self
                .whoami
                .get(chain_id)
                .ok_or(Error::ChainNotWhitelisted {
                    chain_id: *chain_id,
                })?;

            // if _Who Am I_ is not found, insert from the setup Chains whoami
            validators
                .entry(self.adapter.whoami())
                .or_insert_with(|| whoami_validator.clone());
        }

        Ok(SentryApi {
            adapter: self.adapter,
            client: self.client,
            logger: self.logger,
            config: self.config,
            sentry_url: self.sentry_url,
            whoami: self.whoami,
            propagate_to,
        })
    }
}

impl<C: Unlocked + 'static, P> SentryApi<C, P> {
    pub async fn get_latest_msg(
        &self,
        channel: ChannelId,
        from: ValidatorId,
        message_types: &[MessageType],
    ) -> Result<Option<MessageTypes>, Error> {
        let messages_filter = MessageTypesFilter(message_types.to_vec());
        let messages_encoded = urlencoding::Encoded(messages_filter.to_string());

        let endpoint = self
            .sentry_url
            .join(&format!(
                "v5/channel/{}/validator-messages/{}/{}?limit=1",
                channel, from, messages_encoded
            ))
            .expect("Should not error when creating endpoint url");

        let response = self
            .client
            .get(endpoint)
            .send()
            .await?
            .json::<ValidatorMessagesListResponse>()
            .await?;

        Ok(response.messages.into_iter().next().map(|m| m.msg))
    }

    pub async fn get_our_latest_msg(
        &self,
        channel: ChannelId,
        message_types: &[MessageType],
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
                self.sentry_url
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
        channel_context: &ChainOf<Channel>,
        page: u64,
    ) -> Result<AllSpendersResponse, Error> {
        let channel_id = channel_context.context.id();
        let url = self
            .sentry_url
            .join(&format!(
                "v5/channel/{}/spender/all?page={}",
                channel_id, page
            ))
            .expect("Should not error when creating endpoint");

        let auth_token = self
            .adapter
            .get_auth(channel_context.chain.chain_id, self.adapter.whoami())
            .map_err(|_adapter_err| Error::AuthenticationToken {
                for_chain: channel_context.chain.chain_id,
            })?;

        self.client
            .get(url)
            .bearer_auth(&auth_token)
            .send()
            .await?
            .json()
            .map_err(Error::Request)
            .await
    }

    pub async fn get_all_spenders(
        &self,
        channel_context: &ChainOf<Channel>,
    ) -> Result<HashMap<Address, Spender>, Error> {
        let first_page = self.get_spenders_page(channel_context, 0).await?;

        if first_page.pagination.total_pages < 2 {
            Ok(first_page.spenders)
        } else {
            let all: Vec<AllSpendersResponse> = try_join_all(
                (1..first_page.pagination.total_pages)
                    .map(|i| self.get_spenders_page(channel_context, i)),
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
        channel_context: &ChainOf<Channel>,
    ) -> Result<AccountingResponse<CheckedState>, Error> {
        let url = self
            .sentry_url
            .join(&format!(
                "v5/channel/{}/accounting",
                channel_context.context.id()
            ))
            .expect("Should not error when creating endpoint");

        let auth_token = self
            .adapter
            .get_auth(channel_context.chain.chain_id, self.adapter.whoami())
            .map_err(|_adapter_err| Error::AuthenticationToken {
                for_chain: channel_context.chain.chain_id,
            })?;

        let response = self.client.get(url).bearer_auth(auth_token).send().await?;

        assert_eq!(reqwest::StatusCode::OK, response.status());

        response
            .json::<AccountingResponse<CheckedState>>()
            .map_err(Error::Request)
            .await
    }

    /// Fetches all `Campaign`s from the _Who am I_ Sentry.
    /// It builds the `Channel`s to be processed alongside all the `Validator`s' url & auth token.
    pub async fn collect_channels(
        &self,
    ) -> Result<(HashSet<ChainOf<Channel>>, ChainsValidators), Error> {
        let client = reqwest::Client::builder()
            .timeout(self.config.all_campaigns_timeout)
            .build()?;

        let campaigns =
            campaigns::all_campaigns(client, &self.sentry_url, Some(self.adapter.whoami())).await?;

        let (validators, channels) = campaigns.into_iter().fold(
            (ChainsValidators::new(), HashSet::<ChainOf<Channel>>::new()),
            |(mut validators, mut channels), campaign| {
                let channel_context = match self.config.find_chain_of(campaign.channel.token) {
                    Some(chain_of) => chain_of.with_channel(campaign.channel),
                    // Skip the current Channel as the Chain/Token is not configured
                    None => return (validators, channels),
                };

                // prepare to populate the chain of the Campaign validators
                let chain_validators = validators
                    .entry(channel_context.chain.chain_id)
                    .or_default();

                for validator_desc in campaign.validators.iter() {
                    // if Validator is already there, we can just skip it
                    // remember, the campaigns are ordered by `created DESC`
                    // so we will always get the latest Validator url first
                    match chain_validators.entry(validator_desc.id) {
                        Entry::Occupied(_) => continue,
                        Entry::Vacant(entry) => {
                            // try to parse the url of the Validator Desc
                            let validator_url = validator_desc.url.parse::<ApiUrl>();
                            // and also try to find the Auth token in the config

                            // if there was an error with any of the operations, skip this `ValidatorDesc`
                            let auth_token = self
                                .adapter
                                .get_auth(channel_context.chain.chain_id, validator_desc.id);

                            // only if `ApiUrl` parsing is `Ok` & Auth Token is found in the `Adapter`
                            if let (Ok(url), Ok(auth_token)) = (validator_url, auth_token) {
                                // add an entry for propagation
                                entry.insert(Validator {
                                    url,
                                    token: auth_token,
                                });
                            }
                            // otherwise it will try to do the same things on the next encounter of this
                            // `ValidatorId` for the particular `Chain`
                        }
                    }
                }

                // last but not least insert the channel!
                channels.insert(channel_context);

                (validators, channels)
            },
        );

        Ok((channels, validators))
    }
}

pub fn assert_result<E>(assert: bool, or_error: E) -> Result<(), E> {
    if assert {
        Ok(())
    } else {
        Err(or_error)
    }
}

impl<C: Unlocked + 'static> SentryApi<C> {
    pub async fn propagate(
        &self,
        channel_context: &ChainOf<Channel>,
        messages: &[MessageTypes],
    ) -> Result<Vec<PropagationResult>, Error> {
        let chain_validators = self
            .propagate_to
            .get(&channel_context.chain.chain_id)
            .ok_or(Error::ChainNotWhitelisted {
                chain_id: channel_context.chain.chain_id,
            })?;

        let channel_validators = [
            channel_context.context.leader,
            channel_context.context.follower,
        ];

        let propagate_to_validators = channel_validators
            .iter()
            .filter_map(|channel_validator| {
                chain_validators
                    .get(channel_validator)
                    .cloned()
                    .map(|validator| (*channel_validator, validator))
            })
            .collect::<HashMap<_, _>>();

        // check if we found all the channel validators in the propagation list
        if propagate_to_validators.len() != channel_validators.len() {
            return Err(Error::PropagationValidatorsNotFound {
                channel: channel_validators.to_vec(),
                found: propagate_to_validators,
            });
        }

        let propagation_results = join_all(propagate_to_validators.iter().map(
            |(validator_id, validator)| {
                propagate_to::<C>(
                    &self.client,
                    self.config.propagation_timeout,
                    channel_context.context.id(),
                    (*validator_id, validator),
                    messages,
                )
            },
        ))
        .await;

        Ok(propagation_results)
    }
}

async fn propagate_to<C: Unlocked>(
    client: &Client,
    timeout: Duration,
    channel_id: ChannelId,
    (validator_id, validator): (ValidatorId, &Validator),
    messages: &[MessageTypes],
) -> PropagationResult {
    let endpoint = validator
        .url
        .join(&format!("v5/channel/{}/validator-messages", channel_id))
        .expect("Should not error when creating endpoint url");

    let request_body = ValidatorMessagesCreateRequest {
        messages: messages.to_vec(),
    };

    let _response: SuccessResponse = client
        .request(Method::POST, endpoint)
        .timeout(timeout)
        .bearer_auth(&validator.token)
        .json(&request_body)
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
            validator: Some(validator),
            chains: vec![],
        };

        let endpoint = sentry_url
            .join(&format!(
                "v5/channel/list?{}",
                serde_qs::to_string(&query).expect("Should not fail to serialize")
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
        sentry::campaign_list::{CampaignListQuery, CampaignListResponse, ValidatorParam},
        util::ApiUrl,
        Campaign, ValidatorId,
    };
    use reqwest::Client;

    /// Fetches all `Campaign`s from `sentry` by going through all pages and collecting the `Campaign`s into a single `Vec`
    /// You can filter by `&validator=0x...` when passing `for_validator`.
    /// This will return campaigns that include the provided `for_validator` validator.
    pub async fn all_campaigns(
        client: Client,
        sentry_url: &ApiUrl,
        for_validator: Option<ValidatorId>,
    ) -> Result<Vec<Campaign>, reqwest::Error> {
        let first_page = fetch_page(&client, sentry_url, 0, for_validator).await?;

        if first_page.pagination.total_pages < 2 {
            Ok(first_page.campaigns)
        } else {
            let all = try_join_all(
                (1..first_page.pagination.total_pages)
                    .map(|i| fetch_page(&client, sentry_url, i, for_validator)),
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
        for_validator: Option<ValidatorId>,
    ) -> Result<CampaignListResponse, reqwest::Error> {
        let query = CampaignListQuery {
            page,
            active_to_ge: Utc::now(),
            creator: None,
            validator: for_validator.map(ValidatorParam::Validator),
        };

        let endpoint = sentry_url
            .join(&format!(
                "v5/campaign/list?{}",
                serde_qs::to_string(&query).expect("Should not fail to serialize")
            ))
            .expect("Should not fail to create endpoint URL");

        client.get(endpoint).send().await?.json().await
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use adapter::{
        dummy::{Adapter, Dummy, Options},
        ethereum::test_util::GANACHE_INFO_1,
    };
    use primitives::{
        campaign::validators::Validators as CampaignValidators,
        config::{configuration, Environment, GANACHE_CONFIG},
        sentry::{
            campaign_list::CampaignListResponse, channel_list::ChannelListResponse, Pagination,
        },
        test_util::{
            discard_logger, ADVERTISER, ADVERTISER_2, CREATOR, DUMMY_AUTH, DUMMY_CAMPAIGN,
            DUMMY_VALIDATOR_FOLLOWER, DUMMY_VALIDATOR_LEADER, FOLLOWER, GUARDIAN, IDS, LEADER,
            LEADER_2, PUBLISHER, PUBLISHER_2,
        },
        CampaignId, UnifiedNum, ValidatorDesc,
    };
    use std::str::FromStr;
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    #[test]
    fn test_message_types_filter_encoding() {
        let cases = [
            ("", vec![]),
            (
                "NewState%2BApproveState",
                vec![MessageType::NewState, MessageType::ApproveState],
            ),
            (
                "NewState%2BApproveState%2BRejectState",
                vec![
                    MessageType::NewState,
                    MessageType::ApproveState,
                    MessageType::RejectState,
                ],
            ),
        ];

        for (expected_urlencoded, message_types) in cases {
            let filter = MessageTypesFilter(message_types);
            let message_types_filter = urlencoding::Encoded(filter.to_string());

            assert_eq!(expected_urlencoded, message_types_filter.to_str());
            assert_eq!(expected_urlencoded, format!("{message_types_filter}"));
        }
    }

    /// Uses the [`Dummy`] adapter with [`DUMMY_AUTH`] as the authentication tokens.
    /// Sentry url can be provided, for `wiremock` to be able to mock the calls in [`SentryApi`].
    pub fn setup_dummy_sentry(
        whoami: ValidatorId,
        config: Config,
        sentry_url: ApiUrl,
    ) -> SentryApi<Dummy, ()> {
        let adapter = Adapter::with_unlocked(Dummy::init(Options {
            dummy_identity: whoami,
            dummy_auth_tokens: DUMMY_AUTH.clone(),
            dummy_chains: config.chains.values().cloned().collect(),
        }));
        let logger = discard_logger();

        SentryApi::new(adapter, logger, config, sentry_url).expect("Should build sentry")
    }

    #[tokio::test]
    async fn test_get_all_spenders() {
        let server = MockServer::start().await;
        let test_spender = Spender {
            total_deposited: UnifiedNum::from(100_000_000),
            total_spent: None,
        };
        let mut all_spenders = HashMap::new();
        all_spenders.insert(*ADVERTISER, test_spender.clone());
        all_spenders.insert(*PUBLISHER, test_spender.clone());
        all_spenders.insert(*PUBLISHER_2, test_spender.clone());
        all_spenders.insert(*ADVERTISER_2, test_spender.clone());
        all_spenders.insert(*CREATOR, test_spender.clone());

        let first_page_response = AllSpendersResponse {
            spenders: vec![
                (
                    *ADVERTISER,
                    all_spenders.get(&ADVERTISER).unwrap().to_owned(),
                ),
                (
                    *PUBLISHER,
                    all_spenders.get(&*PUBLISHER).unwrap().to_owned(),
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
                    *PUBLISHER_2,
                    all_spenders.get(&PUBLISHER_2).unwrap().to_owned(),
                ),
                (
                    *ADVERTISER_2,
                    all_spenders.get(&ADVERTISER_2).unwrap().to_owned(),
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
            spenders: vec![(*CREATOR, all_spenders.get(&*CREATOR).unwrap().to_owned())]
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

        let sentry_url = ApiUrl::from_str(&server.uri()).expect("Should parse");

        let mut validators = Validators::new();
        validators.insert(
            DUMMY_VALIDATOR_LEADER.id,
            Validator {
                url: sentry_url.clone(),
                token: AuthToken::default(),
            },
        );
        let mut config = configuration(Environment::Development, None).expect("Should get Config");
        config.spendable_find_limit = 2;

        let leader_adapter = Adapter::with_unlocked(Dummy::init(Options {
            dummy_identity: IDS[&LEADER],
            dummy_auth_tokens: DUMMY_AUTH.clone(),
            dummy_chains: config.chains.values().cloned().collect(),
        }));
        let logger = discard_logger();

        let channel_context = config
            .find_chain_of(DUMMY_CAMPAIGN.channel.token)
            .expect("Should find Dummy campaign token in config")
            .with_channel(DUMMY_CAMPAIGN.channel);

        let sentry = SentryApi::new(leader_adapter, logger, config, sentry_url)
            .expect("Should build sentry");

        let mut res = sentry
            .get_all_spenders(&channel_context)
            .await
            .expect("should get response");

        // Checks for page 1
        let res_advertiser = res.remove(&ADVERTISER);
        let res_publisher = res.remove(&*PUBLISHER);
        assert_eq!(res_advertiser.expect("Should have value"), test_spender);
        assert_eq!(res_publisher.expect("Should have value"), test_spender);

        // Checks for page 2
        let res_publisher2 = res.remove(&PUBLISHER_2);
        let res_advertiser_2 = res.remove(&ADVERTISER_2);
        assert_eq!(res_publisher2.expect("Should have value"), test_spender);
        assert_eq!(res_advertiser_2.expect("Should have value"), test_spender);

        // Checks for page 3
        let res_creator = res.remove(&*CREATOR);
        assert_eq!(res_creator.expect("Should have value"), test_spender);

        // There should be no remaining elements
        assert_eq!(res.len(), 0)
    }

    #[tokio::test]
    async fn test_collecting_and_channels_and_campaigns() {
        let server = MockServer::start().await;
        let chain_1_token = GANACHE_INFO_1.tokens["Mocked TOKEN 1"].address;

        // Setting up new leader and a channel and campaign which use it on Ganache #1337
        let dummy_leader_2 = ValidatorDesc {
            id: IDS[&LEADER_2],
            url: "http://tom.adex.network".to_string(),
            fee: 200.into(),
            fee_addr: None,
        };
        let channel_new_leader = Channel {
            leader: IDS[&*LEADER_2],
            follower: IDS[&*FOLLOWER],
            guardian: *GUARDIAN,
            token: DUMMY_CAMPAIGN.channel.token,
            nonce: DUMMY_CAMPAIGN.channel.nonce,
        };
        let mut campaign_new_leader = DUMMY_CAMPAIGN.clone();
        campaign_new_leader.id = CampaignId::new();
        campaign_new_leader.channel = channel_new_leader;
        campaign_new_leader.validators =
            CampaignValidators::new((dummy_leader_2.clone(), DUMMY_VALIDATOR_FOLLOWER.clone()));

        // Setting up new follower and a channel and campaign which use it on Ganache #1
        let dummy_follower_2 = ValidatorDesc {
            id: IDS[&GUARDIAN],
            url: "http://jerry.adex.network".to_string(),
            fee: 300.into(),
            fee_addr: None,
        };
        let channel_new_follower = Channel {
            leader: IDS[&*LEADER],
            follower: IDS[&*GUARDIAN],
            guardian: *GUARDIAN,
            token: chain_1_token,
            nonce: DUMMY_CAMPAIGN.channel.nonce,
        };
        let mut campaign_new_follower = DUMMY_CAMPAIGN.clone();
        campaign_new_follower.id = CampaignId::new();
        campaign_new_follower.channel = channel_new_follower;
        campaign_new_follower.validators =
            CampaignValidators::new((DUMMY_VALIDATOR_LEADER.clone(), dummy_follower_2.clone()));

        // Setting up a channel and campaign which use the new leader and follower on Ganache #1
        let channel_new_leader_and_follower = Channel {
            leader: IDS[&*LEADER_2],
            follower: IDS[&*GUARDIAN],
            guardian: *GUARDIAN,
            token: chain_1_token,
            nonce: DUMMY_CAMPAIGN.channel.nonce,
        };
        let mut campaign_new_leader_and_follower = DUMMY_CAMPAIGN.clone();
        campaign_new_leader_and_follower.id = CampaignId::new();
        campaign_new_leader_and_follower.channel = channel_new_leader_and_follower;
        campaign_new_leader_and_follower.validators =
            CampaignValidators::new((dummy_leader_2.clone(), dummy_follower_2.clone()));

        let sentry_url = ApiUrl::from_str(&server.uri()).expect("Should parse");

        let mut config = GANACHE_CONFIG.clone();
        config.campaigns_find_limit = 2;

        // Initializing SentryApi instance
        let leader_sentry = setup_dummy_sentry(IDS[&LEADER], config.clone(), sentry_url.clone());

        // Getting Wiremock to return the campaigns when called
        let first_page_response = CampaignListResponse {
            campaigns: vec![DUMMY_CAMPAIGN.clone(), campaign_new_leader.clone()],
            pagination: Pagination {
                page: 0,
                total_pages: 2,
            },
        };

        let second_page_response = CampaignListResponse {
            campaigns: vec![
                campaign_new_follower.clone(),
                campaign_new_leader_and_follower.clone(),
            ],
            pagination: Pagination {
                page: 1,
                total_pages: 2,
            },
        };

        Mock::given(method("GET"))
            .and(path("/v5/campaign/list"))
            .and(query_param("page", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&first_page_response))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v5/campaign/list"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&second_page_response))
            .mount(&server)
            .await;

        // Testing collect_channels()
        {
            let mut expected_channels: HashSet<ChainOf<Channel>> = HashSet::new();
            expected_channels.insert(
                config
                    .find_chain_of(DUMMY_CAMPAIGN.channel.token)
                    .expect("Should find channel token in config")
                    .with_channel(DUMMY_CAMPAIGN.channel),
            );
            expected_channels.insert(
                config
                    .find_chain_of(channel_new_leader.token)
                    .expect("Should find channel token in config")
                    .with_channel(channel_new_leader),
            );
            expected_channels.insert(
                config
                    .find_chain_of(channel_new_follower.token)
                    .expect("Should find channel token in config")
                    .with_channel(channel_new_follower),
            );
            expected_channels.insert(
                config
                    .find_chain_of(channel_new_leader_and_follower.token)
                    .expect("Should find channel token in config")
                    .with_channel(channel_new_leader_and_follower),
            );

            let (channels, chains_validators) = leader_sentry
                .collect_channels()
                .await
                .expect("Should collect channels");
            assert_eq!(channels, expected_channels, "Correct channels are returned");

            let chains_validators_1337 = chains_validators
                .get(&ChainId::from(1337))
                .expect("There should be validators for #1337 chain");
            assert!(
                chains_validators_1337.contains_key(&DUMMY_VALIDATOR_LEADER.id),
                "Dummy leader is included"
            );
            assert!(
                chains_validators_1337.contains_key(&DUMMY_VALIDATOR_FOLLOWER.id),
                "Dummy follower is included"
            );
            assert!(
                chains_validators_1337.contains_key(&dummy_leader_2.id),
                "Dummy leader 2 is included"
            );
            assert_eq!(
                chains_validators_1337.keys().len(),
                3,
                "There are no extra validators returned"
            );

            let chains_validators_1 = chains_validators
                .get(&ChainId::from(1))
                .expect("There should be validators for #1 chain");
            assert!(
                chains_validators_1.contains_key(&DUMMY_VALIDATOR_LEADER.id),
                "Dummy leader is returned"
            );
            assert!(
                chains_validators_1.contains_key(&dummy_follower_2.id),
                "Dummy follower 2 is returned"
            );
            assert!(
                chains_validators_1.contains_key(&dummy_leader_2.id),
                "Dummy leader 2 is returned"
            );
            assert_eq!(
                chains_validators_1.keys().len(),
                3,
                "There are no extra validators returned"
            );
        }
        // Calls all_campaigns() to see if all campaigns are returned
        // We test for query parameters in campaign_list() tests
        {
            let all_campaigns = vec![
                DUMMY_CAMPAIGN.clone(),
                campaign_new_leader.clone(),
                campaign_new_follower.clone(),
                campaign_new_leader_and_follower.clone(),
            ];
            let res =
                campaigns::all_campaigns(leader_sentry.client.clone(), &sentry_url.clone(), None)
                    .await
                    .expect("Should get all campaigns");
            assert_eq!(res, all_campaigns, "All campaigns are present");
        }
        // test all_channels
        {
            // Get Wiremock to return the channels
            let first_page_response = ChannelListResponse {
                channels: vec![DUMMY_CAMPAIGN.channel, channel_new_leader],
                pagination: Pagination {
                    page: 0,
                    total_pages: 2,
                },
            };

            let second_page_response = ChannelListResponse {
                channels: vec![channel_new_follower, channel_new_leader_and_follower],
                pagination: Pagination {
                    page: 1,
                    total_pages: 2,
                },
            };

            Mock::given(method("GET"))
                .and(path("/v5/channel/list"))
                .and(query_param("page", "0"))
                .respond_with(ResponseTemplate::new(200).set_body_json(&first_page_response))
                .mount(&server)
                .await;

            Mock::given(method("GET"))
                .and(path("/v5/channel/list"))
                .and(query_param("page", "1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(&second_page_response))
                .mount(&server)
                .await;
            let all_channels = vec![
                DUMMY_CAMPAIGN.channel,
                channel_new_leader,
                channel_new_follower,
                channel_new_leader_and_follower,
            ];
            let res = channels::all_channels(
                leader_sentry.client.clone(),
                &sentry_url.clone(),
                DUMMY_VALIDATOR_LEADER.id,
            )
            .await
            .expect("Should get channels");
            assert_eq!(all_channels, res, "All channels are present");
        }
    }
}
