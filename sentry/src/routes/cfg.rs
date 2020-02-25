use crate::Application;
use crate::ResponseError;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Request, Response};
use primitives::adapter::Adapter;

pub async fn config<A: Adapter>(
    _: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let config_str = serde_json::to_string(&app.config)?;

    Ok(Response::builder()
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(config_str))
        .expect("Creating a response should never fail"))
}
