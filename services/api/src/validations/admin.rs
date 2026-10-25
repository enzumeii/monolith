use crate::error::AppError;
use crate::models::AdminPageRequest;
use axum::http::StatusCode;
use domain::dto::{CreateAdminRequest, ListAdminsQuery, UpdateAdminRequest};

impl TryFrom<ListAdminsQuery> for AdminPageRequest {
    type Error = AppError;
    fn try_from(query: ListAdminsQuery) -> Result<Self, Self::Error> {
        validate_page(&query)
    }
}

/// Map the raw query params to a validated `AdminPageRequest`.
/// Applies defaults (`page=1`, `per_page=25`) and enforces 1..=100.
pub fn validate_page(query: &ListAdminsQuery) -> Result<AdminPageRequest, AppError> {
    let (page, per_page) = super::common::validate_page(query.page, query.per_page)?;
    Ok(AdminPageRequest { page, per_page })
}

pub fn validate_create(req: &CreateAdminRequest) -> Result<(), AppError> {
    validate_username(&req.username)?;
    validate_password(&req.password)
}
pub fn validate_update_input(req: &UpdateAdminRequest) -> Result<(), AppError> {
    if req.username.is_none() && req.password.is_none() {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "update must include a username or password",
        ));
    }
    if let Some(username) = &req.username {
        validate_username(username)?;
    }
    if let Some(password) = &req.password {
        validate_password(password)?;
    }
    Ok(())
}
fn validate_username(username: &str) -> Result<(), AppError> {
    if !(1..=20).contains(&username.chars().count())
        || username.trim().is_empty()
        || username != username.trim()
    {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "username must contain 1–20 characters without surrounding whitespace",
        ));
    }
    Ok(())
}
fn validate_password(password: &str) -> Result<(), AppError> {
    if !(12..=128).contains(&password.chars().count()) {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "new password must contain 12–128 characters",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/validations/admin.rs"]
mod tests;
