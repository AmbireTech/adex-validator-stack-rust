In order to call the route successfully you need to:
- GET `/analytics`:
  - Use a valid `AnalyticsQuery`, the only keys that you can use are `AllowedKey::Country` and `AllowedKey::AdSlotType`
  - If you are using `?segmentBy=...` query parameter it must also be an allowed key, otherwise an error will be returned
- GET `/analytics/for-publisher` (auth required):
  - Returns all analytics where the currently authenticated address `Auth.uid` is a **publisher**.
  - Use a valid `AnalyticsQuery` with no restrictons on the allowed keys that you can use.
- GET `/analytics/for-advertiser`:
  - Returns all analytics where the currently authenticated address `Auth.uid` is a **advertiser**.
  - Use a valid `AnalyticsQuery` with no restrictons on the allowed keys that you can use.
- GET `/analytics/for/admin`:
  - Admin access to the analytics with no restrictions on the keys for filtering.
  - Use a valid `AnalyticsQuery` with no restrictons on the allowed keys that you can use.
- If the request is successful your output will be an array with all the fetched entries which include the time, the value of the metric from the query and the `segment_by` field