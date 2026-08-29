mod config;
mod error;
mod handlers;
mod middleware;
mod repositories;
mod routes;
mod services;
mod state;

use config::Settings;
use state::AppState;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Logger Setup
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    // Configuration file reader
    let settings = Settings::new().map_err(|e| {
        tracing::error!(error = %e, "Failed to load application configuration");
        e
    })?;

    // Database Connection
    let pool = repositories::create_pool(&settings).await?;
    tracing::info!("Database connection pool established successfully");

    // Webserver resources
    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    let listener = TcpListener::bind(&addr).await.map_err(|e| {
        tracing::error!(error = %e,"Failed to initiate TCP Connection to http://{}", addr);
        e
    })?;

    // Dependency Injection
    let state = AppState::bind(&pool, settings);

    // Webserver builder
    let router = routes::router(state);
    axum::serve(listener, router).await.map_err(|e| {
        tracing::error!(error = %e,"Failed to start server on http://{}", addr);
        e
    })?;

    tracing::info!("Listening on http://{}", addr);

    Ok(())
}
