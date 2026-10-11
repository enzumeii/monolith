//! Admin-management server functions.
//!
//! Used by the Admins page (super admin) and by the Profile page for the
//! self-service update — a single `update_admin` endpoint is the source of
//! truth for both, so there is no separate profile API module.

use dioxus::prelude::*;
use domain::AdminRole;
use domain::dto::{AdminDTO, AdminListResponse};

use crate::session::Session;

// Only used inside the server-side function bodies (compiled out on web).
#[cfg(feature = "server")]
use super::auth::ensure_valid;
#[cfg(feature = "server")]
use super::client::{decode, url};
#[cfg(feature = "server")]
use domain::dto::{CreateAdminRequest, UpdateAdminRequest};

/// List admins with pagination (`GET /v1/admins`).
#[server]
pub async fn list_admins(
    session: Session,
    page: u32,
    per_page: u32,
) -> ServerFnResult<(Session, AdminListResponse)> {
    let session = ensure_valid(session).await?;
    let out: AdminListResponse = decode(
        reqwest::Client::new()
            .get(url(&format!("/admins?page={page}&per_page={per_page}")))
            .bearer_auth(&session.access_token)
            .send()
            .await
            .map_err(|e| ServerFnError::new(format!("list request failed: {e}")))?,
        "admin list",
    )
    .await?;
    Ok((session, out))
}

/// Create a new admin (`POST /v1/admins`). Super admin only (API-enforced).
#[server]
pub async fn create_admin(
    session: Session,
    username: String,
    password: String,
    role: AdminRole,
) -> ServerFnResult<(Session, AdminDTO)> {
    let session = ensure_valid(session).await?;
    let out: AdminDTO = decode(
        reqwest::Client::new()
            .post(url("/admins"))
            .bearer_auth(&session.access_token)
            .json(&CreateAdminRequest {
                username,
                password,
                role,
            })
            .send()
            .await
            .map_err(|e| ServerFnError::new(format!("create request failed: {e}")))?,
        "created admin",
    )
    .await?;
    Ok((session, out))
}

/// Update an admin's username and/or password (`PATCH /v1/admins/{id}`).
///
/// A super admin may update any account; a regular admin may update only
/// their own (their profile). The upstream revokes the target's refresh
/// tokens on update.
#[server]
pub async fn update_admin(
    session: Session,
    id: i64,
    username: Option<String>,
    password: Option<String>,
) -> ServerFnResult<(Session, AdminDTO)> {
    let session = ensure_valid(session).await?;
    let out: AdminDTO = decode(
        reqwest::Client::new()
            .patch(url(&format!("/admins/{id}")))
            .bearer_auth(&session.access_token)
            .json(&UpdateAdminRequest { username, password })
            .send()
            .await
            .map_err(|e| ServerFnError::new(format!("update request failed: {e}")))?,
        "updated admin",
    )
    .await?;
    Ok((session, out))
}

/// Delete an admin (`DELETE /v1/admins/{id}`). Super admin only and never
/// self (API-enforced).
#[server]
pub async fn delete_admin(session: Session, id: i64) -> ServerFnResult<(Session, AdminDTO)> {
    let session = ensure_valid(session).await?;
    let out: AdminDTO = decode(
        reqwest::Client::new()
            .delete(url(&format!("/admins/{id}")))
            .bearer_auth(&session.access_token)
            .send()
            .await
            .map_err(|e| ServerFnError::new(format!("delete request failed: {e}")))?,
        "deleted admin",
    )
    .await?;
    Ok((session, out))
}
