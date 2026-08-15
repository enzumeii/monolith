use crate::config::Settings;
use sqlx::postgres::{PgPool, PgPoolOptions};
use tracing::{error, info};

pub async fn create_pool(settings: &Settings) -> Result<PgPool, sqlx::Error> {
    info!(
        max_connections = settings.database.max_connections,
        min_connections = settings.database.min_connections,
        "Initializing PostgreSQL connection pool..."
    );

    PgPoolOptions::new()
        .max_connections(settings.database.max_connections)
        .min_connections(settings.database.min_connections)
        .connect(&settings.database.url)
        .await
        .map_err(|err| {
            error!(
                error = %err,
                "Failed to connect to PostgreSQL database. Verify host, port, credentials, and network status."
            );
            err
        })
}
