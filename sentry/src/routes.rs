//! Sentry REST API documentation
//!
//! ## Channel
//!
//! All routes are implemented under the module [channel].
//!
//! ### Route parameters
//!
//! Paths which include these parameters are validated as follows:
//!
//! - `:id` - [`ChannelId`]
//! - `:addr` - a valid [`Address`] or [`ValidatorId`].
//!
//! ### Routes
//!
//! - [`GET /v5/channel/list`](crate::routes::channel::channel_list)
//!
//! Request query parameters: [`ChannelListQuery`](primitives::sentry::channel_list::ChannelListQuery)
//!
//! Response: [`ChannelListResponse`](primitives::sentry::channel_list::ChannelListResponse)
//!
//! - [`GET /v5/channel/:id/accounting`](channel::get_accounting_for_channel)
//!
//! Response: [`AccountingResponse::<CheckedState>`](primitives::sentry::AccountingResponse)
//!
//! - [`GET /v5/channel/:id/spender/:addr`](channel::get_spender_limits) (auth required)
//!
//! Response: [`SpenderResponse`](primitives::sentry::SpenderResponse)
//!
//! - [`POST /v5/channel/:id/spender/:addr`](channel::add_spender_leaf) (auth required)
//!
//! This route forces the addition of a spender [`Accounting`]
//! (if one does not exist) to the given [`Channel`] with `spent = 0`.
//! This will also ensure that the spender is added to the [`NewState`] as well.
//!
//! Response: [`SuccessResponse`]
//!
//! - [`GET /v5/channel/:id/spender/all`](channel::get_all_spender_limits) (auth required)
//!
//! Response: [`AllSpendersResponse`](primitives::sentry::AllSpendersResponse)
//!
//! - [`GET /v5/channel/:id/validator-messages`][list_validator_messages]
//!
//!   - [`GET /v5/channel/:id/validator-messages/:addr`][list_validator_messages] - filter by the given [`ValidatorId`]
//!   - [`GET /v5/channel/:id/validator-messages/:addr/:validator_messages`][list_validator_messages] - filters by the given [`ValidatorId`] and a
//!     [`Validator message types`][`MessageTypes`].
//!      - `:validator_messages` - url encoded list of [`Validator message types`][`MessageTypes`] separated by a `+`.
//!
//!         E.g. `NewState+ApproveState` becomes `NewState%2BApproveState`
//!
//! Request query parameters: [ValidatorMessagesListQuery](primitives::sentry::ValidatorMessagesListQuery)
//!
//! Response: [ValidatorMessagesListResponse](primitives::sentry::ValidatorMessagesListResponse)
//!
//! [list_validator_messages]: channel::validator_message::list_validator_messages
//!
//! - [`POST /v5/channel/:id/validator-messages`](channel::validator_message::create_validator_messages) (auth required)
//!
//! Request body (json): [`ValidatorMessagesCreateRequest`](primitives::sentry::ValidatorMessagesCreateRequest)
//!
//! Example:
//! ```json
//! {
//!     "messages": [
//!         /// validator messages
//!         ...
//!     ]
//! }
//! ```
//!
//! Validator messages: [`MessageTypes`]
//!
//! - [`POST /v5/channel/:id/last-approved`](channel::last_approved)
//!
//! Request query parameters: [`LastApprovedQuery`][primitives::sentry::LastApprovedQuery]
//!
//! Response: [`LastApprovedResponse`][primitives::sentry::LastApprovedResponse]
//!
//! - [`POST /v5/channel/:id/pay`](channel::channel_payout) (auth required)
//!
//! Channel Payout with authentication of the spender.
//!
//! This route handles withdrawals of advertiser funds for the authenticated spender.
//! It needs to ensure all campaigns are closed. It accepts a JSON body in the request which contains
//! all of the earners and updates their balances accordingly. Used when an advertiser/spender wants
//! to get their remaining funds back.
//!
//! Request JSON body: [`ChannelPayRequest`](primitives::sentry::ChannelPayRequest)
//!
//! Response: [`SuccessResponse`](primitives::sentry::SuccessResponse)
//!
//!
//! - `GET /v5/channel/:id/get-leaf`
//!
//! TODO: implement and document as part of issue #382
//!
//! This route gets the latest approved state ([`NewState`]/[`ApproveState`] pair),
//! and finds the given `spender`/`earner` in the balances tree, and produce a merkle proof for it.
//! This is useful for the Platform to verify if a spender leaf really exists.
//!
//! Request query parameters:
//!
//! - `spender=[0x...]` or `earner=[0x...]` (required)
//!
//! Example Spender:
//!
//! `/get-leaf?spender=0x...`
//!
//! Example Earner:
//!
//! `/get-leaf?earner=0x....`
//! This module includes all routes for `Sentry` and the documentation of each Request/Response.
//!
//! ## Campaign
//!
//! All routes are implemented under the module [campaign].
//!
//! ### Route parameters
//!
//! Paths which include these parameters are validated as follow:
//!
//! - `:id` - [`CampaignId`]
//!
//! ### Routes
//!
//! - [`GET /v5/campaign/list`](campaign::campaign_list)
//!
//! Lists all campaigns with pagination and orders them in
//! descending order (`DESC`) by `Campaign.created`.
//! This ensures that the order in the pages will not change if a new
//! `Campaign` is created while still retrieving a page.
//!
//! Request query parameters: [`CampaignListQuery`][primitives::sentry::campaign_list::CampaignListQuery]
//!
//!   - `page=[integer]` (optional) default: `0`
//!   - `creator=[0x....]` (optional) - address of the creator to be filtered by
//!   - `activeTo=[integer]` (optional) in seconds - filters campaigns by `Campaign.active.to > query.activeTo`
//!   - `validator=[0x...]` or `leader=[0x...]` (optional) - address of the validator to be filtered by. You can either
//!     - `validator=[0x...]` - it will return all `Campaign`s where this address is **either** `Channel.leader` or `Channel.follower`
//!     - `leader=[0x...]` - it will return all `Campaign`s where this address is `Channel.leader`
//!
//! Response: [`CampaignListResponse`][primitives::sentry::campaign_list::CampaignListResponse]
//!
//! - [`POST /v5/campaign`](campaign::create_campaign) (auth required)
//!
//! Create a new Campaign. Request must be sent by the [`Campaign.creator`](primitives::Campaign::creator).
//!
//! **Authentication is required** to validate [`Campaign.creator`](primitives::Campaign::creator) == [`Auth.uid`](crate::Auth::uid)
//!
//! It will make sure the `Channel` is created if new and it will update
//! the spendable amount using the [`Adapter`]`::get_deposit()`.
//!
//!
//! Request body (json): [`CreateCampaign`][primitives::sentry::campaign_create::CreateCampaign]
//!
//! Response: [`Campaign`]
//!
//! - [`POST /v5/campaign/:id`](campaign::update_campaign::handle_route) (auth required)
//!
//! Modify the [`Campaign`]. Request must be sent by the [`Campaign.creator`](primitives::Campaign::creator).
//!
//! **Authentication is required** to validate [`Campaign.creator`](primitives::Campaign::creator) == [`Auth.uid`](crate::Auth::uid)
//!
//! Request body (json): [`ModifyCampaign`][primitives::sentry::campaign_modify::ModifyCampaign]
//!
//! Response: [`Campaign`]
//!
//! - [`POST /v5/campaign/:id/close`](campaign::close_campaign) (auth required)
//!
//! Close the campaign.
//!
//! Request must be sent by the [`Campaign.creator`](primitives::Campaign::creator).
//!
//! **Authentication is required** to validate [`Campaign.creator`](primitives::Campaign::creator) == [`Auth.uid`](crate::Auth::uid)
//!
//! Closes the campaign by setting [`Campaign.budget`](primitives::Campaign::budget) so that `remaining budget = 0`.
//!
//! Response: [`SuccessResponse`]
//!
//! ## Analytics
//!
//! - [`GET /v5/analytics`](get_analytics)
//!
//! Allowed keys: [`AllowedKey::Country`][primitives::analytics::query::AllowedKey::Country] & [`AllowedKey::AdSlotType`][primitives::analytics::query::AllowedKey::AdSlotType]
//!
//! - [`GET /v5/analytics/for-publisher`](get_analytics) (auth required)
//!
//! Returns all analytics where the currently authenticated address [`Auth.uid`](crate::Auth::uid) is a **publisher**.
//!
//! All [`ALLOWED_KEYS`] are allowed for this route.
//!
//!
//! - [`GET /v5/analytics/for-advertiser`](get_analytics) (auth required)
//!
//! Returns all analytics where the currently authenticated address [`Auth.uid`](crate::Auth::uid) is an **advertiser**.
//!
//! All [`ALLOWED_KEYS`] are allowed for this route.
//!
//! - [`GET /v5/analytics/for-admin`](get_analytics) (auth required)
//!
//! Admin access to the analytics with no restrictions on the keys for filtering.
//!
//! All [`ALLOWED_KEYS`] are allowed for admins.
//!
//! Admin addresses are configured in the [`Config.admins`](primitives::Config::admins)
//!
//! [`Adapter`]: adapter::Adapter
//! [`Address`]: primitives::Address
//! [`AllowedKey`]: primitives::analytics::query::AllowedKey
//! [`ALLOWED_KEYS`]: primitives::analytics::query::ALLOWED_KEYS
//! [`ApproveState`]: primitives::validator::ApproveState
//! [`Accounting`]: crate::db::accounting::Accounting
//! [`AccountingResponse`]: primitives::sentry::AccountingResponse
//! [`Campaign`]: primitives::Campaign
//! [`CampaignId`]: primitives::CampaignId
//! [`ChannelId`]: primitives::ChannelId
//! [`Channel`]: primitives::Channel
//! [`MessageTypes`]: primitives::validator::MessageTypes
//! [`NewState`]: primitives::validator::NewState
//! [`SuccessResponse`]: primitives::sentry::SuccessResponse
//! [`ValidatorId`]: primitives::ValidatorId

pub use analytics::analytics as get_analytics;

pub use cfg::config as get_cfg;

// `analytics` module has single request, so we only export this request
mod analytics;
pub mod campaign;
// `cfg` module has single request, so we only export this request
mod cfg;
pub mod channel;

pub mod routers;
