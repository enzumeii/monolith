use serde::{Deserialize, Serialize};

use super::AdminRole;

//////////////////////// REQUESTS ////////////////////////
#[derive(Debug, Deserialize, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

//////////////////////// RESPONSES ////////////////////////
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub id: i64,
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
