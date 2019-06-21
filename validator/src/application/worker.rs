pub use self::infinite::InfiniteWorker;
pub use self::single::TickWorker;

pub mod single {
    use std::sync::Arc;
    use std::time::Duration;

    use futures::compat::Future01CompatExt;
    use futures::future::{FutureExt, TryFutureExt};
    use tokio::util::FutureExt as TokioFutureExt;

    use domain::{Channel, SpecValidator};

    use crate::application::validator::{Follower, Leader};
    use crate::domain::{ChannelRepository, Validator, Worker, WorkerFuture};

    #[derive(Clone)]
    pub struct TickWorker {
        pub leader: Leader,
        pub follower: Follower,
        pub channel_repository: Arc<dyn ChannelRepository>,
        // @TODO: use the adapter(maybe?) instead of repeating the identity
        pub identity: String,
        // @TODO: Pass configuration by which this can be set
        pub validation_tick_timeout: Duration,
    }

    /// Single tick worker
    impl TickWorker {
        pub async fn tick(self) -> Result<(), ()> {
            let all_channels = await!(self.channel_repository.all(&self.identity));

            match all_channels {
                Ok(channels) => {
                    for channel in channels {
                        await!(self.clone().handle_channel(channel)).unwrap();
                    }
                }
                Err(error) => eprintln!("Error occurred: {:#?}", error),
            };

            Ok(())
        }

        async fn handle_channel(self, channel: Channel) -> Result<(), ()> {
            let channel_id = channel.id;

            match &channel.spec.validators.find(&self.identity) {
                SpecValidator::Leader(_) => {
                    let tick_future = self.leader.tick(channel);

                    let tick_result = await!(tick_future
                        .compat()
                        .timeout(self.validation_tick_timeout)
                        .compat());

                    match tick_result {
                        Ok(_) => println!("Channel {} handled as __Leader__", channel_id),
                        Err(_) => eprintln!("Channel {} Timed out", channel_id),
                    }
                }
                SpecValidator::Follower(_) => {
                    let tick_future = self.follower.tick(channel);

                    let tick_result = await!(tick_future
                        .compat()
                        .timeout(self.validation_tick_timeout)
                        .compat());

                    match tick_result {
                        Ok(_) => println!("Channel {} handled as __Follower__", channel_id),
                        Err(_) => eprintln!("Channel {} Timed out", channel_id),
                    }
                }
                SpecValidator::None => {
                    eprintln!("Channel {} is not validated by us", channel_id);
                }
            };

            Ok(())
        }
    }

    impl Worker for TickWorker {
        fn run(&self) -> WorkerFuture {
            self.clone().tick().boxed()
        }
    }
}
pub mod infinite {
    use std::ops::Add;
    use std::time::{Duration, Instant};

    use futures::compat::Future01CompatExt;
    use futures::future::{join, FutureExt};
    use tokio::timer::Delay;

    use crate::application::worker::TickWorker;
    use crate::domain::{Worker, WorkerFuture};

    #[derive(Clone)]
    pub struct InfiniteWorker {
        pub tick_worker: TickWorker,
        pub ticks_wait_time: Duration,
    }

    /// Infinite tick worker
    impl InfiniteWorker {
        pub async fn infinite(self) -> Result<(), ()> {
            let handle = self.clone();
            loop {
                let future = handle.clone().tick_worker.tick();
                let tick_future = Delay::new(Instant::now().add(self.ticks_wait_time));

                let joined = join(future, tick_future.compat());

                await!(joined);
            }
        }
    }

    impl Worker for InfiniteWorker {
        fn run(&self) -> WorkerFuture {
            self.clone().infinite().boxed()
        }
    }
}
