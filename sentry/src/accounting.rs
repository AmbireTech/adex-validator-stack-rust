use std::{collections::HashMap, sync::Arc};

use dashmap::{mapref::entry::Entry, DashMap};
use futures::{stream::select, StreamExt};
use primitives::{
    sentry::accounting::{Balances, CheckedState},
    Address, ChannelId, UnifiedNum,
};
use thiserror::Error;
use tokio::{
    sync::mpsc::{channel, error::SendError, Receiver, Sender},
    time::interval,
};
use tokio_stream::wrappers::{IntervalStream, ReceiverStream};

use self::store::{Client, Store};

#[derive(Debug)]
pub struct Payment {
    pub earner: Address,
    pub amount: UnifiedNum,
}

pub type Spender = Address;
pub type Payouts = HashMap<Spender, Payment>;

struct Value {
    channel: ChannelId,
    payouts: Payouts,
}

pub struct Aggregator<C: Client> {
    aggregates: Arc<DashMap<ChannelId, Balances<CheckedState>>>,
    receiver: Option<Receiver<Value>>,
    sender: Sender<Value>,
    store: Store<C>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Overflow(String),
    #[error(transparent)]
    Sending(#[from] SendError<(ChannelId, Payouts)>),
    #[error("The sum of Earning and the sum of Spending should be equal")]
    PayoutsDiffer,
}

impl<C> Aggregator<C>
where
    C: Client + 'static,
{
    pub async fn initialize(store: Store<C>) -> Self {
        let (sender, receiver) = channel(500);

        // fetch all aggregates from Store and load then in the Aggregator
        // E.g.:
        // let aggregates = store.load_all().await?;

        Self {
            aggregates: Default::default(),
            receiver: Some(receiver),
            sender,
            store,
        }
    }

    /// Validates that the `sum(earners)` == `sum(spenders)` before sending the payouts for recording
    pub async fn record(&self, channel: ChannelId, payouts: Payouts) -> Result<(), Error> {
        self.sender
            .send(Value { channel, payouts })
            .await
            .map_err(|err| SendError((err.0.channel, err.0.payouts)))
            .map_err(Error::Sending)
    }

    /// spawns a thread for:
    /// 1. Aggregating received from the channel events into the Accounting
    /// 2. On regular intervals checks the `Aggregate.updated_at` and updates the aggregate in the persistence store
    /// # Panics
    /// If `spawn()` was called before it will panic as [`tokio::channel::Receiver`](tokio::channel::Receiver) can only be used from **one** place
    pub fn spawn(&mut self) {
        let receiver = self
            .receiver
            .take()
            .expect("Only 1 receiver can be used and spawn() should be called only once!");

        let aggregates = self.aggregates.clone();
        // if recording a value fails, we can send the payout again and try to processes it later
        let sender = self.sender.clone();
        // store
        let store = self.store.clone();

        tokio::spawn(async move {
            // todo: Should be configurable value
            let period = std::time::Duration::from_secs(5 * 60);

            let update_interval =
                IntervalStream::new(interval(period)).map(|_e| TimeFor::AggregatesUpdate);
            let receiver_stream =
                ReceiverStream::new(receiver).map(|v| TimeFor::EventAggregation(v));

            let mut select_time = select(update_interval, receiver_stream);

            while let Some(time_for) = select_time.next().await {
                match time_for {
                    TimeFor::AggregatesUpdate => {
                        todo!("call persistence Store and get Accounting")
                    }
                    TimeFor::EventAggregation(Value { channel, payouts }) => {
                        match aggregates.entry(channel) {
                            // entry that already exists and is fetched from Store
                            Entry::Occupied(mut entry) => {
                                let balances = entry.get_mut();

                                for (spender, payout) in payouts {
                                    // TODO: Handle overflow properly and log the message
                                    balances
                                        .spend(spender, payout.earner, payout.amount)
                                        .expect("Overflow!")
                                }
                            }
                            Entry::Vacant(vacant) => {
                                let accounting = match store.fetch(channel).await {
                                    Ok(accounting) => accounting,
                                    Err(error) => {
                                        // TODO: Log error from store

                                        // send the value back to try and update the accounting again.
                                        match sender.send(Value { channel, payouts }).await {
                                            Ok(_) => {}
                                            Err(_error) => todo!("log error"), // error!(&logger, "Something terrible went wrong, receiver is closed?!")
                                        }
                                        continue;
                                    }
                                };

                                // let mut balances = Balances::<CheckedState>::default();
                                // for (spender, payout) in payouts {
                                //     balances.spend(spender, payout.earner, payout.amount);
                                // }

                                /* let accounting = Accounting {
                                    channel_id: channel,
                                    balances,
                                    updated_at: None,
                                    created_at: Utc::now(),
                                }; */

                                // try to save in Store the first time we receive a new value

                                // vacant.insert()
                            }
                        }
                    }
                }
            }
        });
    }

    pub async fn handle() {}
}

mod store {
    use async_trait::async_trait;
    use std::{ops::Deref, sync::Arc};

    use primitives::{
        channel_v5::Channel,
        sentry::accounting::{Accounting, Balances, CheckedState},
        ChannelId,
    };

    #[derive(Debug)]
    pub struct Store<C: Client> {
        inner: Arc<C>,
    }

    impl<C: Client> Clone for Store<C> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
            }
        }
    }

    impl<C: Client> Deref for Store<C> {
        type Target = C;

        fn deref(&self) -> &Self::Target {
            &self.inner.as_ref()
        }
    }

    #[async_trait]
    pub trait Client: Send + Sync {
        type Error: Send;

        async fn fetch(
            &self,
            channel: ChannelId,
        ) -> Result<Option<Accounting<CheckedState>>, Self::Error>;

        async fn create(
            &self,
            channel: Channel,
            balances: Balances<CheckedState>,
        ) -> Result<Accounting<CheckedState>, Self::Error>;

        async fn update(
            &self,
            channel: &Channel,
            new_balances: Balances<CheckedState>,
        ) -> Result<Accounting<CheckedState>, Self::Error>;
    }

    // TODO: Move to DB
    pub mod postgres {
        use super::*;

        use async_trait::async_trait;
        use chrono::Utc;
        use primitives::{channel_v5::Channel, sentry::accounting::Balances};
        use std::convert::TryFrom;
        use thiserror::Error;
        use tokio_postgres::types::Json;

        #[derive(Debug, Error)]
        pub enum Error {
            #[error("Accounting Balances error: {0}")]
            Balances(#[from] primitives::sentry::accounting::Error),
            #[error("Fetching Accounting from postgres error: {0}")]
            Postgres(#[from] tokio_postgres::Error),
            #[error(
                "Creating or updating a record did not return the expected number of modified rows"
            )]
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

            async fn update(
                &self,
                channel: &Channel,
                new_balances: Balances<CheckedState>,
            ) -> Result<Accounting<CheckedState>, Self::Error> {
                let statement = self.client.prepare("UPDATE accounting SET earners = $1::jsonb, spenders = $2::jsonb, updated = $3 WHERE channel_id = $4 RETURNING channel_id, channel, earners, spenders, updated, created").await?;

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
    }
}

enum TimeFor {
    AggregatesUpdate,
    EventAggregation(Value),
}
