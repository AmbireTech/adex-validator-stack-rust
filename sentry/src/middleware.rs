use std::fmt::Debug;

use crate::{Application, ResponseError};
use hyper::{Body, Request};
use primitives::adapter::Adapter;

use async_trait::async_trait;

pub mod auth;
pub mod channel;
pub mod cors;

#[async_trait]
pub trait Middleware<A: Adapter + 'static>: Send + Sync + Debug {
    async fn call<'a>(
        &self,
        request: Request<Body>,
        application: &'a Application<A>,
    ) -> Result<Request<Body>, ResponseError>;
}

#[derive(Debug, Default)]
/// `Chain` allows chaining multiple middleware to be applied on the Request of the application
/// Chained middlewares are applied in the order they were chained
pub struct Chain<A: Adapter + 'static>(Vec<Box<dyn Middleware<A>>>);

impl<A: Adapter + 'static> Chain<A> {
    pub fn new() -> Self {
        Chain(vec![])
    }

    pub fn chain<M: Middleware<A> + 'static>(mut self, middleware: M) -> Self {
        self.0.push(Box::new(middleware));

        self
    }

    /// Applies chained middlewares in the order they were chained
    pub async fn apply<'a>(
        &self,
        mut request: Request<Body>,
        application: &'a Application<A>,
    ) -> Result<Request<Body>, ResponseError> {
        for middleware in self.0.iter() {
            request = middleware.call(request, application).await?;
        }

        Ok(request)
    }
}
