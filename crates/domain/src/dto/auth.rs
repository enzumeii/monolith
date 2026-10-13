use serde::{Deserialize, Serialize};

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

/// Safe browser-visible account information and access-token expiration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerResponse {
    pub admin: super::AdminDTO,
    pub expires_at: u64,
}
