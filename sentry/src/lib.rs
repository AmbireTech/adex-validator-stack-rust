#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[doc(inline)]
pub use application::{Application, Auth, Session};

pub mod access;
pub mod analytics;
pub mod application;
pub mod db;
pub mod middleware;
pub mod payout;
pub mod platform;
pub mod response;
pub mod routes;
pub mod spender;

#[cfg(test)]
pub mod test_util;
