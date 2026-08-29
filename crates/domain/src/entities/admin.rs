use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "ADMIN_ROLE", rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Default)]
pub enum AdminRole {
    SuperAdmin,
    #[default]
    Admin,
}

impl std::fmt::Display for AdminRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdminRole::SuperAdmin => write!(f, "SUPER_ADMIN"),
            AdminRole::Admin => write!(f, "ADMIN"),
        }
    }
}

#[derive(Debug, FromRow)]
pub struct Admin {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub role: AdminRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
