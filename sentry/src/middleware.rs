//! This module contains all the routers' middlewares
//!

#[cfg(test)]
pub use test_util::*;

pub mod auth;
pub mod campaign;
pub mod channel;

#[cfg(test)]
pub mod test_util {
    use axum::{body::BoxBody, response::Response};

    /// Extracts the body as a String from the Response.
    ///
    /// Used when you want to check the response body or debug a response.
    pub async fn body_to_string(response: Response<BoxBody>) -> String {
        String::from_utf8(hyper::body::to_bytes(response).await.unwrap().to_vec()).unwrap()
    }
}
