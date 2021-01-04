use crate::access::check_access;
use crate::access::Error as AccessError;
use crate::db::event_aggregate::insert_event_aggregate;
use crate::db::get_channel_by_id;
use crate::db::DbPool;
use crate::event_reducer;
use crate::Application;
use crate::ResponseError;
use crate::Session;
use crate::{analytics_recorder, Auth};
use async_std::sync::RwLock;
use chrono::Utc;
use lazy_static::lazy_static;
use primitives::adapter::Adapter;
use primitives::sentry::{Event, EventAggregate};
use primitives::{Channel, ChannelId};
use slog::{error, Logger};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::delay_for;

lazy_static! {
    pub static ref ANALYTICS_RECORDER: Option<String> = env::var("ANALYTICS_RECORDER").ok();
}

#[derive(Debug)]
struct Record {
    channel: Channel,
    aggregate: EventAggregate,
}

type Recorder = Arc<RwLock<HashMap<ChannelId, Record>>>;

#[derive(Default, Clone)]
pub struct EventAggregator {
    recorder: Recorder,
}

pub fn new_aggr(channel_id: &ChannelId) -> EventAggregate {
    EventAggregate {
        channel_id: channel_id.to_owned(),
        created: Utc::now(),
        events: HashMap::new(),
    }
}

async fn store(db: &DbPool, channel_id: &ChannelId, logger: &Logger, recorder: Recorder) {
    let mut channel_recorder = recorder.write().await;
    let record: Option<&Record> = channel_recorder.get(channel_id);
    if let Some(data) = record {
        if let Err(e) = insert_event_aggregate(&db, &channel_id, &data.aggregate).await {
            error!(&logger, "{}", e; "module" => "event_aggregator", "in" => "store");
        } else {
            // reset aggr record
            let record = Record {
                channel: data.channel.to_owned(),
                aggregate: new_aggr(&channel_id),
            };
            channel_recorder.insert(channel_id.to_owned(), record);
        }
    }
}

impl EventAggregator {
    pub async fn record<'a, A: Adapter>(
        &self,
        app: &'a Application<A>,
        channel_id: &ChannelId,
        session: &Session,
        auth: Option<&Auth>,
        events: &'a [Event],
    ) -> Result<(), ResponseError> {
        let recorder = self.recorder.clone();
        let aggr_throttle = app.config.aggr_throttle;
        let dbpool = app.pool.clone();
        let redis = app.redis.clone();
        let logger = app.logger.clone();

        let mut channel_recorder = self.recorder.write().await;
        let record: &mut Record = match channel_recorder.get_mut(&channel_id) {
            Some(record) => record,
            None => {
                // fetch channel
                let channel = get_channel_by_id(&app.pool, &channel_id)
                    .await?
                    .ok_or(ResponseError::NotFound)?;

                let withdraw_period_start = channel.spec.withdraw_period_start;
                let channel_id = channel.id;
                let record = Record {
                    channel,
                    aggregate: new_aggr(&channel_id),
                };

                // insert into
                channel_recorder.insert(channel_id.to_owned(), record);

                //
                // spawn async task that persists
                // the channel events to database
                if aggr_throttle > 0 {
                    let recorder = recorder.clone();
                    tokio::spawn(async move {
                        loop {
                            // break loop if the
                            // channel withdraw period has started
                            // since no event is allowed once a channel
                            // is in withdraw period

                            if Utc::now() > withdraw_period_start {
                                break;
                            }

                            delay_for(Duration::from_millis(aggr_throttle as u64)).await;
                            store(&dbpool, &channel_id, &logger, recorder.clone()).await;
                        }
                    });
                }

                channel_recorder
                    .get_mut(&channel_id)
                    .expect("should have aggr, we just inserted")
            }
        };

        check_access(
            &app.redis,
            session,
            auth,
            &app.config.ip_rate_limit,
            &record.channel,
            events,
        )
        .await
        .map_err(|e| match e {
            AccessError::OnlyCreatorCanCloseChannel | AccessError::ForbiddenReferrer => {
                ResponseError::Forbidden(e.to_string())
            }
            AccessError::RulesError(error) => ResponseError::TooManyRequests(error),
            AccessError::UnAuthenticated => ResponseError::Unauthorized,
            _ => ResponseError::BadRequest(e.to_string()),
        })?;

        events.iter().for_each(|ev| {
            match event_reducer::reduce(
                &app.logger,
                &record.channel,
                &mut record.aggregate,
                ev,
                &session,
            ) {
                Ok(_) => {}
                Err(err) => error!(&app.logger, "Event Reducer failed"; "error" => ?err ),
            }
        });

        // only time we don't have session is during
        // an unauthenticated close event
        if ANALYTICS_RECORDER.is_some() {
            tokio::spawn(analytics_recorder::record(
                redis.clone(),
                record.channel.clone(),
                session.clone(),
                events.to_owned().to_vec(),
                app.logger.clone(),
            ));
        }

        // drop write access to RwLock
        // this is required to prevent a deadlock in store
        drop(channel_recorder);

        if aggr_throttle == 0 {
            store(&app.pool, &channel_id, &app.logger, recorder.clone()).await;
        }

        Ok(())
    }
}
