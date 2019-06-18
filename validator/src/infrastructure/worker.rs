pub mod single {
    use crate::domain::channel::ChannelRepository;
    use crate::infrastructure::persistence::channel::api::ApiChannelRepository;
    use crate::infrastructure::sentry::SentryApi;
    use crate::infrastructure::validator::follower::Follower;
    use crate::infrastructure::validator::leader::Leader;

    #[derive(Clone)]
    pub struct TickWorker {
        pub leader: Leader,
        pub follower: Follower,
        pub sentry: SentryApi,
    }

    /// Single tick worker
    impl TickWorker {
        pub async fn tick(self) -> Result<(), ()> {
            let repo = ApiChannelRepository {
                sentry: self.sentry.clone(),
            };

            let all_channels = await!(repo.all("0x2892f6C41E0718eeeDd49D98D648C789668cA67d"));

            match all_channels {
                Ok(channel) => println!("{:#?}", channel),
                Err(error) => eprintln!("Error occurred: {:#?}", error),
            };

            Ok(())
        }
    }
}
pub mod infinite {
    use crate::infrastructure::sentry::SentryApi;
    use crate::infrastructure::worker::single::TickWorker;
    use futures::compat::Future01CompatExt;
    use futures::future::join;
    use reqwest::r#async::Client;
    use std::ops::Add;
    use std::time::{Duration, Instant};
    use tokio::timer::Delay;

    #[derive(Clone)]
    pub struct InfiniteWorker {
        pub tick_worker: TickWorker,
    }

    /// Infinite tick worker
    impl InfiniteWorker {
        pub async fn infinite(self) -> Result<(), ()> {
            let sentry = SentryApi {
                client: Client::new(),
                //                sentry_url: CONFIG.sentry_url.clone(),
                sentry_url: "http://localhost:8005".to_string(),
            };
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
}
