use chrono::{DateTime, Utc};
use fake::faker::Chrono;
use fake::Faker;
use time::Duration;

/// Creates a DateTime<Utc> between two dates. If `to` is not provided it will use
/// `Now + 365 days`.
///
pub fn datetime_between(from: &DateTime<Utc>, to: Option<&DateTime<Utc>>) -> DateTime<Utc> {
    let default_to = Utc::now() + Duration::days(365);
    let to = to.unwrap_or(&default_to);
    <Faker as Chrono>::between(None, &from.to_rfc3339(), &to.to_rfc3339())
        .parse()
        .expect("Whoops, DateTime<Utc> should be created from Fake...")
}

/// Creates a DateTime<Utc> in the past between `from` and `Now - 1 sec`.
/// If `from` is not provided it will use `-1 week`
///
pub fn past_datetime(from: Option<&DateTime<Utc>>) -> DateTime<Utc> {
    // make sure that we always generate DateTimes in the past, so use `Now - 1 sec`
    let to = Utc::now() - Duration::seconds(1);

    let default_from = Utc::now() - Duration::weeks(1);

    let from = from.unwrap_or(&default_from);

    datetime_between(&from, Some(&to))
}
