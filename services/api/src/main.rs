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
        .event_format(api::logging::SourceFormat)
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
    let scheduler = state
        .settings
        .scheduler
        .enabled
        .then(|| tokio::spawn(api::run_scheduler(state.clone())));
    let router = routes::router(state);
    tracing::info!(%address,"API listening");
    let result = axum::serve(listener, router)
        .with_graceful_shutdown(async {
            if let Err(failure) = shutdown_signal().await {
                error::log_background_error("shutdown signal", &AppError::from(failure));
            }
        })
        .await;
    maintenance.abort();
    let _ = maintenance.await;
    if let Some(scheduler) = scheduler {
        scheduler.abort();
        let _ = scheduler.await;
    }
    if result.is_ok() {
        pool.close().await;
    }
    result.map_err(|error| AppError::from(error).context("serve API"))?;
    Ok(())
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
