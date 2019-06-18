pub use self::infinite::InfiniteWorker;
pub use self::single::TickWorker;

pub mod single {
    use std::sync::Arc;

    use futures::future::FutureExt;

    use crate::domain::channel::ChannelRepository;
    use crate::domain::{Worker, WorkerFuture};
    use crate::infrastructure::validator::follower::Follower;
    use crate::infrastructure::validator::leader::Leader;

    #[derive(Clone)]
    pub struct TickWorker {
        pub leader: Leader,
        pub follower: Follower,
        pub channel_repository: Arc<dyn ChannelRepository>,
    }

    /// Single tick worker
    impl TickWorker {
        pub async fn tick(self) -> Result<(), ()> {
            let all_channels = await!(self
                .channel_repository
                .all("0x2892f6C41E0718eeeDd49D98D648C789668cA67d"));

            match all_channels {
                Ok(channel) => println!("{:#?}", channel),
                Err(error) => eprintln!("Error occurred: {:#?}", error),
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
