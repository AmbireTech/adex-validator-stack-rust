use std::fmt::Debug;

use crate::{Application, ResponseError};
use adapter::client::UnlockedClient;
use hyper::{Body, Request};

use async_trait::async_trait;

pub mod auth;
pub mod campaign;
pub mod channel;
pub mod cors;

#[async_trait]
pub trait Middleware<C: UnlockedClient + 'static>: Send + Sync + Debug {
    async fn call<'a>(
        &self,
        request: Request<Body>,
        application: &'a Application<C>,
    ) -> Result<Request<Body>, ResponseError>;
}

#[derive(Debug, Default)]
/// `Chain` allows chaining multiple middleware to be applied on the Request of the application
/// Chained middlewares are applied in the order they were chained
pub struct Chain<C: UnlockedClient + 'static>(Vec<Box<dyn Middleware<C>>>);

impl<C: UnlockedClient + 'static> Chain<C> {
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
