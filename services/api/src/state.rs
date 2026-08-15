use sqlx::PgPool;

use crate::config::Settings;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub settings: Settings,
}
