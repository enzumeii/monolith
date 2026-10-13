use dioxus::prelude::*;
use domain::dto::{LoginRequest, RefreshRequest, TokenResponse};

/// POST `/v1/auth/login`.
#[server]
pub async fn login(req: LoginRequest) -> ServerFnResult<TokenResponse> {
    let state = crate::state::app_state();
    let resp = state
        .http
        .post(state.url("/auth/login"))
        .json(&req)
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("login request failed: {e}")))?;
    super::client::decode(resp, "login response").await
}

/// POST `/v1/auth/refresh`. The refresh token is single-use; the response
/// rotates both tokens.
#[server]
pub async fn refresh(req: RefreshRequest) -> ServerFnResult<TokenResponse> {
    let state = crate::state::app_state();
    let resp = state
        .http
        .post(state.url("/auth/refresh"))
        .json(&req)
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("refresh request failed: {e}")))?;
    super::client::decode(resp, "refresh response").await
}

/// GET `/v1/auth/logout`. Best effort — the client clears its session regardless.
#[server]
pub async fn logout(access_token: String) -> ServerFnResult<()> {
    let state = crate::state::app_state();
    let _ = state
        .http
        .post(state.url("/auth/logout"))
        .bearer_auth(&access_token)
        .send()
        .await;
    Ok(())
}
