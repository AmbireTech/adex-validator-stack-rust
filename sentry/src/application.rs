use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    sync::Arc,
};

use axum::{
    extract::{FromRequest, RequestParts},
    http::{Method, StatusCode},
    middleware,
    routing::get,
    Extension, Router,
};
use axum_server::{tls_rustls::RustlsConfig, Handle};
use once_cell::sync::Lazy;
use redis::{aio::MultiplexedConnection, ConnectionInfo};
use serde::{Deserialize, Deserializer};
use slog::{error, info, Logger};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

use adapter::{client::Locked, Adapter};
use primitives::{config::Environment, ValidatorId};

use crate::{
    db::{CampaignRemaining, DbPool},
    middleware::auth::authenticate,
    platform::PlatformApi,
    routes::{
        get_cfg,
        routers::{analytics_router, campaigns_router, channels_router, units_for_slot_router},
    },
};

/// an error used when deserializing a [`EnvConfig`] instance from environment variables
/// see [`EnvConfig::from_env()`]
pub use envy::Error as EnvError;

pub const DEFAULT_PORT: u16 = 8005;
pub const DEFAULT_IP_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
pub static DEFAULT_REDIS_URL: Lazy<ConnectionInfo> = Lazy::new(|| {
    "redis://127.0.0.1:6379"
        .parse::<ConnectionInfo>()
        .expect("Valid URL")
});
/// Sentry Application config set by environment variables
#[derive(Debug, Deserialize, Clone)]
pub struct EnvConfig {
    /// Defaults to `Development`: [`Environment::default()`]
    #[serde(default)]
    pub env: Environment,
    /// The port on which the Sentry REST API will be accessible.
    ///
    /// Defaults to `8005`: [`DEFAULT_PORT`]
    #[serde(default = "default_port")]
    pub port: u16,
    /// The address on which the Sentry REST API will be accessible.
    /// `0.0.0.0` can be used for Docker.
    /// `127.0.0.1` can be used for locally running servers.
    ///
    /// Defaults to `0.0.0.0`: [`DEFAULT_IP_ADDR`]
    #[serde(default = "default_ip_addr")]
    pub ip_addr: IpAddr,
    /// Defaults to locally running Redis server: [`DEFAULT_REDIS_URL`]
    #[serde(deserialize_with = "redis_url", default = "default_redis_url")]
    pub redis_url: ConnectionInfo,
    /// Whether or not to seed the database in [`Environment::Development`].
    #[serde(default)]
    pub seed_db: bool,
}

impl EnvConfig {
    /// Deserialize the application [`EnvConfig`] from Environment variables.
    pub fn from_env() -> Result<Self, EnvError> {
        envy::from_env()
    }
}

fn redis_url<'a, 'de: 'a, D>(deserializer: D) -> Result<ConnectionInfo, D::Error>
where
    D: Deserializer<'de>,
{
    let url_string = String::deserialize(deserializer)?;

    url_string.parse().map_err(serde::de::Error::custom)
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn default_ip_addr() -> IpAddr {
    DEFAULT_IP_ADDR
}

fn default_redis_url() -> ConnectionInfo {
    DEFAULT_REDIS_URL.clone()
}

/// The Sentry REST web application
pub struct Application<C: Locked + 'static> {
    /// For sentry to work properly, we need an [`adapter::Adapter`] in a [`adapter::LockedState`] state.
    pub adapter: Adapter<C>,
    pub config: primitives::Config,
    pub logger: Logger,
    pub redis: MultiplexedConnection,
    pub pool: DbPool,
    pub campaign_remaining: CampaignRemaining,
    pub platform_api: PlatformApi,
}

impl<C> Application<C>
where
    C: Locked,
{
    pub fn new(
        adapter: Adapter<C>,
        config: primitives::Config,
        logger: Logger,
        redis: MultiplexedConnection,
        pool: DbPool,
        campaign_remaining: CampaignRemaining,
        platform_api: PlatformApi,
    ) -> Self {
        Self {
            adapter,
            config,
            logger,
            redis,
            pool,
            campaign_remaining,
            platform_api,
        }
    }

    pub async fn routing(&self) -> Router {
        let cors = CorsLayer::new()
            // "GET,HEAD,PUT,PATCH,POST,DELETE"
            .allow_methods([
                Method::GET,
                Method::HEAD,
                Method::PUT,
                Method::PATCH,
                Method::POST,
                Method::DELETE,
            ])
            // allow requests from any origin
            // "*"
            .allow_origin(tower_http::cors::Any);

        let router = Router::new()
            .nest("/channel", channels_router::<C>())
            .nest("/campaign", campaigns_router::<C>())
            .nest("/analytics", analytics_router::<C>())
            .nest("/units-for-slot", units_for_slot_router::<C>());

        Router::new()
            .nest("/v5", router)
            .route("/cfg", get(get_cfg::<C>))
            .layer(
                // keeps the order from top to bottom!
                ServiceBuilder::new()
                    .layer(cors)
                    .layer(middleware::from_fn(authenticate::<C, _>)),
            )
            .layer(Extension(Arc::new(self.clone())))
    }
}

impl<C: Locked + 'static> Application<C> {
    pub async fn run(self, enable_tls: EnableTls) {
        let logger = self.logger.clone();
        let socket_addr = match &enable_tls {
            EnableTls::NoTls(socket_addr) => socket_addr,
            EnableTls::Tls { socket_addr, .. } => socket_addr,
        };

        info!(&logger, "Listening on socket address: {}!", socket_addr);
        let router = self.routing().await;

        let handle = Handle::new();

        // Spawn a task to shutdown server.
        tokio::spawn(shutdown_signal(logger.clone(), handle.clone()));

        match enable_tls {
            EnableTls::NoTls(socket_addr) => {
                let server = axum_server::bind(socket_addr)
                    .handle(handle)
                    .serve(router.into_make_service());

                tokio::pin!(server);

                while let Err(e) = (&mut server).await {
                    // This is usually caused by trying to connect on HTTP instead of HTTPS
                    error!(&logger, "server error: {}", e; "main" => "run");
                }
            }

            EnableTls::Tls {
                config,
                socket_addr,
            } => {
                let server = axum_server::bind_rustls(socket_addr, config)
                    .handle(handle)
                    .serve(router.into_make_service());

                tokio::pin!(server);

                while let Err(e) = (&mut server).await {
                    // This is usually caused by trying to connect on HTTP instead of HTTPS
                    error!(&logger, "server error: {}", e; "main" => "run");
                }
            }
        }
    }
}

impl<C: Locked> Clone for Application<C> {
    fn clone(&self) -> Self {
        Self {
            adapter: self.adapter.clone(),
            config: self.config.clone(),
            logger: self.logger.clone(),
            redis: self.redis.clone(),
            pool: self.pool.clone(),
            campaign_remaining: self.campaign_remaining.clone(),
            platform_api: self.platform_api.clone(),
        }
    }
}

/// Either enable or do not the Tls support.
pub enum EnableTls {
    NoTls(SocketAddr),
    Tls {
        socket_addr: SocketAddr,
        config: RustlsConfig,
    },
}

impl EnableTls {
    pub async fn new_tls<C: AsRef<Path>, K: AsRef<Path>>(
        certificates: C,
        private_keys: K,
        socket_addr: SocketAddr,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let config = RustlsConfig::from_pem_file(certificates, private_keys).await?;

        Ok(Self::Tls {
            config,
            socket_addr,
        })
    }

    pub fn no_tls(socket_addr: SocketAddr) -> Self {
        Self::NoTls(socket_addr)
    }
}

/// Sentry [`Application`] Session
#[derive(Debug, Clone)]
pub struct Session {
    pub ip: Option<String>,
    pub country: Option<String>,
    pub referrer_header: Option<String>,
    pub os: Option<String>,
}

/// Validated Authentication for the Sentry [`Application`].
#[derive(Debug, Clone)]
pub struct Auth {
    pub era: i64,
    pub uid: ValidatorId,
    /// The Chain for which this authentication was validated
    pub chain: primitives::Chain,
}

/// A query string deserialized using `serde_qs` instead of axum's `serde_urlencoded`
pub struct Qs<T>(pub T);

#[axum::async_trait]
impl<T, B> FromRequest<B> for Qs<T>
where
    T: serde::de::DeserializeOwned,
    B: Send,
{
    type Rejection = (StatusCode, String);

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let query = req.uri().query().unwrap_or_default();

        match serde_qs::from_str(query) {
            Ok(query) => Ok(Self(query)),
            Err(err) => Err((StatusCode::BAD_REQUEST, err.to_string())),
        }
    }
}

/// A Ctrl+C signal to gracefully shutdown the server
async fn shutdown_signal(logger: Logger, handle: Handle) {
    // Wait for the Ctrl+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");

    // Signal the server to shutdown using Handle.
    handle.shutdown();

    info!(&logger, "Received Ctrl+C signal. Shutting down..")
}

pub mod seed {
    use std::sync::Arc;

    use axum::{Extension, Json};

    use adapter::{
        ethereum::{test_util::{Erc20Token, Outpace}, ChainTransport},
        Dummy, Ethereum,
    };
    use primitives::{
        sentry::campaign_create::CreateCampaign,
        spender::Spendable,
        test_util::{ADVERTISER, ADVERTISER_2, CAMPAIGNS, LEADER, FOLLOWER},
        unified_num::FromWhole,
        BigNum, Campaign, ChainOf, Deposit, UnifiedNum, ValidatorId,
    };

    use crate::{
        db::{insert_channel, spendable::insert_spendable},
        routes::{
            campaign::create_campaign,
            channel::{channel_dummy_deposit, ChannelDummyDeposit},
        },
        Application, Auth,
    };

    pub async fn seed_dummy(app: Application<Dummy>) -> Result<(), Box<dyn std::error::Error>> {
        // create campaign
        // Chain 1337
        let campaign_1 = CAMPAIGNS[0].clone();
        // Chain 1337
        let campaign_2 = CAMPAIGNS[1].clone();
        // Chain 1
        let campaign_3 = CAMPAIGNS[2].clone();

        async fn create_seed_campaign(
            app: Application<Dummy>,
            campaign: &ChainOf<Campaign>,
        ) -> Result<(), Box<dyn std::error::Error>> {
            let campaign_to_create = CreateCampaign::from_campaign(campaign.context.clone());
            let auth = Auth {
                era: 0,
                uid: ValidatorId::from(campaign_to_create.creator),
                chain: campaign.chain.clone(),
            };
            create_campaign(
                Json(campaign_to_create),
                Extension(auth),
                Extension(Arc::new(app)),
            )
            .await
            .expect("Should create seed campaigns");

            Ok(())
        }

        async fn dummy_deposit(
            app: Application<Dummy>,
            campaign: &ChainOf<Campaign>,
        ) -> Result<(), Box<dyn std::error::Error>> {
            let channel = campaign.context.channel;
            let auth = Auth {
                era: 0,
                uid: ValidatorId::from(campaign.context.creator),
                chain: campaign.chain.clone(),
            };

            let request = ChannelDummyDeposit {
                channel,
                deposit: Deposit {
                    total: UnifiedNum::from_whole(1_000_000),
                },
            };

            let result =
                channel_dummy_deposit(Extension(Arc::new(app)), Extension(auth), Json(request))
                    .await;

            assert!(result.is_ok());

            Ok(())
        }
        // chain 1337
        dummy_deposit(app.clone(), &campaign_1).await?;
        // chain 1337
        dummy_deposit(app.clone(), &campaign_2).await?;
        // chain 1
        dummy_deposit(app.clone(), &campaign_3).await?;

        create_seed_campaign(app.clone(), &campaign_1).await?;
        create_seed_campaign(app.clone(), &campaign_2).await?;
        create_seed_campaign(app.clone(), &campaign_3).await?;
        Ok(())
    }

    pub async fn seed_ethereum(
        app: Application<Ethereum>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Chain 1337
        let campaign_1 = CAMPAIGNS[0].clone();
        // Chain 1337
        let campaign_2 = CAMPAIGNS[1].clone();
        // Chain 1
        let campaign_3 = CAMPAIGNS[2].clone();

        let web3_chain_1337 = campaign_1.chain.init_web3()?;
        let token_1337 = Erc20Token::new(&web3_chain_1337, campaign_1.token.clone());
        let outpace_1337 = Outpace::new(&web3_chain_1337, campaign_1.chain.outpace);
        let web3_chain_1 = campaign_3.chain.init_web3()?;
        let token_1 = Erc20Token::new(&web3_chain_1, campaign_3.token.clone());
        let outpace_1 = Outpace::new(&web3_chain_1, campaign_1.chain.outpace);

        token_1337
            .set_balance(LEADER.to_bytes(), ADVERTISER.to_bytes(), &BigNum::with_precision(3_000_000, token_1337.info.precision.into()))
            .await
            .expect("Failed to set balance");
        outpace_1337
            .deposit(&campaign_1.context.channel, ADVERTISER.to_bytes(), &BigNum::with_precision(1_000_000, token_1337.info.precision.into()))
            .await
            .expect("Should deposit funds");
        outpace_1337
            .deposit(&campaign_2.context.channel, ADVERTISER.to_bytes(), &BigNum::with_precision(1_000_000, token_1337.info.precision.into()))
            .await
            .expect("Should deposit funds");

        token_1
            .set_balance(LEADER.to_bytes(), ADVERTISER_2.to_bytes(), &BigNum::with_precision(2_000_000, token_1.info.precision.into()))
            .await
            .expect("Failed to set balance");

        outpace_1
            .deposit(&campaign_3.context.channel, ADVERTISER_2.to_bytes(), &BigNum::with_precision(1_000_000, token_1.info.precision.into()))
            .await
            .expect("Should deposit funds");

        async fn create_seed_campaign(
            app: Application<Ethereum>,
            campaign: &ChainOf<Campaign>,
        ) -> Result<(), Box<dyn std::error::Error>> {
            let channel_context = ChainOf::of_channel(campaign);
            let campaign_to_create = CreateCampaign::from_campaign(campaign.context.clone());

            let auth = Auth {
                era: 0,
                uid: ValidatorId::from(campaign.context.creator),
                chain: campaign.chain.clone(),
            };

            let spendable = Spendable {
                spender: campaign.context.creator,
                channel: campaign.context.channel,
                deposit: Deposit {
                    total: UnifiedNum::from_whole(10_000),
                },
            };
            insert_channel(&app.pool, &channel_context)
                .await
                .expect("Should insert channel of seed campaign");
            insert_spendable(app.pool.clone(), &spendable)
                .await
                .expect("Should insert spendable for campaign creator");

            create_campaign(
                Json(campaign_to_create),
                Extension(auth),
                Extension(Arc::new(app.clone())),
            )
            .await
            .expect("should create campaign");

            Ok(())
        }

        create_seed_campaign(app.clone(), &campaign_1).await?;
        create_seed_campaign(app.clone(), &campaign_2).await?;
        create_seed_campaign(app.clone(), &campaign_3).await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use super::*;

    #[test]
    fn environment() {
        let development = serde_json::from_value::<Environment>(json!("development"))
            .expect("Should deserialize");
        let production =
            serde_json::from_value::<Environment>(json!("production")).expect("Should deserialize");

        assert_eq!(Environment::Development, development);
        assert_eq!(Environment::Production, production);
    }
}
