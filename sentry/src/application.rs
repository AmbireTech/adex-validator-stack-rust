use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use hyper::{
    service::{make_service_fn, service_fn},
    Error, Server,
};
use once_cell::sync::Lazy;
use primitives::{adapter::Adapter, config::Environment};
use redis::ConnectionInfo;
use serde::{Deserialize, Deserializer};
use slog::{error, info};

/// an error used when deserializing a [`Config`] instance from environment variables
/// see [`Config::from_env()`]
pub use envy::Error as EnvError;

use crate::Application;

pub const DEFAULT_PORT: u16 = 8005;
pub const DEFAULT_IP_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
pub static DEFAULT_REDIS_URL: Lazy<ConnectionInfo> = Lazy::new(|| {
    "redis://127.0.0.1:6379"
        .parse::<ConnectionInfo>()
        .expect("Valid URL")
});
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// Defaults to `Development`: [`Environment::default()`]
    pub env: Environment,
    /// The port on which the Sentry REST API will be accessible.
    #[serde(default = "default_port")]
    /// Defaults to `8005`: [`DEFAULT_PORT`]
    pub port: u16,
    /// The address on which the Sentry REST API will be accessible.
    /// `0.0.0.0` can be used for Docker.
    /// `127.0.0.1` can be used for locally running servers.
    #[serde(default = "default_ip_addr")]
    /// Defaults to `0.0.0.0`: [`DEFAULT_IP_ADDR`]
    pub ip_addr: IpAddr,
    #[serde(deserialize_with = "redis_url", default = "default_redis_url")]
    /// Defaults to locally running Redis server: [`DEFAULT_REDIS_URL`]
    pub redis_url: ConnectionInfo,
}

impl Config {
    /// Deserialize the application [`Config`] from Environment variables.
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

impl<A: Adapter + 'static> Application<A> {
    /// Starts the `hyper` `Server`.
    pub async fn run(self, socket_addr: SocketAddr) {
        let logger = self.logger.clone();
        info!(&logger, "Listening on socket address: {}!", socket_addr);

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
