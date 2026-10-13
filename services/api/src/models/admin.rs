use chrono::{DateTime, Utc};
use sqlx::FromRow;

use domain::dto::{AdminDTO, AdminRole};

#[derive(Debug, FromRow)]
pub struct Admin {
    pub id: i64,
    pub auth_version: i64,
    pub username: String,
    pub password_hash: String,
    pub role: DbAdminRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "ADMIN_ROLE", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DbAdminRole {
    SuperAdmin,
    Admin,
}

impl From<AdminRole> for DbAdminRole {
    fn from(role: AdminRole) -> Self {
        match role {
            AdminRole::SuperAdmin => Self::SuperAdmin,
            AdminRole::Admin => Self::Admin,
        }
    }
}
impl From<DbAdminRole> for AdminRole {
    fn from(role: DbAdminRole) -> Self {
        match role {
            DbAdminRole::SuperAdmin => Self::SuperAdmin,
            DbAdminRole::Admin => Self::Admin,
        }
    }
}
impl From<Admin> for AdminDTO {
    fn from(admin: Admin) -> Self {
        Self {
            id: admin.id,
            username: admin.username,
            role: admin.role.into(),
        }
    }
}
