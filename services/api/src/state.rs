use crate::config::Settings;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub settings: Arc<Settings>,
}

#[must_use]
pub fn app_state(pool: PgPool, settings: Settings) -> AppState {
    AppState {
        pool,
        settings: Arc::new(settings),
    }
}
