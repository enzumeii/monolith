//! Authentication server functions: login, logout, and token refresh.
//!
//! `refresh` is the single place that calls `POST /v1/auth/refresh`; the
//! transparent pre-request refresh (inlined in the admin endpoints) reuses it
//! so the session rotates exactly once per refresh.

use dioxus::prelude::*;

use crate::session::Session;

/// Authenticate against `POST /v1/auth/login`, then resolve the admin's role
/// and username (via `GET /v1/admins/{id}`) to build the client session.
#[server]
pub async fn login(username: String, password: String) -> ServerFnResult<Session> {
    let tokens: domain::dto::TokenResponse = super::client::decode(
        reqwest::Client::new()
            .post(super::client::url("/auth/login"))
            .json(&domain::dto::LoginRequest { username, password })
            .send()
            .await
            .map_err(|e| ServerFnError::new(format!("login request failed: {e}")))?,
        "login response",
    )
    .await?;

    let admin: domain::dto::AdminDTO = super::client::decode(
        reqwest::Client::new()
            .get(super::client::url(&format!("/admins/{}", tokens.id)))
            .bearer_auth(&tokens.access_token)
            .send()
            .await
            .map_err(|e| ServerFnError::new(format!("admin fetch failed: {e}")))?,
        "admin response",
    )
    .await?;

    Ok(Session {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at: super::client::now_unix() + tokens.expires_in,
        admin_id: tokens.id,
        username: admin.username,
        role: admin.role,
    })
}

/// Revoke the session's refresh tokens server-side (`GET /v1/auth/logout`).
/// Best effort: the client clears its session regardless of the outcome.
#[server]
pub async fn logout(session: Session) -> ServerFnResult<()> {
    let _ = reqwest::Client::new()
        .get(super::client::url("/auth/logout"))
        .bearer_auth(&session.access_token)
        .send()
        .await;
    Ok(())
}

/// Rotate the token pair via `POST /v1/auth/refresh`.
///
/// The refresh token is single-use and rotates: the new response's refresh
/// token replaces the old. A rejected refresh is unrecoverable — the client
/// must sign in again.
#[server]
pub async fn refresh(session: Session) -> ServerFnResult<Session> {
    let tokens: domain::dto::TokenResponse = super::client::decode(
        reqwest::Client::new()
            .post(super::client::url("/auth/refresh"))
            .json(&domain::dto::RefreshRequest {
                refresh_token: session.refresh_token.clone(),
            })
            .send()
            .await
            .map_err(|e| ServerFnError::new(format!("refresh request failed: {e}")))?,
        "refresh response",
    )
    .await?;

    Ok(Session {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at: super::client::now_unix() + tokens.expires_in,
        admin_id: session.admin_id,
        username: session.username,
        role: session.role,
    })
}
