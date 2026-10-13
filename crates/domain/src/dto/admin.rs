use serde::{Deserialize, Serialize};

/// Admin privilege level in the API wire contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
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
