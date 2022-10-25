use std::{net::SocketAddr, sync::Arc};

use adex_primitives::{platform::AdSlotResponse, IPFS};
use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router, Server,
};
use dashmap::DashMap;
use tracing::info;

pub type MockedResponses = DashMap<IPFS, AdSlotResponse>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let tracing_subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(tracing_subscriber)
        .expect("setting tracing default failed");

    let slot_responses = Arc::new(MockedResponses::new());

    // build our application with a single router
    let app = Router::new()
        .route("/slot", post(mock_slot_response))
        .route("/slot/:ipfs", get(get_slot))
        .layer(Extension(slot_responses));

    let socket_addr: SocketAddr = ([127, 0, 0, 1], 8004).into();
    info!("Server running on: {socket_addr}");

    Server::bind(&socket_addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn get_slot(
    Extension(responses): Extension<Arc<MockedResponses>>,
    Path(slot): Path<IPFS>,
) -> Response {
    match responses.get(&slot) {
        Some(slot) => (StatusCode::OK, Json(slot.value().clone())).into_response(),
        None => (StatusCode::NOT_FOUND, "Slot not found").into_response(),
    }
}

async fn mock_slot_response(
    Extension(responses): Extension<Arc<MockedResponses>>,
    Json(mocked_response): Json<AdSlotResponse>,
) -> impl IntoResponse {
    responses.insert(mocked_response.slot.ipfs, mocked_response);

    StatusCode::OK
}
