//! `GET /cfg` request

use adapter::client::Locked;
use axum::{Extension, Json};
use hyper::{header::CONTENT_TYPE, Body, Request, Response};

use primitives::Config;

use crate::{response::ResponseError, Application};

/// `GET /cfg` request
pub async fn config_axum<C: Locked + 'static>(
    Extension(app): Extension<Application<C>>,
) -> Json<Config> {
    Json(app.config.clone())
}

/// `GET /cfg` request
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
