use crate::accounting::Client;

use async_trait::async_trait;
use chrono::Utc;
use primitives::{
    channel_v5::Channel,
    sentry::accounting::{Accounting, Balances, CheckedState},
    ChannelId,
};
use std::convert::TryFrom;
use thiserror::Error;
use tokio_postgres::types::Json;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Accounting Balances error: {0}")]
    Balances(#[from] primitives::sentry::accounting::Error),
    #[error("Fetching Accounting from postgres error: {0}")]
    Postgres(#[from] tokio_postgres::Error),
    #[error("Creating or updating a record did not return the expected number of modified rows")]
    NotModified,
}

pub struct Postgres {
    client: deadpool_postgres::Client,
}

impl Postgres {
    pub fn new(client: deadpool_postgres::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Client for Postgres {
    type Error = Error;

    /// ```text
    /// SELECT channel, earners, spenders, created, updated FROM accounting WHERE channel_id = $1
    /// ```
    async fn fetch(
        &self,
        channel: ChannelId,
    ) -> Result<Option<Accounting<CheckedState>>, Self::Error> {
        let statement = self.client.prepare("SELECT channel, earners, spenders, created, updated FROM accounting WHERE channel_id = $1").await?;

        let accounting = self
            .client
            .query_opt(&statement, &[&channel])
            .await?
            .as_ref()
            .map(Accounting::<CheckedState>::try_from)
            .transpose()
            .map_err(Error::Balances)?;

        Ok(accounting)
    }

    /// ```text
    /// INSERT INTO accounting (channel_id, channel, earners, spenders, updated, created) VALUES ($1, $2, $3, $4, $5, $6)
    /// ```
    async fn create(
        &self,
        channel: Channel,
        balances: Balances<CheckedState>,
    ) -> Result<Accounting<CheckedState>, Self::Error> {
        let statement = self.client.prepare("INSERT INTO accounting (channel_id, channel, earners, spenders, updated, created) VALUES ($1, $2, $3, $4, $5, $6)").await?;

        let earners = Json(&balances.earners);
        let spenders = Json(&balances.spenders);
        let updated = None;
        let created = Utc::now();

        let modified_rows = self
            .client
            .execute(
                &statement,
                &[
                    &channel.id(),
                    &channel,
                    &earners,
                    &spenders,
                    &updated,
                    &created,
                ],
            )
            .await?;

        // we expect only a single row to be modified with this query!
        if modified_rows == 1 {
            Ok(Accounting {
                channel,
                balances,
                updated,
                created,
            })
        } else {
            Err(Error::NotModified)
        }
    }

    /// ```text
    /// UPDATE accounting SET earners = $1::jsonb, spenders = $2::jsonb, updated = $3
    /// WHERE channel_id = $4 RETURNING channel, earners, spenders, updated, created
    /// ```
    async fn update(
        &self,
        channel: &Channel,
        new_balances: Balances<CheckedState>,
    ) -> Result<Accounting<CheckedState>, Self::Error> {
        let statement = self.client.prepare("UPDATE accounting SET earners = $1::jsonb, spenders = $2::jsonb, updated = $3 WHERE channel_id = $4 RETURNING channel, earners, spenders, updated, created").await?;

        let earners = Json(&new_balances.earners);
        let spenders = Json(&new_balances.spenders);
        let updated = Some(Utc::now());

        // we are using the RETURNING statement and selecting all field to return the new Accounting
        let row = self
            .client
            .query_one(&statement, &[&earners, &spenders, &updated, &channel.id()])
            .await?;

        let new_accounting = Accounting::try_from(&row)?;

        Ok(new_accounting)
    }
}

#[cfg(test)]
mod test {
    use primitives::{
        sentry::accounting::Balances,
        util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN},
    };

    use crate::db::tests_postgres::{setup_test_migrations, DATABASE_POOL};

    use super::*;

    #[tokio::test]
    async fn store_create_insert_and_update() {
        let test_pool = DATABASE_POOL.get().await.expect("Should get test pool");

        let client = Postgres::new(test_pool.get().await.expect("Should get client"));

        setup_test_migrations(test_pool.clone())
            .await
            .expect("Migrations should succeed");

        let channel = DUMMY_CAMPAIGN.channel.clone();
        // Accounting that does not exist yet
        {
            let non_existing = client
                .fetch(channel.id())
                .await
                .expect("Query should execute");

            assert!(
                non_existing.is_none(),
                "Accounting is empty, we expect no returned accounting for this Channel"
            );
        }

        // Create a new Accounting
        let new_accounting = {
            let mut balances = Balances::<CheckedState>::default();
            balances
                .spend(ADDRESSES["creator"], ADDRESSES["publisher"], 1_000.into())
                .expect("Should not overflow");

            let actual_acc = client
                .create(channel.clone(), balances.clone())
                .await
                .expect("Should insert Accounting");

            let expected_acc = Accounting {
                channel,
                balances,
                updated: None,
                // we have to use the same `created` time for the expected Accounting
                created: actual_acc.created.clone(),
            };

            assert_eq!(expected_acc, actual_acc);

            actual_acc
        };

        // Update Accounting
        {
            let mut new_balances = new_accounting.balances.clone();
            new_balances
                .spend(ADDRESSES["creator"], ADDRESSES["leader"], 500.into())
                .expect("Should not overflow");

            let updated_accounting = client
                .update(&new_accounting.channel, new_balances.clone())
                .await
                .expect("Should update and return the updated Accounting");

            assert!(
                updated_accounting.updated.is_some(),
                "the Updated time should be present now"
            );

            assert_eq!(
                &new_balances, &updated_accounting.balances,
                "Should update the new balances accordingly"
            );
        }
    }
}
