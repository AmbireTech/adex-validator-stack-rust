use crate::ResponseError;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Response, Request};
use primitives::adapter::Adapter;
use crate::Application;

pub struct ConfigController<'a, A: Adapter> {
    pub app: &'a Application<A>
}

impl<'a, A: Adapter> ConfigController<'a, A> {

    pub fn new(app: &'a Application<A>) -> Self {
        Self { app }
    }

    pub async fn config(&self, _: Request<Body>) -> Result<Response<Body>, ResponseError> {
        let config_str = serde_json::to_string(&self.app.config)?;
    
        Ok(Response::builder()
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(config_str))
            .expect("Creating a response should never fail"))
    }

}

