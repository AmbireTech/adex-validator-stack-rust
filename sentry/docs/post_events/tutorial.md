In order to call the route successfully you need to:
- You must send an authenticated request with a body which is an array of events in the
  form of `{ events: [ ... ] }`
- The events must pass all the rules in `check_access(...)` that include:
  - The campaign shouldn't be expired
  - The session shouldn't be from a forbidden country or referrer
  - The events should apply all applicable access rules - either the ones provided in `Campaign.event_submission` or the default ones (uid = campaign.creator). The rule uid's should include the `Auth.uid` to make a rule applicable. If a Rule is found which has a `rate_limit` field, then rate limit is applied using `apply_rule(...)`
- Publisher payouts and validator fees are calculated, this step can fail if the campaign has no remaining bduget. If remaining is negative after Redis/Postgres are updated an `EventError` will be thrown
- Successfully paid events will be recorded in analytics (see [`record(...)`](`sentry::analytics::record`))
