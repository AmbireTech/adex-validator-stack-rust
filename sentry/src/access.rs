use chrono::Utc;
use futures::future::try_join_all;
use redis::aio::MultiplexedConnection;

use crate::{Auth, Session};
use primitives::{
    event_submission::{RateLimit, Rule},
    sentry::Event,
    Campaign,
};
use std::cmp::PartialEq;
use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum Error {
    #[error("Campaign is expired")]
    CampaignIsExpired,
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
    campaign: &Campaign,
    events: &[Event],
) -> Result<(), Error> {
    let current_time = Utc::now();

    if current_time > campaign.active.to {
        return Err(Error::CampaignIsExpired);
    }
    let auth_uid = auth.map(|auth| auth.uid.to_string()).unwrap_or_default();

    // Rules for events
    if forbidden_country(session) || forbidden_referrer(session) {
        return Err(Error::ForbiddenReferrer);
    }

    let default_rules = [
        Rule {
            uids: Some(vec![campaign.creator.to_string()]),
            rate_limit: None,
        },
        Rule {
            uids: None,
            rate_limit: Some(rate_limit.clone()),
        },
    ];

    // Enforce access limits
    let allow_rules = campaign
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
            .map(|rule| apply_rule(redis.clone(), rule, events, campaign, &auth_uid, session)),
    );

    apply_all_rules.await.map_err(Error::RulesError).map(|_| ())
}

async fn apply_rule(
    redis: MultiplexedConnection,
    rule: &Rule,
    events: &[Event],
    campaign: &Campaign,
    uid: &str,
    session: &Session,
) -> Result<(), String> {
    match &rule.rate_limit {
        Some(rate_limit) => {
            let key = if &rate_limit.limit_type == "sid" {
                Ok(format!(
                    "adexRateLimit:{}:{}",
                    hex::encode(campaign.id),
                    uid
                ))
            } else if &rate_limit.limit_type == "ip" {
                if events.len() != 1 {
                    Err("rateLimit: only allows 1 event".to_string())
                } else {
                    Ok(format!(
                        "adexRateLimit:{}:{}",
                        hex::encode(campaign.id),
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
    use primitives::{
        config::GANACHE_CONFIG,
        event_submission::{RateLimit, Rule},
        sentry::Event,
        test_util::{DUMMY_CAMPAIGN, DUMMY_IPFS, FOLLOWER, IDS, PUBLISHER_2},
        Config, EventSubmission,
    };

    use deadpool::managed::Object;

    use crate::{
        db::redis_pool::{Manager, TESTS_POOL},
        Session,
    };

    use super::*;

    async fn setup() -> (Config, Object<Manager>) {
        let connection = TESTS_POOL.get().await.expect("Should return Object");
        let config = GANACHE_CONFIG.clone();

        (config, connection)
    }

    fn get_campaign(with_rule: Rule) -> Campaign {
        let mut campaign = DUMMY_CAMPAIGN.clone();

        campaign.event_submission = Some(EventSubmission {
            allow: vec![with_rule],
        });

        campaign
    }

    fn get_impression_events(count: i8) -> Vec<Event> {
        (0..count)
            .map(|_| Event::Impression {
                publisher: *PUBLISHER_2,
                ad_unit: DUMMY_IPFS[0],
                ad_slot: DUMMY_IPFS[1],
                referrer: None,
            })
            .collect()
    }

    #[tokio::test]
    async fn session_uid_rate_limit() {
        let (config, database) = setup().await;

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "sid".to_string(),
                time_frame: Duration::from_millis(20_000),
            }),
        };
        let campaign = get_campaign(rule);

        let chain_context = config
            .find_chain_of(campaign.channel.token)
            .expect("Campaign's Channel.token should be set in config");

        let auth = Auth {
            era: 0,
            uid: IDS[&FOLLOWER],
            chain: chain_context.chain.clone(),
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let events = get_impression_events(2);

        let response = check_access(
            &database,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &campaign,
            &events,
        )
        .await;
        assert_eq!(Ok(()), response);

        let err_response = check_access(
            &database,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &campaign,
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
        let (config, database) = setup().await;

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };

        let campaign = get_campaign(rule);

        let chain_context = config
            .find_chain_of(campaign.channel.token)
            .expect("Campaign's Channel.token should be set in config");

        let auth = Auth {
            era: 0,
            uid: IDS[&FOLLOWER],
            chain: chain_context.chain.clone(),
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let err_response = check_access(
            &database,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &campaign,
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
            &database,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &campaign,
            &get_impression_events(1),
        )
        .await;
        assert_eq!(Ok(()), response);
    }

    #[tokio::test]
    async fn check_access_past_channel_valid_until() {
        let (config, database) = setup().await;

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let mut campaign = get_campaign(rule);
        campaign.active.to = Utc.ymd(1970, 1, 1).and_hms(12, 00, 9);

        let chain_context = config
            .find_chain_of(campaign.channel.token)
            .expect("Campaign's Channel.token should be set in config");

        let auth = Auth {
            era: 0,
            uid: IDS[&FOLLOWER],
            chain: chain_context.chain.clone(),
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let err_response = check_access(
            &database,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &campaign,
            &get_impression_events(2),
        )
        .await;

        assert_eq!(Err(Error::CampaignIsExpired), err_response);
    }

    #[tokio::test]
    async fn with_forbidden_country() {
        let (config, database) = setup().await;

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let campaign = get_campaign(rule);

        let chain_context = config
            .find_chain_of(campaign.channel.token)
            .expect("Campaign's Channel.token should be set in config");

        let auth = Auth {
            era: 0,
            uid: IDS[&FOLLOWER],
            chain: chain_context.chain.clone(),
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: Some("XX".into()),
            os: None,
        };

        let err_response = check_access(
            &database,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &campaign,
            &get_impression_events(2),
        )
        .await;

        assert_eq!(Err(Error::ForbiddenReferrer), err_response);
    }

    #[tokio::test]
    async fn with_forbidden_referrer() {
        let (config, database) = setup().await;

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(1),
            }),
        };
        let campaign = get_campaign(rule);

        let chain_context = config
            .find_chain_of(campaign.channel.token)
            .expect("Campaign's Channel.token should be set in config");

        let auth = Auth {
            era: 0,
            uid: IDS[&FOLLOWER],
            chain: chain_context.chain.clone(),
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: Some("http://127.0.0.1".into()),
            country: None,
            os: None,
        };

        let err_response = check_access(
            &database,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &campaign,
            &get_impression_events(2),
        )
        .await;

        assert_eq!(Err(Error::ForbiddenReferrer), err_response);
    }

    #[tokio::test]
    async fn no_rate_limit() {
        let (config, database) = setup().await;

        let rule = Rule {
            uids: None,
            rate_limit: None,
        };
        let campaign = get_campaign(rule);

        let chain_context = config
            .find_chain_of(campaign.channel.token)
            .expect("Campaign's Channel.token should be set in config");

        let auth = Auth {
            era: 0,
            uid: IDS[&FOLLOWER],
            chain: chain_context.chain.clone(),
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let ok_response = check_access(
            &database,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &campaign,
            &get_impression_events(1),
        )
        .await;

        assert_eq!(Ok(()), ok_response);
    }

    #[tokio::test]
    async fn applied_rules() {
        let (config, mut database) = setup().await;

        let rule = Rule {
            uids: None,
            rate_limit: Some(RateLimit {
                limit_type: "ip".to_string(),
                time_frame: Duration::from_millis(60_000),
            }),
        };
        let campaign = get_campaign(rule);

        let chain_context = config
            .find_chain_of(campaign.channel.token)
            .expect("Campaign's Channel.token should be set in config");

        let auth = Auth {
            era: 0,
            uid: IDS[&FOLLOWER],
            chain: chain_context.chain.clone(),
        };

        let session = Session {
            ip: Default::default(),
            referrer_header: None,
            country: None,
            os: None,
        };

        let ok_response = check_access(
            &database,
            &session,
            Some(&auth),
            &config.ip_rate_limit,
            &campaign,
            &get_impression_events(1),
        )
        .await;

        assert_eq!(Ok(()), ok_response);
        let key = "adexRateLimit:936da01f9abd4d9d80c702af85c822a8:".to_string();
        let value = "1".to_string();

        let value_in_redis = redis::cmd("GET")
            .arg(&key)
            // Deref can't work here, so we need to call the `Object` -> `Database.connection`
            .query_async::<_, String>(&mut database.connection)
            .await
            .expect("should exist in redis");
        assert_eq!(&value, &value_in_redis);
    }
}
