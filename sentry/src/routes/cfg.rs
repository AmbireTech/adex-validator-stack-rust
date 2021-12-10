use crate::Application;
use crate::ResponseError;
use adapter::client::UnlockedClient;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Request, Response};

pub async fn config<C: UnlockedClient + 'static>(
    _: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let config_str = serde_json::to_string(&app.config)?;

    Ok(Response::builder()
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(config_str))
        .expect("Creating a response should never fail"))
}
