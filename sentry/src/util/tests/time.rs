use chrono::{DateTime, Utc};
use fake::faker::Chrono;
use fake::Faker;
use time::Duration;

/// Creates a DatTime<Utc> between two dates. If `to` is not provided it will use
/// `Now + 365 days`.
///
pub fn datetime_between(from: &DateTime<Utc>, to: Option<&DateTime<Utc>>) -> DateTime<Utc> {
    let default_to = Utc::now() + Duration::days(365);
    let to = to.unwrap_or(&default_to);
    <Faker as Chrono>::between(None, &from.to_rfc3339(), &to.to_rfc3339())
        .parse()
        .expect("Whoops, DateTime<Utc> should be created from Fake...")
}
