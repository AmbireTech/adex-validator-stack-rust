use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
    sync::Arc,
};

use adapter::client::Locked;
use axum::{
    extract::{FromRequest, RequestParts},
    http::StatusCode,
    middleware, Extension, Router, routing::get,
};
use hyper::{
    service::{make_service_fn, service_fn},
    Error, Server,
};
use once_cell::sync::Lazy;
use primitives::{config::Environment, ValidatorId};
use redis::ConnectionInfo;
use serde::{Deserialize, Deserializer};
use simple_hyper_server_tls::{listener_from_pem_files, Protocols, TlsListener};
use slog::{error, info};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

use crate::{
    db::{CampaignRemaining, DbPool},
    middleware::{
        auth::{authenticate, Authenticate},
        cors::{cors, Cors},
        Middleware,
    },
    platform::PlatformApi,
    response::{map_response_error, ResponseError},
    routes::{
        get_cfg,
        routers::{
            analytics_router, campaigns_router, campaigns_router_axum, channels_router,
            channels_router_axum,
        }, get_cfg_axum,
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

    pub async fn axum_routing(&self) -> Router {
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

        let channels = channels_router_axum::<C>();

        let campaigns = campaigns_router_axum::<C>();

        let router = Router::new()
            .nest("/channel", channels)
            .nest("/campaign", campaigns);

        Router::new()
            .nest("/v5", router)
            .route("/cfg", get(get_cfg_axum::<C>))
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
    /// Starts the `hyper` `Server`.
    pub async fn run2(self, enable_tls: EnableTls) {
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

                let server = Server::bind(&socket_addr)
                    .serve(make_service)
                    .with_graceful_shutdown(shutdown_signal(logger.clone()));

                if let Err(e) = server.await {
                    error!(&logger, "server error: {}", e; "main" => "run");
                }
            }
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
                let server = Server::builder(listener)
                    .serve(make_service)
                    .with_graceful_shutdown(shutdown_signal(logger.clone()));
                tokio::pin!(server);

                while let Err(e) = (&mut server).await {
                    // This is usually caused by trying to connect on HTTP instead of HTTPS
                    error!(&logger, "server error: {}", e; "main" => "run");
                }
            }
        }
    }

    pub async fn run(self, enable_tls: EnableTls) {
        let logger = self.logger.clone();
        let socket_addr = match &enable_tls {
            EnableTls::NoTls(socket_addr) => socket_addr,
            EnableTls::Tls { socket_addr, .. } => socket_addr,
        };

        info!(&logger, "Listening on socket address: {}!", socket_addr);

        let app = self.axum_routing().await;

        let server = axum::Server::bind(socket_addr)
            .serve(app.into_make_service())
            .with_graceful_shutdown(shutdown_signal(logger.clone()));

        tokio::pin!(server);

        while let Err(e) = (&mut server).await {
            // This is usually caused by trying to connect on HTTP instead of HTTPS
            error!(&logger, "server error: {}", e; "main" => "run");
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
    },
}

impl EnableTls {
    pub fn new_tls<C: AsRef<Path>, K: AsRef<Path>>(
        certificates: C,
        private_keys: K,
        socket_addr: SocketAddr,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let listener =
            listener_from_pem_files(certificates, private_keys, Protocols::ALL, &socket_addr)?;

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
async fn shutdown_signal(logger: Logger) {
    // Wait for the Ctrl+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");

    info!(&logger, "Received Ctrl+C signal. Shutting down..")
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
