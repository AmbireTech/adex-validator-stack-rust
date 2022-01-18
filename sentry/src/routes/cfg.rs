//! GET "/cfg" request module

use crate::Application;
use crate::ResponseError;
use adapter::client::Locked;
use hyper::header::CONTENT_TYPE;
use hyper::{Body, Request, Response};

/// "GET /cfg"
pub async fn config<C: Locked + 'static>(
    _: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let config_str = serde_json::to_string(&app.config)?;

    Ok(Response::builder()
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(config_str))
        .expect("Creating a response should never fail"))
}
