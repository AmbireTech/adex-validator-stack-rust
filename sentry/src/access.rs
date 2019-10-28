use chrono::Utc;
use futures::compat::Future01CompatExt;
use futures::future::try_join_all;
use redis::aio::SharedConnection;

use primitives::event_submission::{RateLimit, Rule};
use primitives::sentry::Event;
use primitives::Channel;

use crate::Session;

#[derive(Debug, PartialEq, Eq)]
pub enum Response {
    OnlyCreatorCanCloseChannel,
    ChannelIsExpired,
    ChannelIsPastWithdrawPeriod,
    RulesError(String),
    Success,
}

// @TODO: Make pub(crate)
pub async fn check_access(
    redis: &SharedConnection,
    session: &Session,
    rate_limit: &RateLimit,
    channel: &Channel,
    events: &[Event],
) -> Response {
    let is_close_event = |e: &Event| match e {
        Event::Close => true,
        _ => false,
    };

    // Check basic access rules
    // only the creator can send a CLOSE
    if session.uid != channel.creator && events.iter().any(is_close_event) {
        return Response::OnlyCreatorCanCloseChannel;
    }

    let current_time = Utc::now();

    if current_time > channel.valid_until {
        return Response::ChannelIsExpired;
    }

    if current_time > channel.spec.withdraw_period_start && !events.iter().all(is_close_event) {
        return Response::ChannelIsPastWithdrawPeriod;
    }

    let default_rules = [
        Rule {
            uids: Some(vec![channel.creator.clone()]),
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
            Some(uids) => uids.iter().any(|uid| &session.uid == uid),
            None => true,
        })
        .collect::<Vec<_>>();

    if rules.iter().any(|r| r.rate_limit.is_none()) {
        // We matched a rule that has *no rateLimit*, so we're good
        return Response::Success;
    }

    let apply_all_rules = try_join_all(
        rules
            .iter()
            .map(|rule| apply_rule(redis.clone(), &rule, &events, &channel, &session)),
    );

    if let Err(rule_error) = apply_all_rules.await {
        Response::RulesError(rule_error)
    } else {
        Response::Success
    }
}

async fn apply_rule(
    redis: SharedConnection,
    rule: &Rule,
    events: &[Event],
    channel: &Channel,
    session: &Session,
) -> Result<(), String> {
    match &rule.rate_limit {
        Some(rate_limit) => {
            let key = if &rate_limit.limit_type == "sid" {
                // @TODO: Is this really necessary?
                if session.uid.is_empty() {
                    Err("rateLimit: unauthenticated request".to_string())
                } else {
                    Ok(format!("adexRateLimit:{}:{}", channel.id, session.uid))
                }
            } else if &rate_limit.limit_type == "ip" {
                if events.len() != 1 {
                    Err("rateLimit: only allows 1 event".to_string())
                } else {
                    Ok(format!("adexRateLimit:{}:{}", channel.id, session.ip))
                }
            } else {
                // return for the whole function
                return Ok(());
            }?;

            if redis::cmd("EXISTS")
                .arg(&key)
                .query_async::<_, String>(redis.clone())
                .compat()
                .await
                .map(|(_, exists)| exists == "1")
                .map_err(|error| format!("{}", error))?
            {
                return Err("rateLimit: too many requests".to_string());
            }

            let seconds = rate_limit.time_frame.as_secs_f32().ceil();

            redis::cmd("SETEX")
                .arg(&key)
                .arg(seconds)
                .arg("1")
                .query_async::<_, String>(redis)
                .compat()
                .await
                .map(|_| ())
                .map_err(|error| format!("{}", error))
        }
        None => Ok(()),
    }
}
