use chrono::{DateTime, Utc};
use sqlx::FromRow;

use crate::dto::AdminRole;

#[derive(Debug, FromRow)]
pub struct Admin {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub role: AdminRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
