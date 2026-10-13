use serde::{Deserialize, Serialize};

//////////////////////// REQUESTS ////////////////////////
#[derive(Deserialize, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Serialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

//////////////////////// RESPONSES ////////////////////////
#[derive(Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub id: i64,
}
