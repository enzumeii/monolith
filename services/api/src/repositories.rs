pub(crate) mod admin;
pub(crate) mod auth;

use crate::config::Settings;
use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn create_pool(settings: &Settings) -> Result<PgPool, crate::error::AppError> {
    tracing::info!(
        max_connections = settings.database.max_connections,
        min_connections = settings.database.min_connections,
        "Initializing PostgreSQL connection pool..."
    );

    let pool = PgPoolOptions::new()
        .max_connections(settings.database.max_connections)
        .min_connections(settings.database.min_connections)
        .connect(&settings.database.url)
        .await
        .map_err(|error| crate::error::AppError::from(error).context("connect database"))?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|error| crate::error::AppError::from(error).context("run database migrations"))?;

    tracing::info!("Database migrations ran successfully");

    Ok(pool)
}
