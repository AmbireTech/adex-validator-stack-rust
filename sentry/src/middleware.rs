//! This module contains all the routers' middlewares
//!

use std::fmt::Debug;

use crate::{response::ResponseError, Application};
use adapter::client::Locked;
use hyper::{Body, Request};

use async_trait::async_trait;

#[cfg(test)]
pub use test_util::*;

pub mod auth;
pub mod campaign;
pub mod channel;
pub mod cors;

#[async_trait]
pub trait Middleware<C: Locked + 'static>: Send + Sync + Debug {
    async fn call<'a>(
        &self,
        request: Request<Body>,
        application: &'a Application<C>,
    ) -> Result<Request<Body>, ResponseError>;
}

#[derive(Debug, Default)]
/// `Chain` allows chaining multiple middleware to be applied on the Request of the application
/// Chained middlewares are applied in the order they were chained
pub struct Chain<C: Locked + 'static>(Vec<Box<dyn Middleware<C>>>);

impl<C: Locked + 'static> Chain<C> {
    pub fn new() -> Self {
        Chain(vec![])
    }

    pub fn chain<M: Middleware<C> + 'static>(mut self, middleware: M) -> Self {
        self.0.push(Box::new(middleware));

        self
    }

    /// Applies chained middlewares in the order they were chained
    pub async fn apply(
        &self,
        mut request: Request<Body>,
        application: &Application<C>,
    ) -> Result<Request<Body>, ResponseError> {
        for middleware in self.0.iter() {
            request = middleware.call(request, application).await?;
        }

        Ok(request)
    }
}

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
