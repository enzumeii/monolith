use api::{config::Settings, repositories, routes, state::app_state};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let settings = Settings::new().map_err(|e| {
        tracing::error!(error = %e, "Failed to load application configuration");
        e
    })?;

    let pool = repositories::create_pool(&settings).await?;
    tracing::info!("Database connection pool established successfully");

    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    let listener = TcpListener::bind(&addr).await.map_err(|e| {
        tracing::error!(error = %e,"Failed to initiate TCP Connection to http://{}", addr);
        e
    })?;

    let state = app_state(pool, settings);
    let router = routes::router(state);

    tracing::info!("Listening on http://{}", addr);
    axum::serve(listener, router).await.map_err(|e| {
        tracing::error!(error = %e,"Failed to start server on http://{}", addr);
        e
    })?;

    Ok(())
}
