//! Sentry REST API documentation
//!
//!
//! All routes are listed below. Here is an overview and links to all of them:
//! - [Channel](#channel) routes
//!   - [GET `/v5/channel/list`](#get-v5channellist)
//!   - [GET `/v5/channel/:id/accounting`](#get-v5channelidaccounting)
//!   - [GET `/v5/channel/:id/spender/:addr`](#get-v5channelidspenderaddr-auth-required) (auth required)
//!   - [POST `/v5/channel/:id/spender/:addr`](#post-v5channelidspenderaddr-auth-required) (auth required)
//!   - [GET `/v5/channel/:id/spender/all`](#get-v5channelidspenderall-auth-required) (auth required)
//!   - [GET `/v5/channel/:id/validator-messages`](#get-v5channelidvalidator-messages)
//!   - [GET `/v5/channel/:id/validator-messages/:addr`](#get-v5channelidvalidator-messages)
//!   - [GET `/v5/channel/:id/validator-messages/:addr/:validator_messages`](#get-v5channelidvalidator-messages)
//!   - [POST `/v5/channel/:id/validator-messages`](#post-v5channelidvalidator-messages-auth-required) (auth required)
//!   - [GET `/v5/channel/:id/last-approved`](#get-v5channelidlast-approved)
//!   - [POST `/v5/channel/:id/pay`](#post-v5channelidpay-auth-required) (auth required)
//!   - [GET `/v5/channel/:id/get-leaf`](#get-v5channelidget-leaf)
//! - [Campaign](#campaign) routes
//!     - [GET `/v5/campaign/list`](#get-v5campaignlist)
//!     - [POST `/v5/campaign`](#post-v5campaign-auth-required) (auth required)
//!     - [POST `/v5/campaign/:id`](#post-v5campaignid-auth-required) (auth required)
//!     - [POST `/v5/campaign/:id/events`](#post-v5campaignidevents) (auth required)
//!     - [POST `/v5/campaign/:id/close`](#post-v5campaignidclose-auth-required) (auth required)
//! - [Analytics](#analytics) routes
//!   - [GET `/v5/analytics`](#get-v5analytics)
//!   - [GET `/v5/analytics/for-publisher`](#get-v5analyticsfor-publisher-auth-required) (auth required)
//!   - [GET `/v5/analytics/for-advertiser`](#get-v5analyticsfor-advertiser-auth-required) (auth required)
//!   - [GET `/v5/analytics/for-admin`](#get-v5analyticsfor-admin-auth-required) (auth required)
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
//! #### GET `/v5/channel/list`
//!
//! The route is handled by [`channel::channel_list()`].
//!
//! Request query parameters: [`ChannelListQuery`](primitives::sentry::channel_list::ChannelListQuery)
//!
//! Response: [`ChannelListResponse`](primitives::sentry::channel_list::ChannelListResponse)
//!
//! ##### Examples
//!
//! Query:
//!
//! ```
#![doc = include_str!("../../primitives/examples/channel_list_query.rs")]
//! ```
//!
//! #### GET `/v5/channel/:id/accounting`
//!
//! The route is handled by [`channel::get_accounting_for_channel()`].
//!
//! Response: [`AccountingResponse::<CheckedState>`](primitives::sentry::AccountingResponse)
//!
//! #### GET `/v5/channel/:id/spender/:addr` (auth required)
//!
//! The route is handled by [`channel::get_spender_limits()`].
//!
//! Response: [`SpenderResponse`](primitives::sentry::SpenderResponse)
//!
//! #### POST `/v5/channel/:id/spender/:addr` (auth required)
//!
//! This route forces the addition of a spender [`Accounting`]
//! (if one does not exist) to the given [`Channel`] with `spent = 0`.
//! This will also ensure that the spender is added to the [`NewState`] as well.
//!
//! The route is handled by [`channel::add_spender_leaf()`].
//!
//! Response: [`SuccessResponse`]
//!
//! #### GET `/v5/channel/:id/spender/all` (auth required)
//!
//! The route is handled by [`channel::get_all_spender_limits()`].
//!
//! Response: [`AllSpendersResponse`](primitives::sentry::AllSpendersResponse)
//!
//! #### GET `/v5/channel/:id/validator-messages`
//!
//! - GET `/v5/channel/:id/validator-messages/:addr` - filter by the given [`ValidatorId`]
//! - GET `/v5/channel/:id/validator-messages/:addr/:validator_messages` - filters by the given [`ValidatorId`] and a
//!   [`Validator message types`][`MessageTypes`].
//!    - `:validator_messages` - url encoded list of [`Validator message types`][`MessageTypes`] separated by a `+`.
//!
//!       E.g. `NewState+ApproveState` becomes `NewState%2BApproveState`
//!
//! The route is handled by [`channel::validator_message::list_validator_messages()`].
//!
//! Request query parameters: [ValidatorMessagesListQuery](primitives::sentry::ValidatorMessagesListQuery)
//!
//! Response: [ValidatorMessagesListResponse](primitives::sentry::ValidatorMessagesListResponse)
//!
//! #### POST `/v5/channel/:id/validator-messages` (auth required)
//!
//! The route is handled by [`channel::validator_message::create_validator_messages()`].
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
//! #### GET `/v5/channel/:id/last-approved`
//!
//! The route is handled by [`channel::last_approved()`].
//!
//! Request query parameters: [`LastApprovedQuery`][primitives::sentry::LastApprovedQuery]
//!
//! Response: [`LastApprovedResponse`][primitives::sentry::LastApprovedResponse]
//!
//! #### POST `/v5/channel/:id/pay` (auth required)
//!
//! Channel Payout with authentication of the spender.
//!
//! This route handles withdrawals of advertiser funds for the authenticated spender.
//! It needs to ensure all campaigns are closed. It accepts a JSON body in the request which contains
//! all of the earners and updates their balances accordingly. Used when an advertiser/spender wants
//! to get their remaining funds back.
//!
//! The route is handled by [`channel::channel_payout()`].
//!
//! Request JSON body: [`ChannelPayRequest`](primitives::sentry::ChannelPayRequest)
//!
//! Response: [`SuccessResponse`](primitives::sentry::SuccessResponse)
//!
//!
//! #### GET `/v5/channel/:id/get-leaf`
//!
//! TODO: implement and document as part of issue #382
//!
//! This route gets the latest approved state ([`NewState`]/[`ApproveState`] pair),
//! and finds the given `spender`/`earner` in the balances tree, and produce a merkle proof for it.
//! This is useful for the Platform to verify if a spender leaf really exists.
//!
//! The route is handled by `todo`.
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
//! #### GET `/v5/campaign/list`
//!
//! Lists all campaigns with pagination and orders them in
//! ascending order (`ASC`) by `Campaign.created`.
//! This ensures that the order in the pages will not change if a new
//! `Campaign` is created while still retrieving a page.
//!
//! The route is handled by [`campaign::campaign_list()`].
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
//!
//! Response: [`CampaignListResponse`][primitives::sentry::campaign_list::CampaignListResponse]
//!
//! ##### Examples
//!
//! Query:
//!
//! ```
#![doc = include_str!("../../primitives/examples/campaign_list_query.rs")]
//! ```
//! 
//! Response:
//! 
//! ```
#![doc = include_str!("../../primitives/examples/campaign_list_response.rs")]
//! ```
//!
//! #### POST `/v5/campaign` (auth required)
//!
//! Create a new Campaign. Request must be sent by the [`Campaign.creator`](primitives::Campaign::creator).
//!
//! **Authentication is required** to validate [`Campaign.creator`](primitives::Campaign::creator) == [`Auth.uid`](crate::Auth::uid)
//!
//! It will make sure the `Channel` is created if new and it will update
//! the spendable amount using the [`Adapter`]`::get_deposit()`.
//!
//! The route is handled by [`campaign::create_campaign()`].
//!
//! Request body (json): [`CreateCampaign`][primitives::sentry::campaign_create::CreateCampaign]
//!
//! Response: [`Campaign`]
//!
//! #### POST `/v5/campaign/:id` (auth required)
//!
//! Modify the [`Campaign`]. Request must be sent by the [`Campaign.creator`](primitives::Campaign::creator).
//!
//! **Authentication is required** to validate [`Campaign.creator`](primitives::Campaign::creator) == [`Auth.uid`](crate::Auth::uid)
//!
//! The route is handled by [`campaign::update_campaign::handle_route()`].
//!
//! Request body (json): [`ModifyCampaign`][primitives::sentry::campaign_modify::ModifyCampaign]
//!
//! Response: [`Campaign`]
//!
//! #### POST `/v5/campaign/:id/events`
//!
//! Add new [`Event`]s (`IMPRESSION`s & `CLICK`s) to the [`Campaign`].
//! Applies [`Campaign.event_submission`] rules and additional validation using [`check_access()`].
//!
//! The route is handled by [`campaign::insert_events::handle_route()`].
//!
//! Request body (json):
//!
//! ```json
//! {
//!     "events": [
//!         // Events
//!    ]
//! }
//! ```
//!
//! Response: [`SuccessResponse`]
//!
//! #### POST `/v5/campaign/:id/close` (auth required)
//!
//! Close the campaign.
//!
//! The route is handled by [`campaign::close_campaign()`].
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
//! #### GET `/v5/analytics`
//!
//! Allowed keys: [`AllowedKey::Country`][primitives::analytics::query::AllowedKey::Country], [`AllowedKey::AdSlotType`][primitives::analytics::query::AllowedKey::AdSlotType]
//!
//! #### GET `/v5/analytics/for-publisher` (auth required)
//!
//! Returns all analytics where the currently authenticated address [`Auth.uid`](crate::Auth::uid) is a **publisher**.
//!
//! All [`ALLOWED_KEYS`] are allowed for this route.
//!
//! The route is handled by [`get_analytics()`].
//!
//! #### GET `/v5/analytics/for-advertiser` (auth required)
//!
//! Returns all analytics where the currently authenticated address [`Auth.uid`](crate::Auth::uid) is an **advertiser**.
//!
//! All [`ALLOWED_KEYS`] are allowed for this route.
//!
//! The route is handled by [`get_analytics()`].
//!
//! #### GET `/v5/analytics/for-admin` (auth required)
//!
//! Admin access to the analytics with no restrictions on the keys for filtering.
//!
//! All [`ALLOWED_KEYS`] are allowed for admins.
//!
//! Admin addresses are configured in the [`Config.admins`](primitives::Config::admins).
//!
//! The route is handled by [`get_analytics()`].
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
//! [`Event`]: primitives::sentry::Event
//! [`Campaign.event_submission`]: primitives::Campaign::event_submission
//! [`check_access()`]: crate::access::check_access
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

mod units_for_slot;
