use crate::models::Admin;
use domain::dto::AdminDTO;

pub(crate) fn response(admin: Admin) -> AdminDTO {
    AdminDTO {
        id: admin.id,
        username: admin.username,
        role: admin.role.into(),
    }
}
