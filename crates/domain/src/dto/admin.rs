use serde::{Deserialize, Serialize};

#[cfg(feature = "db")]
use crate::entities::Admin;

/// Admin privilege level.
///
/// This is a wire type shared by the API and the web backoffice, so it lives
/// in the `dto` module and is always compiled (no database dependency). When
/// the `db` feature is active it also maps to the Postgres `ADMIN_ROLE` enum
/// via sqlx.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "db", derive(sqlx::Type))]
#[cfg_attr(
    feature = "db",
    sqlx(type_name = "ADMIN_ROLE", rename_all = "SCREAMING_SNAKE_CASE")
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
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

//////////////////////// REQUESTS ////////////////////////
#[derive(Debug, Deserialize, Serialize)]
pub struct CreateAdminRequest {
    pub username: String,
    pub password: String,
    pub role: AdminRole,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct UpdateAdminRequest {
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListAdminsQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct AdminPageRequest {
    pub page: u32,
    pub per_page: u32,
}

//////////////////////// RESPONSES ////////////////////////
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminListResponse {
    pub admins: Vec<AdminDTO>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminDTO {
    pub id: i64,
    pub username: String,
    pub role: AdminRole,
}

#[cfg(feature = "db")]
impl From<Admin> for AdminDTO {
    fn from(admin: Admin) -> Self {
        Self {
            id: admin.id,
            username: admin.username,
            role: admin.role,
        }
    }
}
