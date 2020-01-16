use crate::access::check_access;
use crate::db::event_aggregate::insert_event_aggregate;
use crate::db::DbPool;
use crate::event_reducer;
use crate::Application;
use crate::ResponseError;
use crate::Session;
use chrono::{Duration, Utc};
use primitives::adapter::Adapter;
use primitives::sentry::{Event, EventAggregate};
use primitives::{Channel, ChannelId};
use std::collections::HashMap;
use std::time::Duration as TimeDuration;
use tokio::time::delay_for;

// use futures::
use async_std::sync::RwLock;
use std::sync::Arc;

#[derive(Default, Clone)]
pub struct EventAggregator {
    aggregate: Arc<RwLock<HashMap<String, EventAggregate>>>,
}

pub fn new_aggr(channel_id: &ChannelId) -> EventAggregate {
    EventAggregate {
        channel_id: channel_id.to_owned(),
        created: Utc::now(),
        events: HashMap::new(),
    }
}

async fn store(
    db: &DbPool,
    channel_id: &ChannelId,
    aggr: Arc<RwLock<HashMap<String, EventAggregate>>>,
) {
    let mut recorder = aggr.write().await;
    let ev_aggr: Option<&EventAggregate> = recorder.get(&channel_id.to_string());
    if let Some(data) = ev_aggr {
        if let Err(e) = insert_event_aggregate(&db, &channel_id, data).await {
            eprintln!("{}", e);
            return;
        };
        // reset aggr
        recorder.insert(channel_id.to_string(), new_aggr(&channel_id));
    }
}

impl EventAggregator {
    pub async fn record<'a, A: Adapter>(
        &self,
        app: &'a Application<A>,
        channel: Channel,
        session: Session,
        events: &'a [Event],
    ) -> Result<(), ResponseError> {
        let has_access = check_access(
            &app.redis,
            &session,
            &app.config.ip_rate_limit,
            &channel,
            events,
        )
        .await;
        if let Err(e) = has_access {
            return Err(ResponseError::BadRequest(e.to_string()));
        }

        let mut recorder = self.aggregate.write().await;
        let mut aggr: &mut EventAggregate =
            if let Some(aggr) = recorder.get_mut(&channel.id.to_string()) {
                aggr
            } else {
                // insert into
                recorder.insert(channel.id.to_string(), new_aggr(&channel.id));
                recorder
                    .get_mut(&channel.id.to_string())
                    .expect("should have aggr, we just inserted")
            };

        // if aggr is none
        events
            .iter()
            .for_each(|ev| event_reducer::reduce(&channel, &mut aggr, ev));
        let created = aggr.created;
        let dbpool = app.pool.clone();
        let aggr_throttle = app.config.aggr_throttle;
        let aggregate = self.aggregate.clone();

        // drop write access to RwLock
        // this is required to prevent a deadlock in store
        drop(recorder);

        // Checks if aggr_throttle is set
        // and if current time is greater than aggr.created plus throttle seconds
        //
        // This approach spawns an async task every > AGGR_THROTTLE seconds
        // Each spawned task resolves after AGGR_THROTTLE seconds
        //

        if aggr_throttle > 0 && Utc::now() > (created + Duration::seconds(aggr_throttle as i64)) {
            // spawn a tokio task for saving to database
            tokio::spawn(async move {
                delay_for(TimeDuration::from_secs(aggr_throttle as u64)).await;
                store(&dbpool, &channel.id, aggregate).await;
            });
        } else {
            store(&app.pool, &channel.id, aggregate).await;
        }

        Ok(())
    }
}
