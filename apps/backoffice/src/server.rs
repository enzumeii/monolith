#![cfg(feature = "server")]

use dioxus::prelude::*;
use dioxus_server::axum::{
    Extension, Router,
    extract::{Request, State},
    http::{
        StatusCode,
        header::{CACHE_CONTROL, ORIGIN},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
};

use crate::state::AppState;

fn router(state: AppState, app: fn() -> Element) -> Router {
    Router::new()
        .serve_dioxus_application(ServeConfig::new(), app)
        .layer(Extension(state.clone()))
        .layer(middleware::from_fn_with_state(state, request_guard))
}

async fn request_guard(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let api = request.uri().path().starts_with("/api/");
    if api && request.method() != dioxus_server::axum::http::Method::GET {
        let origin = request
            .headers()
            .get(ORIGIN)
            .and_then(|value| value.to_str().ok());
        if origin != Some(state.settings.session.public_origin.as_str()) {
            return (
                StatusCode::FORBIDDEN,
                [(CACHE_CONTROL, "no-store")],
                "Untrusted request origin",
            )
                .into_response();
        }
    }
    let mut response = next.run(request).await;
    if api {
        response
            .headers_mut()
            .insert(CACHE_CONTROL, "no-store".parse().expect("static header"));
    }
    response
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
            serve_until(listener, router(state, app)).await?;
            Ok(())
        })
}

#[cfg(test)]
mod tests;

async fn serve_until(listener: tokio::net::TcpListener, router: Router) -> std::io::Result<()> {
    use std::future::IntoFuture;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = dioxus_server::axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = stopped.await;
        })
        .into_future();
    tokio::pin!(server);
    tokio::select! {result=&mut server=>result,result=shutdown_signal()=>{
        result?;let _=stop.send(());tokio::time::timeout(std::time::Duration::from_secs(30),server).await.map_err(|_|std::io::Error::new(std::io::ErrorKind::TimedOut,"backoffice shutdown deadline exceeded"))?
    }}
}
async fn shutdown_signal() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {result=tokio::signal::ctrl_c()=>result,_=terminate.recv()=>Ok(())}
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await
    }
}
