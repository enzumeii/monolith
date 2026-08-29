use serde::Serialize;
use uuid::Uuid;

use crate::entities::Admin;

#[derive(Debug, Serialize)]
pub struct AdminListResponse {
    pub admins: Vec<AdminDTO>,
}

#[derive(Debug, Serialize)]
pub struct AdminDTO {
    pub id: Uuid,
    pub username: String,
}

impl From<Admin> for AdminDTO {
    fn from(admin: Admin) -> Self {
        Self {
            id: admin.id,
            username: admin.username,
        }
    }
}
