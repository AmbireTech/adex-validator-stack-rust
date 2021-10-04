# Sentry REST API
## For AdEx Protocol V5

### Channel

#### `GET /v5/channel/list`

#### `GET /v5/channel/:id/accounting` (auth required)

#### `GET /v5/channel/:id/spender/:addr` (auth required)

#### `GET /v5/channel/:id/spender/all` (auth required)

#### `POST /v5/channel/:id/spender/:addr` (auth required)

#### `GET /v5/channel/:id/validator-messages`


#### `POST /v5/channel/:id/validator-messages` (auth required)


#### `POST /v5/channel/:id/pay` (auth required)
Channel Payout with authentication of the spender

Withdrawals of advertiser funds - re-introduces the PAY event with a separate route.

#### `GET /v5/channel/:id/get-leaf`
This route gets the latest approved state (`NewState`/`ApproveState` pair),
and finds the given `spender`/`earner` in the balances tree, and produce a merkle proof for it.
This is useful for the Platform to verify if a spender leaf really exists.

Query parameters:

- `spender=[0x...]` or `earner=[0x...]` (required)

#### Example Spender:

`/get-leaf?spender=0x...`

#### Example Earner:

`/get-leaf?earner=0x....`


### Campaign

#### `GET /v5/campaign/list`

Lists all campaigns with pagination and orders them in descending order (`DESC`) by `Campaign.created`. This ensures that the order in the pages will not change if a new `Campaign` is created while still retrieving a page.

Query parameters:
- `page=[integer]` (optional) default: `0`
- `creator=[0x....]` (optional) - address of the creator to be filtered by
- `activeTo=[integer]` (optional) in seconds - filters campaigns by `Campaign.active.to > query.activeTo`
- `validator=[0x...]` or `leader=[0x...]` (optional) - address of the validator to be filtered by. You can either 
  - `validator=[0x...]` - it will return all `Campaign`s where this address is **either** `Channel.leader` or `Channel.follower`
  - `leader=[0x...]` - it will return all `Campaign`s where this address is `Channel.leader`


#### `POST /v5/campaign` (auth required)
Create a new Campaign.

It will make sure the `Channel` is created if new and it will update the spendable amount using the `Adapter::get_deposit()`.

Authentication: **required** to validate `Campaign.creator == Auth.uid`

Request Body: `CreateCampaign` (json)

#### `POST /v5/campaign/:id/close` (auth required)

### Analytics

#### `GET /v5/analytics/for-publisher` (auth required)
todo

#### `GET /v5/analytics/for-advertiser` (auth required)
todo

#### `GET /v5/analytics/for-admin` (auth required)
todo
