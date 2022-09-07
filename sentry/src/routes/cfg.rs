//! `GET /cfg` request

use std::sync::Arc;

use axum::{Extension, Json};

use adapter::client::Locked;
use primitives::Config;

use crate::Application;

/// GET `/cfg` request
///
/// Response: [`Config`]
pub async fn get_cfg<C: Locked + 'static>(
    Extension(app): Extension<Arc<Application<C>>>,
) -> Json<Config> {
    Json(app.config.clone())
}
