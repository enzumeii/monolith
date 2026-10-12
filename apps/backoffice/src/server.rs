#![cfg(feature = "server")]

use dioxus::prelude::*;
use dioxus_server::axum::{Extension, Router};

use crate::state::AppState;

fn router(state: AppState, app: fn() -> Element) -> Router {
    Router::new()
        .serve_dioxus_application(ServeConfig::new(), app)
        .layer(Extension(state))
}

pub fn run(state: AppState, app: fn() -> Element) -> Result<(), Box<dyn std::error::Error>> {
    if dioxus::cli_config::is_cli_enabled() {
        dioxus::serve(move || {
            let state = state.clone();
            async move { Ok(router(state, app)) }
        });
    }

    tracing_subscriber::fmt().init();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            let host = state.settings.server.host.clone();
            let port = state.settings.server.port;
            let listener = tokio::net::TcpListener::bind((host.as_str(), port)).await?;
            tracing::info!(%host, port, "backoffice listening");
            dioxus_server::axum::serve(listener, router(state, app)).await?;
            Ok(())
        })
}
