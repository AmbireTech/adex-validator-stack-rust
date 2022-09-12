use std::time::Duration;

use reqwest::{Client, Error, StatusCode};

// previously fetched from the market (in the supermarket) it should now be fetched from the Platform!
use primitives::{platform::AdSlotResponse, util::ApiUrl, IPFS};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
/// The `PlatformApi` is cheap to clone
pub struct PlatformApi {
    platform_url: ApiUrl,
    client: Client,
}

impl PlatformApi {
    /// The Platform url that was is used for communication with the API
    pub fn url(&self) -> &ApiUrl {
        &self.platform_url
    }

    /// Duration specified will be the time to remain idle before sending a TCP keepalive probe.
    /// Sets [`reqwest::Client`]'s [`reqwest::ClientBuilder::tcp_keepalive`](reqwest::ClientBuilder::tcp_keepalive))
    // @TODO: maybe add timeout too?
    pub fn new(platform_url: ApiUrl, keep_alive_interval: Duration) -> Result<Self> {
        let client = Client::builder()
            .tcp_keepalive(keep_alive_interval)
            .cookie_store(true)
            .build()?;

        Ok(Self {
            platform_url,
            client,
        })
    }

    /// Fetch the [`AdSlot`], [`AdSlot.fallback_unit`], [`AdSlot.website`] information and the `AdUnit`s
    /// of the AdSlot type ( [`AdSlot.ad_type`] ).
    ///
    /// [`AdSlot`]: primitives::AdSlot
    /// [`AdSlot.fallback_unit`]: primitives::AdSlot::fallback_unit
    /// [`AdSlot.website`]: primitives::AdSlot::website
    /// [`AdSlot.ad_type`]: primitives::AdSlot::ad_type
    pub async fn fetch_slot(&self, ipfs: IPFS) -> Result<Option<AdSlotResponse>> {
        let url = self
            .platform_url
            .join(&format!("slot/{}", ipfs))
            .expect("Wrong Platform Url for /slot/{IPFS} endpoint");

        match self.client.get(url).send().await?.error_for_status() {
            Ok(response) => response.json().await.map(Some),
            // if we have a `404 Not Found` error, return None
            Err(err) if err.status() == Some(StatusCode::NOT_FOUND) => Ok(None),
            Err(err) => Err(err),
        }
    }
}
