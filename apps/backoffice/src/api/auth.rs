use super::ApiResult;
use crate::session::Session;
use domain::dto::LoginRequest;
use reqwest::Method;

pub async fn login(request: LoginRequest) -> ApiResult<Session> {
    super::decode(super::request(Method::POST, "/auth/login")?.json(&request)).await
}
pub async fn refresh_session() -> ApiResult<Session> {
    super::decode(super::request(Method::POST, "/auth/refresh")?).await
}
pub async fn logout() -> ApiResult<()> {
    super::send(super::request(Method::POST, "/auth/logout")?).await?;
    Ok(())
}
