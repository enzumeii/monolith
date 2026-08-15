mod config;
mod db;
mod routes;
mod state;

use config::Settings;
use state::AppState;
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let settings = Settings::new().map_err(|e| {
        error!(error = %e, "Failed to load application configuration");
        e
    })?;

    let pool = db::create_pool(&settings).await?;
    info!("Database connection pool established successfully");

    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    let listener = TcpListener::bind(&addr).await.map_err(|e| {
        error!(error = %e,"Failed to initiate TCP Connection to http://{}", addr);
        e
    })?;

    let router = routes::create_router().with_state(AppState {
        db: pool,
        settings: settings,
    });
    axum::serve(listener, router).await.map_err(|e| {
        error!(error = %e,"Failed to start server on http://{}", addr);
        e
    })?;

    info!("Listening on http://{}", addr);

    Ok(())
}
