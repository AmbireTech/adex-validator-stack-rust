pub use self::infinite::InfiniteWorker;
pub use self::single::TickWorker;

pub mod single {
    use std::sync::Arc;

    use futures::future::FutureExt;

    use crate::domain::channel::ChannelRepository;
    use crate::domain::validator::Validator;
    use crate::domain::{Worker, WorkerFuture};
    use crate::infrastructure::validator::follower::Follower;
    use crate::infrastructure::validator::leader::Leader;
    use domain::channel::SpecValidator;
    use domain::Channel;

    #[derive(Clone)]
    pub struct TickWorker {
        pub leader: Leader,
        pub follower: Follower,
        pub channel_repository: Arc<dyn ChannelRepository>,
        pub identity: String,
    }

    /// Single tick worker
    impl TickWorker {
        pub async fn tick(self) -> Result<(), ()> {
            let all_channels = await!(self.channel_repository.all(&self.identity));

            match all_channels {
                Ok(channels) => {
                    for channel in channels {
                        await!(self.handle_channel(channel)).unwrap();
                    }
                }
                Err(error) => eprintln!("Error occurred: {:#?}", error),
            };

            Ok(())
        }

        async fn handle_channel(&self, channel: Channel) -> Result<(), ()> {
            let channel_id = channel.id.clone();

            match &channel.spec.validators.find(&self.identity) {
                SpecValidator::Leader(_) => {
                    self.leader.tick(channel);
                    eprintln!("Channel {} handled as __Leader__", channel_id.to_string());
                }
                SpecValidator::Follower(_) => {
                    self.follower.tick(channel);
                    eprintln!("Channel {} handled as __Follower__", channel_id.to_string());
                }
                SpecValidator::None => {
                    eprintln!("Channel {} is not validated by us", channel_id.to_string());
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

    use crate::domain::{Worker, WorkerFuture};
    use crate::infrastructure::worker::TickWorker;

    #[derive(Clone)]
    pub struct InfiniteWorker {
        pub tick_worker: TickWorker,
    }

    /// Infinite tick worker
    impl InfiniteWorker {
        pub async fn infinite(self) -> Result<(), ()> {
            let handle = self.clone();
            loop {
                let future = handle.clone().tick_worker.tick();
                //                let tick_future = Delay::new(Instant::now().add(CONFIG.ticks_wait_time));
                let tick_future = Delay::new(Instant::now().add(Duration::from_millis(5000)));

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
