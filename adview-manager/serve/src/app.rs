use std::{net::SocketAddr, sync::Arc};

use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Extension, Router, Server,
};
use tracing::{error, info};

use tera::Tera;

use crate::routes::{get_index, get_preview_ad, get_preview_video, post_slot_preview};

#[derive(Debug)]
pub struct State {
    pub tera: Tera,
}

pub struct Application {
    /// The shared state of the application
    state: Arc<State>,
}

pub struct Error {
    error: Box<dyn std::error::Error>,
    status: StatusCode,
}

impl Error {
    pub fn new<E>(error: E, status: StatusCode) -> Self
    where
        E: Into<Box<dyn std::error::Error>>,
    {
        Self {
            error: error.into(),
            status,
        }
    }

    /// Create a new [`Error`] from [`anyhow::Error`] with a custom [`StatusCode`]
    /// instead of the default [`StatusCode::INTERNAL_SERVER_ERROR`].
    pub fn anyhow_status(error: anyhow::Error, status: StatusCode) -> Self {
        Self {
            error: error.into(),
            status,
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let response_tuple = match self.status {
            StatusCode::INTERNAL_SERVER_ERROR => {
                error!({error = %self.error}, "Server error");
                (StatusCode::INTERNAL_SERVER_ERROR, self.error.to_string())
            }
            // we want to log any error that is with status > 500
            status_code if status_code.as_u16() > 500 => {
                error!({error = %self.error}, "Server error");
                (status_code, self.error.to_string())
            }
            // anything else is < 500, so it's safe to not log it due to e.g. bad user input
            status_code => (status_code, self.error.to_string()),
        };

        response_tuple.into_response()
    }
}

impl<E> From<E> for Error
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        let anyhow_err: anyhow::Error = err.into();

        Self {
            error: anyhow_err.into(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl Application {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let serve_dir = match std::env::current_dir().unwrap() {
            serve_path if serve_path.ends_with("serve") => serve_path,
            adview_manager_path if adview_manager_path.ends_with("adview-manager") => {
                adview_manager_path.join("serve")
            }
            // running from the Validator stack workspace
            workspace_path => workspace_path.join("adview-manager/serve"),
        };

        let templates_glob = format!("{}/templates/**/*.html", serve_dir.display());

        info!("Tera templates glob path: {templates_glob}");
        // Use globbing
        let tera = Tera::new(&templates_glob)?;

        let shared_state = Arc::new(State { tera });

        Ok(Self {
            state: shared_state,
        })
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let preview_routes = Router::new()
            .route("/", post(post_slot_preview))
            .route("/ad", get(get_preview_ad))
            .route("/video", get(get_preview_video));

        // build our application with a single route
        let app = Router::new()
            .route("/", get(get_index))
            .nest("/preview", preview_routes)
            .layer(Extension(self.state.clone()));

        let socket_addr: SocketAddr = ([127, 0, 0, 1], 3030).into();
        info!("Server running on: {socket_addr}");

        // run it with hyper on localhost:3030
        Server::bind(&socket_addr)
            .serve(app.into_make_service())
            .await?;

        Ok(())
    }
}
