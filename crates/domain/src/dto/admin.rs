use serde::{Deserialize, Serialize};

use crate::entities::{Admin, AdminRole};

//////////////////////// REQUESTS ////////////////////////
#[derive(Debug, Deserialize)]
pub struct CreateAdminRequest {
    pub username: String,
    pub password: String,
    pub role: AdminRole,
}

//////////////////////// RESPONSES ////////////////////////
#[derive(Debug, Serialize)]
pub struct AdminListResponse {
    pub admins: Vec<AdminDTO>,
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
