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
    let command = api::startup::Command::parse(std::env::args().skip(1))?;
    let deployed = command.deployed;
    let settings =
        Settings::new().map_err(|error| AppError::from(error).context("load API configuration"))?;
    let pool = if deployed {
        repositories::connect_pool(&settings).await?
    } else {
        repositories::create_pool(&settings).await?
    };
    tracing::info!("Database connection pool established");
    let port = if deployed {
        std::env::var("PORT")
            .map_or(Ok(80), |port| port.parse::<u16>())
            .map_err(|error| {
                AppError::from(anyhow::Error::new(error)).context("read server PORT")
            })?
    } else {
        settings.server.port
    };
    let address = format!("{}:{port}", settings.server.host);
    let listener = TcpListener::bind(&address)
        .await
        .map_err(|error| AppError::from(error).context("bind API listener"))?;
    let state = app_state(pool.clone(), settings)?;
    let cron = api::CronTasks::start(&state);
    let router = routes::router(state);
    tracing::info!(%address,"API listening");
    let result = axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        if let Err(failure) = shutdown_signal().await {
            error::log_background_error("shutdown signal", &AppError::from(failure));
        }
    })
    .await;
    cron.shutdown().await;
    pool.close().await;
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
