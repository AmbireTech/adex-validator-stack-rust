use crate::event_reducer;
use primitives::{ChannelId, Channel};
use primitives::adapter::Adapter;
use primitives::sentry::{EventAggregate, Event};
use std::collections::HashMap;
use futures::future::BoxFuture;
use crate::Session;
use crate::access::check_access;
use crate::Application;
use chrono::{Utc, Duration};
use async_std::stream;

// use futures::
use async_std::sync::RwLock;
use std::sync::{Arc};

pub struct EventAggregator {
    // recorder: HashMap<ChannelId, FnMut(&Session, &str) -> BoxFuture>,
    aggregate: Arc<RwLock<HashMap<ChannelId, EventAggregate>>>
}

fn persist(aggr_throttle: i64, channel_id: ChannelId, aggr: Arc<RwLock<HashMap<ChannelId, EventAggregate>>>) {
    let mut interval = stream::interval(Duration::from_secs(aggr_throttle));
    while let Some(_) = interval.next().await {
//        loop through the keys and persist them in the
//        database
    }
}

impl EventAggregator {
    pub async fn record<A: Adapter>(
        &self,
        app: &Application<A>,
        channel: &Channel,
        session: &Session,
        events: &[Event]
        )
    {
        // eventAggrCol
        // channelsCol
        // try getting aggr if none create and store new aggr
    //     redis: &MultiplexedConnection,
    // session: &Session,
    // rate_limit: &RateLimit,
    // channel: &Channel,
    // events: &[Event]
        let has_access = check_access(&app.redis, session, &app.config.ip_rate_limit, channel, events).await;
        if has_access.is_err() {
            // return the error
        }
        let recorder = self.aggregate.write().await.expect("should acquire lock");
        let mut aggr: EventAggregate = *recorder.get_mut(&channel.id);
        // if aggr is none
        // spawn a tokio task for saving to database
        events.iter().for_each( | ev| event_reducer::reduce(channel, &mut aggr, ev));

        if app.config.aggr_throttle > 0
            &&
            Utc::now() < (aggr.created + Duration::seconds(app.config.aggr_throttle as i64))
        {

        }





    }
}