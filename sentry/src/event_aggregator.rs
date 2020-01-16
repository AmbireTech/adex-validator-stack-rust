use crate::event_reducer;
use primitives::{ChannelId, Channel};
use primitives::adapter::Adapter;
use primitives::sentry::{EventAggregate, Event};
use std::collections::HashMap;
use crate::db::event_aggregate::insert_event_aggregate;
use crate::Session;
use crate::access::check_access;
use crate::Application;
use chrono::{Utc, Duration};
use crate::db::DbPool;
use tokio::time::{delay_for};
use std::time::Duration as TimeDuration;
use crate::ResponseError;

// use futures::
use async_std::sync::RwLock;
use std::sync::{Arc};

#[derive(Default, Clone)]
pub struct EventAggregator {
    // recorder: HashMap<ChannelId, FnMut(&Session, &str) -> BoxFuture>,
    aggregate: Arc<RwLock<HashMap<String, EventAggregate>>>
}

pub fn new_aggr(channel_id: &ChannelId) -> EventAggregate {
    EventAggregate {
        channel_id: channel_id.to_owned(),
        created: Utc::now(),
        events: HashMap::new(),
    }
}

async fn store(db: &DbPool, channel_id: &ChannelId, aggr: Arc<RwLock<HashMap<String, EventAggregate>>>) {
    let recorder = aggr.write().await;
    let ev_aggr: Option<&EventAggregate> = recorder.get(&channel_id.to_string());
    if let Some(data) = ev_aggr {
        if let Err(e) = insert_event_aggregate(&db, &channel_id, data).await {
            eprintln!("{}", e);
        };
    }
}

impl EventAggregator {
    pub async fn record<'a, A: Adapter + 'static>(
        &'static self,
        app: &'static Application<A>,
        channel: Channel,
        session: Session,
        events: &'a [Event],
        ) -> Result<(), ResponseError>
    {
        // eventAggrCol
        // channelsCol
        // try getting aggr if none create and store new aggr
        // redis: &MultiplexedConnection,
        // session: &Session,
        // rate_limit: &RateLimit,
        // channel: &Channel,
        // events: &[Event]
        let has_access = check_access(&app.redis, &session, &app.config.ip_rate_limit, &channel, events).await;
        if let Err(e) = has_access {
            return Err(ResponseError::BadRequest(e.to_string()));
        }

        let mut recorder = self.aggregate.write().await;
        let mut aggr: &mut EventAggregate = if let Some(aggr) = recorder.get_mut(&channel.id.to_string()) {
            aggr
        } else {
            // insert into 
            recorder.insert(channel.id.to_string(), new_aggr(&channel.id));
            recorder.get_mut(&channel.id.to_string()).expect("should have aggr, we just inserted")
        };

        // if aggr is none
        // spawn a tokio task for saving to database
        events.iter().for_each( | ev| event_reducer::reduce(&channel, &mut aggr, ev));
        let created = aggr.created;

        // drop write access to RwLock and mut access to aggr
        // we don't need it anymore
        drop(recorder);

        if app.config.aggr_throttle > 0
            &&
            Utc::now() > (created + Duration::seconds(app.config.aggr_throttle as i64))
        {

            tokio::spawn(
                async move {
                    delay_for(TimeDuration::from_secs(app.config.aggr_throttle as u64)).await;
                    store(&app.pool.clone(), &channel.id, self.aggregate.clone()).await;
                }
            );
        } else {
            store(&app.pool, &channel.id, self.aggregate.clone()).await;
        }

        Ok(())
    }
}