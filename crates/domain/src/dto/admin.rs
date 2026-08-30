use serde::{Deserialize, Serialize};

use crate::entities::{Admin, AdminRole};

//////////////////////// REQUESTS ////////////////////////
#[derive(Debug, Deserialize)]
pub struct CreateAdminRequest {
    pub username: String,
    pub password: String,
    pub role: AdminRole,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAdminRequest {
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
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
#[derive(Debug, Serialize)]
pub struct AdminListResponse {
    pub admins: Vec<AdminDTO>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
}

#[derive(Debug, Serialize)]
pub struct AdminDTO {
    pub id: i64,
    pub username: String,
    pub role: AdminRole,
}

impl From<Admin> for AdminDTO {
    fn from(admin: Admin) -> Self {
        Self {
            id: admin.id,
            username: admin.username,
            role: admin.role,
        }
    }
}
