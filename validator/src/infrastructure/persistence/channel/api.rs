use std::iter::once;
use std::sync::Arc;

use futures::compat::Future01CompatExt;
use futures::future::{ok, try_join_all};
use futures::{FutureExt, TryFutureExt};
use futures_legacy::Future as LegacyFuture;
use reqwest::r#async::{Client, Response};
use serde::Deserialize;

use domain::{Channel, RepositoryFuture};

use crate::domain::channel::ChannelRepository;
use crate::infrastructure::persistence::api::ApiPersistenceError;
use crate::infrastructure::sentry::SentryApi;

#[derive(Clone)]
// @TODO: make pub(crate)
pub struct ApiChannelRepository {
    pub sentry: SentryApi,
}

impl ChannelRepository for ApiChannelRepository {
    fn all(&self, identity: &str) -> RepositoryFuture<Vec<Channel>> {
        self.sentry
            .clone()
            .all_channels(Some(identity.to_string()))
            .map_err(|_error| ApiPersistenceError::Reading.into())
            .boxed()
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ChannelAllResponse {
    pub channels: Vec<Channel>,
    pub total_pages: u64,
}
