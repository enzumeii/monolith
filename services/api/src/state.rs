use crate::config::Settings;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub jwt_key: Arc<Vec<u8>>,
    pub settings: Arc<Settings>,
}

pub fn app_state(pool: PgPool, settings: Settings) -> Result<AppState, config::ConfigError> {
    let jwt_key = Arc::new(settings.jwt.key()?);
    Ok(AppState {
        pool,
        jwt_key,
        settings: Arc::new(settings),
    })
}
