use super::ApiResult;
use domain::dto::{
    AdminDTO, AdminListResponse, CreateAdminRequest, ListAdminsQuery, UpdateAdminRequest,
};
use reqwest::Method;

pub async fn list_admins(query: ListAdminsQuery) -> ApiResult<AdminListResponse> {
    super::decode(super::request(Method::GET, "/admins")?.query(&query)).await
}

pub async fn create_admin(request: CreateAdminRequest) -> ApiResult<AdminDTO> {
    super::decode(super::request(Method::POST, "/admins")?.json(&request)).await
}
pub async fn update_admin(id: i64, request: UpdateAdminRequest) -> ApiResult<AdminDTO> {
    super::decode(super::request(Method::PATCH, &format!("/admins/{id}"))?.json(&request)).await
}
pub async fn delete_admin(id: i64) -> ApiResult<AdminDTO> {
    super::decode(super::request(Method::DELETE, &format!("/admins/{id}"))?).await
}
