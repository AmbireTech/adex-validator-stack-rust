use chrono::Utc;
use futures::future::try_join_all;
use redis::aio::MultiplexedConnection;

use crate::Session;
use primitives::event_submission::{RateLimit, Rule};
use primitives::sentry::Event;
use primitives::Channel;
use std::cmp::PartialEq;
use std::error::Error;
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum AccessError {
    OnlyCreatorCanCloseChannel,
    ChannelIsExpired,
    ChannelIsInWithdrawPeriod,
    RulesError(String),
}

impl Error for AccessError {}

impl fmt::Display for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccessError::OnlyCreatorCanCloseChannel => write!(f, "only creator can create channel"),
            AccessError::ChannelIsExpired => write!(f, "channel has expired"),
            AccessError::ChannelIsInWithdrawPeriod => write!(f, "channel is in withdraw period"),
            AccessError::RulesError(error) => write!(f, "{}", error),
        }
    }
}

// @TODO: Make pub(crate)
pub async fn check_access(
    redis: &MultiplexedConnection,
    session: &Session,
    rate_limit: &RateLimit,
    channel: &Channel,
    events: &[Event],
) -> Result<(), AccessError> {
    let is_close_event = |e: &Event| match e {
        Event::Close => true,
        _ => false,
    };
    let current_time = Utc::now();
    let is_in_withdraw_period = current_time > channel.spec.withdraw_period_start;

    if current_time > channel.valid_until {
        return Err(AccessError::ChannelIsExpired);
    }

    // We're only sending a CLOSE
    // That's allowed for the creator normally, and for everyone during the withdraw period
    if events.iter().all(is_close_event)
        && (session.uid == channel.creator || is_in_withdraw_period)
    {
        return Ok(());
    }

    // Only the creator can send a CLOSE
    if session.uid != channel.creator && events.iter().any(is_close_event) {
        return Err(AccessError::OnlyCreatorCanCloseChannel);
    }

    if is_in_withdraw_period {
        return Err(AccessError::ChannelIsInWithdrawPeriod);
    }

    let default_rules = [
        Rule {
            uids: Some(vec![channel.creator.to_string()]),
            rate_limit: None,
        },
        Rule {
            uids: None,
            rate_limit: Some(rate_limit.clone()),
        },
    ];
    // Enforce access limits
    let allow_rules = channel
        .spec
        .event_submission
        .as_ref()
        .map(|ev_sub| ev_sub.allow.as_slice())
        .unwrap_or_else(|| &default_rules);

    // first, find an applicable access rule
    let rules = allow_rules
        .iter()
        .filter(|r| match &r.uids {
            Some(uids) => uids.iter().any(|uid| uid.eq(&session.uid.to_string())),
            None => true,
        })
        .collect::<Vec<_>>();

    if rules.iter().any(|r| r.rate_limit.is_none()) {
        // We matched a rule that has *no rateLimit*, so we're good
        return Ok(());
    }

    let apply_all_rules = try_join_all(
        rules
            .iter()
            .map(|rule| apply_rule(redis.clone(), &rule, &events, &channel, &session)),
    );

    if let Err(rule_error) = apply_all_rules.await {
        Err(AccessError::RulesError(rule_error))
    } else {
        Ok(())
    }
}

async fn apply_rule(
    redis: MultiplexedConnection,
    rule: &Rule,
    events: &[Event],
    channel: &Channel,
    session: &Session,
) -> Result<(), String> {
    match &rule.rate_limit {
        Some(rate_limit) => {
            let key = if &rate_limit.limit_type == "sid" {
                Ok(format!(
                    "adexRateLimit:{}:{}",
                    hex::encode(channel.id),
                    session.uid
                ))
            } else if &rate_limit.limit_type == "ip" {
                if events.len() != 1 {
                    Err("rateLimit: only allows 1 event".to_string())
                } else {
                    Ok(format!(
                        "adexRateLimit:{}:{}",
                        hex::encode(channel.id),
                        session.ip.as_ref().unwrap_or(&String::new())
                    ))
                }
            } else {
                // return for the whole function
                return Ok(());
            }?;

            if redis::cmd("EXISTS")
                .arg(&key)
                .query_async::<_, i8>(&mut redis.clone())
                .await
                .map(|exists| exists == 1)
                .map_err(|error| format!("{}", error))?
            {
                return Err("rateLimit: too many requests".to_string());
            }

            let seconds = rate_limit.time_frame.as_secs_f32().ceil();

            redis::cmd("SETEX")
                .arg(&key)
                .arg(seconds as i32)
                .arg("1")
                .query_async::<_, ()>(&mut redis.clone())
                .await
                .map_err(|error| format!("{}", error))
        }
        None => Ok(()),
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use primitives::config::configuration;
    use primitives::event_submission::{RateLimit, Rule};
    use primitives::sentry::Event;
    use primitives::util::tests::prep_db::{DUMMY_CHANNEL, IDS};
    use primitives::{Channel, Config, EventSubmission};

    use crate::db::redis_connection;
    use crate::Session;

    use super::*;

    async fn setup() -> (Config, MultiplexedConnection) {
        let mut redis = redis_connection().await.expect("Couldn't connect to Redis");
        let config = configuration("development", None).expect("Failed to get dev configuration");

        // run `FLUSHALL` to clean any leftovers of other tests
        let _ = redis::cmd("FLUSHALL")
            .query_async::<_, String>(&mut redis)
            .await;

        (config, redis)
    }

    fn get_channel(with_rule: Rule) -> Channel {
        let mut channel = DUMMY_CHANNEL.clone();

        channel.spec.event_submission = Some(EventSubmission {
            allow: vec![with_rule],
        });

        channel
    }

    fn get_impression_events(count: i8) -> Vec<Event> {
        (0..count)
            .map(|_| Event::Impression {
                publisher: IDS["publisher2"].clone(),
                ad_unit: None,
            })
            .collect()
    }

    #[tokio::test]
    async fn session_uid_rate_limit() {
        let (config, redis) = setup().await;

        let session = Session {
            era: 0,
            uid: IDS["follower"].clone(),
            ip: Default::default(),
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "sid".to_string(),
                time_frame: Duration::from_millis(20_000),
            }),
        };
        let events = get_impression_events(2);
        let channel = get_channel(rule);

        let response =
            check_access(&redis, &session, &config.ip_rate_limit, &channel, &events).await;
        assert_eq!(Ok(()), response);

        let err_response =
            check_access(&redis, &session, &config.ip_rate_limit, &channel, &events).await;
        assert_eq!(
            Err(AccessError::RulesError(
                "rateLimit: too many requests".to_string()
            )),
            err_response
        );
    }

    #[tokio::test]
    async fn ip_rate_limit() {
        let (config, redis) = setup().await;

        let session = Session {
            era: 0,
            uid: IDS["follower"].clone(),
            ip: Default::default(),
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(20_000),
            }),
        };
        let channel = get_channel(rule);

        let err_response = check_access(
            &redis,
            &session,
            &config.ip_rate_limit,
            &channel,
            &get_impression_events(2),
        )
        .await;

        assert_eq!(
            Err(AccessError::RulesError(
                "rateLimit: only allows 1 event".to_string()
            )),
            err_response
        );

        let response = check_access(
            &redis,
            &session,
            &config.ip_rate_limit,
            &channel,
            &get_impression_events(1),
        )
        .await;
        assert_eq!(Ok(()), response);
    }
}
