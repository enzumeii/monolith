use dioxus::prelude::*;
use domain::dto::{
    AdminDTO, AdminListResponse, CreateAdminRequest, ListAdminsQuery, UpdateAdminRequest,
};

/// GET `/v1/admins?page=&per_page=`.
#[server(endpoint = "admins/list_admins")]
pub async fn list_admins(query: ListAdminsQuery) -> ServerFnResult<AdminListResponse> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(25);
    let state = crate::state::app_state();
    let credentials = super::cookies::read(&state)
        .await?
        .ok_or_else(|| super::client::status_error(401))?;
    let resp = state
        .http
        .get(state.url(&format!("/admins?page={page}&per_page={per_page}")))
        .bearer_auth(credentials.access)
        .send()
        .await
        .map_err(|_| super::client::unavailable())?;
    super::client::decode(resp, "admin list").await
}

/// GET `/v1/admins/{id}`.
#[server(endpoint = "admins/get_admin")]
pub async fn get_admin(id: i64) -> ServerFnResult<AdminDTO> {
    let state = crate::state::app_state();
    let credentials = super::cookies::read(&state)
        .await?
        .ok_or_else(|| super::client::status_error(401))?;
    let resp = state
        .http
        .get(state.url(&format!("/admins/{id}")))
        .bearer_auth(credentials.access)
        .send()
        .await
        .map_err(|_| super::client::unavailable())?;
    super::client::decode(resp, "admin").await
}

/// POST `/v1/admins`. Super admin only (API-enforced).
#[server(endpoint = "admins/create_admin")]
pub async fn create_admin(req: CreateAdminRequest) -> ServerFnResult<AdminDTO> {
    let state = crate::state::app_state();
    let credentials = super::cookies::read(&state)
        .await?
        .ok_or_else(|| super::client::status_error(401))?;
    let resp = state
        .http
        .post(state.url("/admins"))
        .bearer_auth(credentials.access)
        .json(&req)
        .send()
        .await
        .map_err(|_| super::client::unavailable())?;
    super::client::decode(resp, "created admin").await
}

/// PATCH `/v1/admins/{id}`. Super admins update any account; regular admins
/// only their own. The upstream revokes the target's refresh tokens on update.
#[server(endpoint = "admins/update_admin")]
pub async fn update_admin(id: i64, req: UpdateAdminRequest) -> ServerFnResult<AdminDTO> {
    let state = crate::state::app_state();
    let credentials = super::cookies::read(&state)
        .await?
        .ok_or_else(|| super::client::status_error(401))?;
    let resp = state
        .http
        .patch(state.url(&format!("/admins/{id}")))
        .bearer_auth(credentials.access)
        .json(&req)
        .send()
        .await
        .map_err(|_| super::client::unavailable())?;
    super::client::decode(resp, "updated admin").await
}

/// DELETE `/v1/admins/{id}`. Super admin only and never self (API-enforced).
#[server(endpoint = "admins/delete_admin")]
pub async fn delete_admin(id: i64) -> ServerFnResult<AdminDTO> {
    let state = crate::state::app_state();
    let credentials = super::cookies::read(&state)
        .await?
        .ok_or_else(|| super::client::status_error(401))?;
    let resp = state
        .http
        .delete(state.url(&format!("/admins/{id}")))
        .bearer_auth(credentials.access)
        .send()
        .await
        .map_err(|_| super::client::unavailable())?;
    super::client::decode(resp, "deleted admin").await
}
