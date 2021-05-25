use std::{collections::HashMap, sync::Arc};

use dashmap::{mapref::entry::Entry, DashMap};
use futures::{stream::select, StreamExt};
use primitives::{
    channel_v5::Channel,
    sentry::accounting::{Accounting, Balances, CheckedState},
    Address, ChannelId, UnifiedNum,
};
use thiserror::Error;
use tokio::{
    sync::mpsc::{channel, error::SendError, Receiver, Sender},
    time::interval,
};
use tokio_stream::wrappers::{IntervalStream, ReceiverStream};

pub use self::store::{Client, Store};

#[derive(Debug)]
pub struct Payment {
    pub earner: Address,
    pub amount: UnifiedNum,
}

pub type Spender = Address;
pub type Payouts = HashMap<Spender, Payment>;

struct Value {
    channel: Channel,
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
    Sending(#[from] SendError<(Channel, Payouts)>),
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
    pub async fn record(&self, channel: Channel, payouts: Payouts) -> Result<(), Error> {
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
            let receiver_stream = ReceiverStream::new(receiver).map(TimeFor::EventAggregation);

            let mut select_time = select(update_interval, receiver_stream);

            while let Some(time_for) = select_time.next().await {
                match time_for {
                    TimeFor::AggregatesUpdate => {
                        todo!("call persistence Store and get Accounting")
                    }
                    TimeFor::EventAggregation(Value { channel, payouts }) => {
                        match aggregates.entry(channel.id()) {
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
                                let existing_accounting = match store.fetch(channel.id()).await {
                                    Ok(accounting) => accounting,
                                    Err(_error) => {
                                        // TODO: Log error from store

                                        // send the value back to try and update the accounting again.
                                        match sender.send(Value { channel, payouts }).await {
                                            Ok(_) => {}
                                            Err(_error) => todo!("log error"), // error!(&logger, "Something terrible went wrong, receiver is closed?!")
                                        }
                                        continue;
                                    }
                                };

                                let accounting_result = match existing_accounting {
                                    Some(Accounting {
                                        mut balances,
                                        channel,
                                        ..
                                    }) => {
                                        for (spender, payout) in payouts {
                                            balances
                                                .spend(spender, payout.earner, payout.amount)
                                                .expect("Should not overflow");
                                        }

                                        store.update(&channel, balances).await
                                    }
                                    None => {
                                        let mut balances = Balances::<CheckedState>::default();
                                        for (spender, payout) in payouts {
                                            balances
                                                .spend(spender, payout.earner, payout.amount)
                                                .expect("Should not overflow");
                                        }

                                        store.create(channel, balances).await
                                    }
                                };

                                match accounting_result {
                                    Ok(accounting) => {
                                        vacant.insert(accounting.balances);
                                    }
                                    Err(_err) => {
                                        // TODO: Log Store error
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    pub async fn handle() {}
}

pub mod store {
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
}

enum TimeFor {
    AggregatesUpdate,
    EventAggregation(Value),
}
