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
