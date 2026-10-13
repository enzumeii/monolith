use api::{
    config::Settings,
    error::{self, AppError},
    repositories, routes,
    state::app_state,
};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(failure) => {
            error::log_background_error("API startup/runtime", &failure);
            std::process::ExitCode::FAILURE
        }
    }
}
async fn run() -> Result<(), AppError> {
    let settings =
        Settings::new().map_err(|error| AppError::from(error).context("load API configuration"))?;
    let pool = repositories::create_pool(&settings).await?;
    tracing::info!("Database connection pool established");
    let address = format!("{}:{}", settings.server.host, settings.server.port);
    let listener = TcpListener::bind(&address)
        .await
        .map_err(|error| AppError::from(error).context("bind API listener"))?;
    let state = app_state(pool.clone(), settings)?;
    let maintenance_pool = pool.clone();
    let maintenance = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Err(failure) = api::cleanup_refresh_tokens(&maintenance_pool).await {
                error::log_background_error("refresh token retention", &failure);
            }
        }
    });
    let router = routes::router(state);
    tracing::info!(%address,"API listening");
    let result = api::runtime::serve_until(
        listener,
        router,
        api::runtime::shutdown_signal(),
        std::time::Duration::from_secs(30),
    )
    .await;
    maintenance.abort();
    let _ = maintenance.await;
    if result.is_ok() {
        pool.close().await;
    }
    result.map_err(|error| AppError::from(error).context("serve API"))?;
    Ok(())
}
