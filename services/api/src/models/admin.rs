use sqlx::FromRow;

use domain::dto::AdminRole;

#[derive(Debug, Clone, Copy)]
pub struct AdminPageRequest {
    pub(crate) page: u32,
    pub(crate) per_page: u32,
}

impl AdminPageRequest {
    #[must_use]
    pub fn page(self) -> u32 {
        self.page
    }
    #[must_use]
    pub fn per_page(self) -> u32 {
        self.per_page
    }
}

#[derive(FromRow)]
pub struct Admin {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub role: DbAdminRole,
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
