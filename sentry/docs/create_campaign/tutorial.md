In order to call the route successfully you need to:
- Have a valid `CreateCampaign` struct as the request body, which is the same as the `Campaign` struct except that the `id` field is optional. (`CampaignId` is generated on creation)
(TODO: Explain when and why an id will be provided)
- Request must be sent by the `Campaign.creator`. (`Auth.uid == Campaign.creator`)
- The campaign must pass the validations made by `Validator.validate()` which include:
  - Ensuring the `Channel`'s validators include the adapter identity
  - Ensuring the `active.to` field is a valid future date
  - Ensuring the `Campaign.validators`, `Campaign.creator` and `Channel.token` are whitelisted
  - Ensuring the campaign budget is above the minimum campaign budget configured
  - Ensuring the validator fee is greater than the minimum configured fee
- If any of the requirements are not met a `ResponseError` will be returned
- On a successful request your response will be a serialied `Campaign` struct