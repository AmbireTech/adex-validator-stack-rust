use chrono::Utc;
use redis::aio::Connection;

use primitives::adapter::Session;
use primitives::event_submission::{RateLimit, Rule};
use primitives::sentry::Event;
use primitives::Channel;

pub enum Response {
    OnlyCreatorCanCloseChannel,
    ChannelIsExpired,
    ChannelIsPastWithdrawPeriod,
    Success,
}

pub async fn check_access(
    _redis: &Connection,
    session: &Session,
    rate_limit: RateLimit,
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
            rate_limit: Some(rate_limit),
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
    let _rules = allow_rules.iter().filter(|r| match &r.uids {
        Some(uids) => uids.iter().any(|uid| &session.uid == uid),
        None => true,
    });

    Response::Success
}
