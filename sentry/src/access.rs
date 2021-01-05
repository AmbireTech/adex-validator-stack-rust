use chrono::Utc;
use futures::future::try_join_all;
use redis::aio::MultiplexedConnection;

use crate::{Auth, Session};
use primitives::event_submission::{RateLimit, Rule};
use primitives::sentry::Event;
use primitives::Channel;
use std::cmp::PartialEq;
use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum Error {
    #[error("only creator can close channel")]
    OnlyCreatorCanCloseChannel,
    #[error("only creator can update targeting rules")]
    OnlyCreatorCanUpdateTargetingRules,
    #[error("channel is expired")]
    ChannelIsExpired,
    #[error("channel is in withdraw period")]
    ChannelIsInWithdrawPeriod,
    #[error("event submission restricted")]
    ForbiddenReferrer,
    #[error("{0}")]
    RulesError(String),
    #[error("unauthenticated")]
    UnAuthenticated,
}

// @TODO: Make pub(crate)
pub async fn check_access(
    redis: &MultiplexedConnection,
    session: &Session,
    auth: Option<&Auth>,
    rate_limit: &RateLimit,
    channel: &Channel,
    events: &[Event],
) -> Result<(), Error> {
    let is_close_event = |e: &Event| matches!(e, Event::Close);
    let is_update_targeting_event = |e: &Event| matches!(e, Event::UpdateTargeting { .. });

    let has_close_event = events.iter().all(is_close_event);
    let has_update_targeting_event = events.iter().all(is_update_targeting_event);
    let current_time = Utc::now();
    let is_in_withdraw_period = current_time > channel.spec.withdraw_period_start;

    if current_time > channel.valid_until {
        return Err(Error::ChannelIsExpired);
    }

    if has_close_event && is_in_withdraw_period {
        return Ok(());
    }

    let (is_creator, auth_uid) = match auth {
        Some(auth) => (auth.uid == channel.creator, auth.uid.to_string()),
        None => (false, Default::default()),
    };
    // We're only sending a CLOSE
    // That's allowed for the creator normally, and for everyone during the withdraw period
    if has_close_event && is_creator {
        return Ok(());
    }

    if has_update_targeting_event && is_creator {
        return Ok(());
    }

    // Only the creator can send a CLOSE
    if !is_creator && events.iter().any(is_close_event) {
        return Err(Error::OnlyCreatorCanCloseChannel);
    }

    // Only the creator can send a UPDATE_TARGETING
    if !is_creator && events.iter().any(is_update_targeting_event) {
        return Err(Error::OnlyCreatorCanUpdateTargetingRules);
    }

    if is_in_withdraw_period {
        return Err(Error::ChannelIsInWithdrawPeriod);
    }

    // Extra rulfes for normal (non-CLOSE) events
    if forbidden_country(&session) || forbidden_referrer(&session) {
        return Err(Error::ForbiddenReferrer);
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
            Some(uids) => uids.iter().any(|uid| uid.eq(&auth_uid)),
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
            .map(|rule| apply_rule(redis.clone(), &rule, &events, &channel, &auth_uid, &session)),
    );

    if let Err(rule_error) = apply_all_rules.await {
        Err(Error::RulesError(rule_error))
    } else {
        Ok(())
    }
}

async fn apply_rule(
    redis: MultiplexedConnection,
    rule: &Rule,
    events: &[Event],
    channel: &Channel,
    uid: &str,
    session: &Session,
) -> Result<(), String> {
    match &rule.rate_limit {
        Some(rate_limit) => {
            let key = if &rate_limit.limit_type == "sid" {
                Ok(format!("adexRateLimit:{}:{}", hex::encode(channel.id), uid))
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

fn forbidden_referrer(session: &Session) -> bool {
    match session
        .referrer_header
        .as_ref()
        .map(|rf| rf.split('/').nth(2))
        .flatten()
    {
        Some(hostname) => {
            hostname == "localhost"
                || hostname == "127.0.0.1"
                || hostname.starts_with("localhost:")
                || hostname.starts_with("127.0.0.1:")
        }
        None => false,
    }
}

fn forbidden_country(session: &Session) -> bool {
    match session.country.as_ref() {
        Some(country) => country == "XX",
        None => false,
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use chrono::TimeZone;
    use primitives::config::configuration;
    use primitives::event_submission::{RateLimit, Rule};
    use primitives::sentry::Event;
    use primitives::targeting::Rules;
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
                publisher: IDS["publisher2"],
                ad_unit: None,
                ad_slot: None,
                referrer: None,
            })
            .collect()
    }

    fn get_close_events(count: i8) -> Vec<Event> {
        (0..count).map(|_| Event::Close).collect()
    }

    fn get_update_targeting_events(count: i8) -> Vec<Event> {
        (0..count)
            .map(|_| Event::UpdateTargeting {
                targeting_rules: Rules::new(),
            })
            .collect()
    }

    #[tokio::test]
    async fn session_uid_rate_limit() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
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

        let response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &events,
        )
        .await;
        assert_eq!(Ok(()), response);

        let err_response = check_access(
            &&redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &events,
        )
        .await;
        assert_eq!(
            Err(Error::RulesError(
                "rateLimit: too many requests".to_string()
            )),
            err_response
        );
    }

    #[tokio::test]
    async fn ip_rate_limit() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let channel = get_channel(rule);

        let err_response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &get_impression_events(2),
        )
        .await;

        assert_eq!(
            Err(Error::RulesError(
                "rateLimit: only allows 1 event".to_string()
            )),
            err_response
        );

        let response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &get_impression_events(1),
        )
        .await;
        assert_eq!(Ok(()), response);
    }

    #[tokio::test]
    #[ignore]
    async fn check_access_past_channel_valid_until() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let mut channel = get_channel(rule);
        channel.valid_until = Utc.ymd(1970, 1, 1).and_hms(12, 00, 9);

        let err_response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &get_impression_events(2),
        )
        .await;

        assert_eq!(Err(Error::ChannelIsExpired), err_response);
    }

    #[tokio::test]
    #[ignore]
    async fn check_access_close_event_in_withdraw_period() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let mut channel = get_channel(rule);
        channel.spec.withdraw_period_start = Utc
        .ymd(1970, 1, 1)
        .and_hms(12, 0, 9);

        let ok_response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &get_close_events(1),
        )
        .await;

        assert_eq!(Ok(()), ok_response);
    }

    #[tokio::test]
    #[ignore]
    async fn check_access_close_event_and_is_creator() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let mut channel = get_channel(rule);
        channel.creator = IDS["follower"];

        let ok_response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &get_close_events(1),
        )
        .await;

        assert_eq!(Ok(()), ok_response);
    }

    #[tokio::test]
    #[ignore]
    async fn check_access_update_targeting_event_and_is_creator() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let mut channel = get_channel(rule);
        channel.creator = IDS["follower"];

        let ok_response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &get_update_targeting_events(1),
        )
        .await;

        assert_eq!(Ok(()), ok_response);
    }

    #[tokio::test]
    #[ignore]
    async fn not_creator_and_there_are_close_events() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let mut channel = get_channel(rule);
        channel.creator = IDS["leader"];
        let mixed_events = vec![
            Event::Impression {
                publisher: IDS["publisher2"],
                ad_unit: None,
                ad_slot: None,
                referrer: None,
            },
            Event::Close,
            Event::UpdateTargeting {
                targeting_rules: Rules::new(),
            },
        ];
        let err_response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &mixed_events,
        )
        .await;

        assert_eq!(Err(Error::OnlyCreatorCanCloseChannel), err_response);
    }

    #[tokio::test]
    #[ignore]
    async fn not_creator_and_there_are_update_targeting_events() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let mut channel = get_channel(rule);
        channel.creator = IDS["leader"];
        let mixed_events = vec![
            Event::Impression {
                publisher: IDS["publisher2"],
                ad_unit: None,
                ad_slot: None,
                referrer: None,
            },
            Event::UpdateTargeting {
                targeting_rules: Rules::new(),
            },
        ];
        let err_response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &mixed_events,
        )
        .await;

        assert_eq!(Err(Error::OnlyCreatorCanUpdateTargetingRules), err_response);
    }

    #[tokio::test]
    #[ignore]
    async fn in_withdraw_period_no_close_events() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let mut channel = get_channel(rule);
        channel.spec.withdraw_period_start = Utc
            .ymd(1970, 1, 1)
            .and_hms(12, 0, 9);

        let err_response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &get_impression_events(2),
        )
        .await;

        assert_eq!(Err(Error::ChannelIsInWithdrawPeriod), err_response);
    }

    #[tokio::test]
    #[ignore]
    async fn with_forbidden_country() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: Some("XX".into()),
            os: None,
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let channel = get_channel(rule);

        let err_response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &get_impression_events(2),
        )
        .await;

        assert_eq!(Err(Error::ForbiddenReferrer), err_response);
    }

    #[tokio::test]
    #[ignore]
    async fn with_forbidden_referrer() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: Some("http://127.0.0.1".into()),
            country: None,
            os: None,
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let channel = get_channel(rule);

        let err_response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &get_impression_events(2),
        )
        .await;

        assert_eq!(Err(Error::ForbiddenReferrer), err_response);
    }

    #[tokio::test]
    #[ignore]
    async fn no_rate_limit() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let rule = Rule {
            uids: None,
            rate_limit: None,
        };
        let channel = get_channel(rule);

        let ok_response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &get_impression_events(1),
        )
        .await;

        assert_eq!(Ok(()), ok_response);
    }

    #[tokio::test]
    #[ignore]
    async fn applied_rules() {
        let (config, redis) = setup().await;

        let auth = Auth {
            era: 0,
            uid: IDS["follower"],
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(60_000),
            }),
        };
        let channel = get_channel(rule);

        let ok_response = check_access(
            &redis,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &channel,
            &get_impression_events(1),
        )
        .await;

        assert_eq!(Ok(()), ok_response);
        let key = "adexRateLimit:061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088:"
            .to_string();
        let value = "1".to_string();

        let value_in_redis = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, String>(&mut redis.clone())
            .await
            .expect("should exist in redis");
        assert_eq!(&value, &value_in_redis);
    }
}
