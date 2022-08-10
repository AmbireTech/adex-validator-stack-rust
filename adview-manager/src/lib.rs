#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use chrono::Duration;
use once_cell::sync::Lazy;

pub use url::Url;

#[doc(inline)]
pub use self::{
    helpers::get_unit_html_with_events,
    manager::{Manager, Options},
};

/// The waiting time before sending out an impression event - **8 seconds** (used as milliseconds in JS).
///
/// Sets the timeout in JS with [setTimeout](https://developer.mozilla.org/en-US/docs/Web/API/setTimeout) function
/// for sending events when showing the Ads.
///
/// Related:
/// - <https://github.com/AdExNetwork/adex-adview-manager/issues/17>
/// - <https://github.com/AdExNetwork/adex-adview-manager/issues/35>
/// - <https://github.com/AdExNetwork/adex-adview-manager/issues/46>
pub static WAIT_FOR_IMPRESSION: Lazy<Duration> = Lazy::new(|| Duration::seconds(8));

/// Impression "stickiness" time - **4 minutes**
///
/// **4 minutes** allows ~4 campaigns to rotate, considering a default frequency cap of 15 minutes
///
/// See <https://github.com/AdExNetwork/adex-adview-manager/issues/65>.
pub static IMPRESSION_STICKINESS_TIME: Lazy<Duration> = Lazy::new(|| Duration::minutes(4));

mod helpers;
pub mod manager;
