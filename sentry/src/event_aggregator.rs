use crate::access::check_access;
use crate::db::event_aggregate::insert_event_aggregate;
use crate::db::DbPool;
use crate::event_reducer;
use crate::Application;
use crate::ResponseError;
use crate::Session;
use async_std::sync::RwLock;
use chrono::Utc;
use primitives::adapter::Adapter;
use primitives::sentry::{Event, EventAggregate};
use primitives::{Channel, ChannelId};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::delay_for;

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
        channel: &Channel,
        session: &Session,
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
        let aggr_throttle = app.config.aggr_throttle;
        let dbpool = app.pool.clone();
        let aggregate = self.aggregate.clone();
        let withdraw_period_start = channel.spec.withdraw_period_start;
        let channel_id = channel.id;

        let mut aggr: &mut EventAggregate = match recorder.get_mut(&channel.id.to_string()) {
            Some(aggr) => aggr,
            None => {
                // insert into
                recorder.insert(channel.id.to_string(), new_aggr(&channel.id));

                // spawn async task that persists
                // the channel events to database
                if aggr_throttle > 0 {
                    tokio::spawn(async move {
                        loop {
                            // break loop if the
                            // channel withdraw period has started
                            // since no event is allowed once a channel
                            // is in withdraw period

                            if Utc::now() > withdraw_period_start {
                                break;
                            }

                            delay_for(Duration::from_secs(aggr_throttle as u64)).await;
                            store(&dbpool, &channel_id, aggregate.clone()).await;
                        }
                    });
                }

                recorder
                    .get_mut(&channel.id.to_string())
                    .expect("should have aggr, we just inserted")
            }
        };

        events
            .iter()
            .for_each(|ev| event_reducer::reduce(&channel, &mut aggr, ev));

        // drop write access to RwLock
        // this is required to prevent a deadlock in store
        drop(recorder);

        if aggr_throttle == 0 {
            println!("storing data in ");
            store(&app.pool, &channel.id, self.aggregate.clone()).await;
        }

        println!("finished succesfully");

        Ok(())
    }
}
