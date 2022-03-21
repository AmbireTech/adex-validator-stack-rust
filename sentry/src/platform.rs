// previously fetched from the market (in the supermarket) it should now be fetched from the Platform!
use primitives::{
    market::{AdSlotResponse, AdUnitResponse, AdUnitsResponse, Campaign, StatusType},
    util::ApiUrl,
    AdUnit, IPFS,
};
use reqwest::{Client, Error, StatusCode};
use slog::{info, Logger};
use std::{fmt, sync::Arc, time::Duration};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
/// The `PlatformApi` is cheap to clone as it already wraps the real client `PlatformApiInner` in an `Arc`
pub struct PlatformApi {
    inner: Arc<PlatformApiInner>,
}

impl PlatformApi {
    /// The Market url that was is used for communication with the API
    pub fn url(&self) -> &ApiUrl {
        &self.inner.platform_url
    }

    // todo: Instead of associate function, use a builder
    pub fn new(
        platform_url: ApiUrl,
        keep_alive_interval: Duration,
        logger: Logger,
    ) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(PlatformApiInner::new(
                platform_url,
                keep_alive_interval,
                logger,
            )?),
        })
    }

    pub async fn fetch_unit(&self, ipfs: IPFS) -> Result<Option<AdUnitResponse>> {
        self.inner.fetch_unit(ipfs).await
    }

    pub async fn fetch_units(&self, ad_type: &str) -> Result<Vec<AdUnit>> {
        self.inner.fetch_units(ad_type).await
    }
}

/// Should we query All or only certain statuses
#[derive(Debug)]
pub enum Statuses<'a> {
    All,
    Only(&'a [StatusType]),
}

impl fmt::Display for Statuses<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Statuses::*;

        match self {
            All => write!(f, "all"),
            Only(statuses) => {
                let statuses = statuses.iter().map(ToString::to_string).collect::<Vec<_>>();

                write!(f, "status={}", statuses.join(","))
            }
        }
    }
}

#[derive(Debug, Clone)]
struct PlatformApiInner {
    platform_url: ApiUrl,
    client: Client,
    logger: Logger,
}

impl PlatformApiInner {
    /// The limit of Campaigns per page when fetching
    /// Limit the value to MAX(500)
    const MARKET_CAMPAIGNS_LIMIT: u64 = 500;
    /// The limit of AdUnits per page when fetching
    /// It should always be > 1
    const MARKET_AD_UNITS_LIMIT: u64 = 1_000;

    /// Duration specified will be the time to remain idle before sending a TCP keepalive probe.
    /// Sets [`reqwest::Client`]'s [`reqwest::ClientBuilder::tcp_keepalive`](reqwest::ClientBuilder::tcp_keepalive))
    // @TODO: maybe add timeout too?
    pub fn new(
        platform_url: ApiUrl,
        keep_alive_interval: Duration,
        logger: Logger,
    ) -> Result<Self> {
        let client = Client::builder()
            .tcp_keepalive(keep_alive_interval)
            .cookie_store(true)
            .build()?;

        Ok(Self {
            platform_url,
            client,
            logger,
        })
    }

    pub async fn fetch_unit(&self, ipfs: IPFS) -> Result<Option<AdUnitResponse>> {
        let url = self
            .platform_url
            .join(&format!("units/{}", ipfs))
            .expect("Wrong Platform Url for /units/{IPFS} endpoint");

        match self.client.get(url).send().await?.error_for_status() {
            Ok(response) => {
                let ad_unit_response = response.json::<AdUnitResponse>().await?;

                Ok(Some(ad_unit_response))
            }
            // if we have a `404 Not Found` error, return None
            Err(err) if err.status() == Some(StatusCode::NOT_FOUND) => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub async fn fetch_units(&self, ad_type: &str) -> Result<Vec<AdUnit>> {
        let mut units = Vec::new();
        let mut skip: u64 = 0;
        let limit = Self::MARKET_AD_UNITS_LIMIT;

        loop {
            // if one page fail, simply return the error for now
            let mut page_results = self.fetch_units_page(ad_type, skip).await?;
            // get the count before appending the page results to all
            let count = page_results.len() as u64;

            // append all received units
            units.append(&mut page_results);
            // add the number of results we need to skip in the next iteration
            skip += count;

            // if the Market returns < market fetch limit
            // we've got all AdSlots from all pages!
            if count < limit {
                // so break out of the loop
                break;
            }
        }

        Ok(units)
    }

    /// `skip` - how many records it should skip (pagination)
    async fn fetch_units_page(&self, ad_type: &str, skip: u64) -> Result<Vec<AdUnit>> {
        let url = self
            .platform_url
            .join(&format!(
                "units?limit={}&skip={}&type={}",
                Self::MARKET_AD_UNITS_LIMIT,
                skip,
                ad_type,
            ))
            .expect("Wrong Market Url for /units endpoint");

        let response = self.client.get(url).send().await?;

        let ad_units: AdUnitsResponse = response.json().await?;

        Ok(ad_units.0)
    }
}
