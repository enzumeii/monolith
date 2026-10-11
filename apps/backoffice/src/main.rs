//! Authed admin backoffice: login, admin management (super admin), profile.
//! Dioxus fullstack (web FE + axum server). The server proxies to the internal
//! `services/api` over HTTP using the base URL from `application.toml`
//! (overridable via env for Kubernetes). It never talks to the DB directly.

mod api;
mod components;
mod config;
mod route;
mod session;

use dioxus::prelude::*;

use crate::components::Login;
use crate::route::Route;
use crate::session::SESSION;

#[cfg(feature = "server")]
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Fail fast on missing/invalid configuration before binding.
    let settings = config::settings();
    tracing::info!(upstream = %settings.upstream.api_base, "backoffice upstream configured");

    // The Dioxus CLI sets `IP`/`PORT` when running under `dx serve`; those (or
    // manually-set env vars) override the configured bind address, which
    // otherwise comes from `application.toml` (`[server]`).
    let host = dioxus::cli_config::server_ip()
        .map_or_else(|| settings.server.host.clone(), |ip| ip.to_string());
    let port = dioxus::cli_config::server_port().unwrap_or(settings.server.port);
    let addr = format!("{host}:{port}");

    let router = dioxus_server::axum::Router::new()
        .serve_dioxus_application(dioxus_server::ServeConfig::new(), App)
        .into_make_service();
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind backoffice server");
    tracing::info!(%addr, "backoffice listening");
    dioxus_server::axum::serve(listener, router)
        .await
        .expect("backoffice server failed");
}

#[cfg(not(feature = "server"))]
fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Stylesheet { href: asset!("/assets/main.css") }
        if SESSION.read().is_none() {
            Login {}
        } else {
            Router::<Route> {}
        }
    }
}
