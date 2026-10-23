pub(crate) mod admin;
pub(crate) mod auth;
pub(crate) mod cron_configurations;
pub(crate) mod cron_jobs;
pub(crate) mod report_result_files;
pub(crate) mod report_results;

use crate::config::Settings;
use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn create_pool(settings: &Settings) -> Result<PgPool, crate::error::AppError> {
    let pool = connect_pool(settings).await?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|error| crate::error::AppError::from(error).context("run database migrations"))?;

    tracing::info!("Database migrations ran successfully");

    Ok(pool)
}

pub async fn connect_pool(settings: &Settings) -> Result<PgPool, crate::error::AppError> {
    tracing::info!(
        max_connections = settings.database.max_connections,
        min_connections = settings.database.min_connections,
        "Initializing PostgreSQL connection pool..."
    );

    PgPoolOptions::new()
        .max_connections(settings.database.max_connections)
        .min_connections(settings.database.min_connections)
        .connect(&settings.database.url)
        .await
        .map_err(|error| crate::error::AppError::from(error).context("connect database"))
}

pub(crate) mod conventions;
