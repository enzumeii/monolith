use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::admin::DbAdminRole;

#[derive(Debug, FromRow)]
pub struct TokenRefreshRecord {
    pub token_id: Uuid,
    pub admin_id: i64,
    pub username: String,
    pub role: DbAdminRole,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}
