use std::{sync::Arc, net::SocketAddr};

use axum::{routing::get, Extension, Router, Server};
use log::info;

use tera::Tera;

use crate::routes::{get_preview_ad, get_index, get_preview_video};

#[derive(Debug)]
pub struct State {
    pub tera: Tera,
}

pub struct Application {
    /// The shared state of the application
    state: Arc<State>,
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
        // build our application with a single route
        let app = Router::new()
            .route("/", get(get_index))
            .route("/preview/ad", get(get_preview_ad))
            .route("/preview/video", get(get_preview_video))
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
