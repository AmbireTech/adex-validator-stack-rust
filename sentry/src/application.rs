use std::{net::{IpAddr, Ipv4Addr, SocketAddr}, path::Path};

use adapter::client::Locked;
use hyper::{
    service::{make_service_fn, service_fn},
    Error, Server,
};
use once_cell::sync::Lazy;
use primitives::{config::Environment, ValidatorId};
use redis::ConnectionInfo;
use serde::{Deserialize, Deserializer};
use simple_hyper_server_tls::{listener_from_pem_files, TlsListener, Protocols};
use slog::{error, info};

use crate::{
    db::{CampaignRemaining, DbPool},
    middleware::{
        auth::Authenticate,
        cors::{cors, Cors},
        Middleware,
    },
    platform::PlatformApi,
    response::{map_response_error, ResponseError},
    routes::{
        get_cfg,
        routers::{analytics_router, campaigns_router, channels_router},
    },
};
use adapter::Adapter;
use hyper::{Body, Method, Request, Response};
use redis::aio::MultiplexedConnection;
use slog::Logger;

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
    let url_string = <&'a str>::deserialize(deserializer)?;

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

    pub async fn handle_routing(&self, req: Request<Body>) -> Response<Body> {
        let headers = match cors(&req) {
            Some(Cors::Simple(headers)) => headers,
            // if we have a Preflight, just return the response directly
            Some(Cors::Preflight(response)) => return response,
            None => Default::default(),
        };

        let req = match Authenticate.call(req, self).await {
            Ok(req) => req,
            Err(error) => return map_response_error(error),
        };

        let mut response = match (req.uri().path(), req.method()) {
            ("/cfg", &Method::GET) => get_cfg(req, self).await,
            (route, _) if route.starts_with("/v5/analytics") => analytics_router(req, self).await,
            // This is important because it prevents us from doing
            // expensive regex matching for routes without /channel
            (path, _) if path.starts_with("/v5/channel") => channels_router(req, self).await,
            (path, _) if path.starts_with("/v5/campaign") => campaigns_router(req, self).await,
            _ => Err(ResponseError::NotFound),
        }
        .unwrap_or_else(map_response_error);

        // extend the headers with the initial headers we have from CORS (if there are some)
        response.headers_mut().extend(headers);
        response
    }
}

impl<C: Locked + 'static> Application<C> {
    /// Starts the `hyper` `Server`.
    pub async fn run(self, enable_tls: EnableTls) {
        let logger = self.logger.clone();
        let socket_addr = match &enable_tls {
            EnableTls::NoTls(socket_addr) => socket_addr,
            EnableTls::Tls { socket_addr, .. } => socket_addr,
        };

        info!(&logger, "Listening on socket address: {}!", socket_addr);

        match enable_tls {
            EnableTls::NoTls(socket_addr) => {
                let make_service = make_service_fn(|_| {
                    let server = self.clone();
                    async move {
                        Ok::<_, Error>(service_fn(move |req| {
                            let server = server.clone();
                            async move { Ok::<_, Error>(server.handle_routing(req).await) }
                        }))
                    }
                });

                let server = Server::bind(&socket_addr).serve(make_service);

                if let Err(e) = server.await {
                    error!(&logger, "server error: {}", e; "main" => "run");
                }
            },
            EnableTls::Tls { listener, .. } => {
                let make_service = make_service_fn(|_| {
                    let server = self.clone();
                    async move {
                        Ok::<_, Error>(service_fn(move |req| {
                            let server = server.clone();
                            async move { Ok::<_, Error>(server.handle_routing(req).await) }
                        }))
                    }
                });

                // TODO: Find a way to redirect to HTTPS
                let mut server = Server::builder(listener).serve(make_service);

                while let Err(e) = (&mut server).await {
                    // This is usually caused by trying to connect on HTTP instead of HTTPS
                    error!(&logger, "server error: {}", e; "main" => "run");
                }
            },
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
        listener: TlsListener,
    }
}

impl EnableTls {
    pub fn new_tls<C: AsRef<Path>, K: AsRef<Path>>(certificates: C, private_keys: K, socket_addr: SocketAddr) -> Result<Self, Box<dyn std::error::Error>> {
        let listener = listener_from_pem_files(certificates, private_keys, Protocols::ALL, &socket_addr)?;

        Ok(Self::Tls {
            listener,
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
