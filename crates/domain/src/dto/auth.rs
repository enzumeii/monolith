use serde::{Deserialize, Serialize};

use crate::entities::AdminRole;

//////////////////////// REQUESTS ////////////////////////
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

//////////////////////// RESPONSES ////////////////////////
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    #[serde(rename = "sub")]
    pub admin_id: i64,
    pub username: String,
    pub role: AdminRole,
    #[serde(rename = "exp")]
    pub expiration: u64,
}
