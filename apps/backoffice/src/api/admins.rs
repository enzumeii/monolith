use dioxus::prelude::*;
use domain::dto::{
    AdminDTO, AdminListResponse, CreateAdminRequest, ListAdminsQuery, UpdateAdminRequest,
};

/// GET `/v1/admins?page=&per_page=`.
#[server]
pub async fn list_admins(
    query: ListAdminsQuery,
    access_token: String,
) -> ServerFnResult<AdminListResponse> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(25);
    let state = crate::state::app_state();
    let resp = state
        .http
        .get(state.url(&format!("/admins?page={page}&per_page={per_page}")))
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("list request failed: {e}")))?;
    super::client::decode(resp, "admin list").await
}

/// GET `/v1/admins/{id}`.
#[server]
pub async fn get_admin(id: i64, access_token: String) -> ServerFnResult<AdminDTO> {
    let state = crate::state::app_state();
    let resp = state
        .http
        .get(state.url(&format!("/admins/{id}")))
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("get admin request failed: {e}")))?;
    super::client::decode(resp, "admin").await
}

/// POST `/v1/admins`. Super admin only (API-enforced).
#[server]
pub async fn create_admin(
    req: CreateAdminRequest,
    access_token: String,
) -> ServerFnResult<AdminDTO> {
    let state = crate::state::app_state();
    let resp = state
        .http
        .post(state.url("/admins"))
        .bearer_auth(&access_token)
        .json(&req)
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("create request failed: {e}")))?;
    super::client::decode(resp, "created admin").await
}

/// PATCH `/v1/admins/{id}`. Super admins update any account; regular admins
/// only their own. The upstream revokes the target's refresh tokens on update.
#[server]
pub async fn update_admin(
    id: i64,
    req: UpdateAdminRequest,
    access_token: String,
) -> ServerFnResult<AdminDTO> {
    let state = crate::state::app_state();
    let resp = state
        .http
        .patch(state.url(&format!("/admins/{id}")))
        .bearer_auth(&access_token)
        .json(&req)
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("update request failed: {e}")))?;
    super::client::decode(resp, "updated admin").await
}

/// DELETE `/v1/admins/{id}`. Super admin only and never self (API-enforced).
#[server]
pub async fn delete_admin(id: i64, access_token: String) -> ServerFnResult<AdminDTO> {
    let state = crate::state::app_state();
    let resp = state
        .http
        .delete(state.url(&format!("/admins/{id}")))
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| ServerFnError::new(format!("delete request failed: {e}")))?;
    super::client::decode(resp, "deleted admin").await
}
