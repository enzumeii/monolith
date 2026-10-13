use domain::dto::AdminRole;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    #[serde(rename = "sub")]
    pub admin_id: i64,
    pub username: String,
    pub role: AdminRole,
    #[serde(rename = "exp")]
    pub expiration: u64,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedAdmin {
    pub id: i64,
    pub role: AdminRole,
}
impl From<&crate::models::Admin> for AuthenticatedAdmin {
    fn from(account: &crate::models::Admin) -> Self {
        Self {
            id: account.id,
            role: account.role.into(),
        }
    }
}
