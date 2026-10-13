use crate::error::AppError;
use crate::models::AuthenticatedAdmin;
use domain::dto::{AdminRole, CreateAdminRequest, ListAdminsQuery, UpdateAdminRequest};

#[derive(Debug, Clone, Copy)]
pub struct AdminPageRequest {
    page: u32,
    per_page: u32,
}

impl AdminPageRequest {
    #[must_use]
    pub fn page(self) -> u32 {
        self.page
    }
    #[must_use]
    pub fn per_page(self) -> u32 {
        self.per_page
    }
}
impl TryFrom<ListAdminsQuery> for AdminPageRequest {
    type Error = AppError;
    fn try_from(query: ListAdminsQuery) -> Result<Self, Self::Error> {
        validate_page(&query)
    }
}

/// Map the raw query params to a validated `AdminPageRequest`.
/// Applies defaults (`page=1`, `per_page=25`) and enforces 1..=100.
pub fn validate_page(query: &ListAdminsQuery) -> Result<AdminPageRequest, AppError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(25);
    if page == 0 || per_page == 0 || per_page > 100 {
        return Err(AppError::BadRequest(
            "page must be positive and per_page must be between 1 and 100".into(),
        ));
    }
    Ok(AdminPageRequest { page, per_page })
}

/// Ensure the caller may update the target account.
///
/// `SuperAdmin` may update any account; a regular `Admin` may only update
/// their own account. Anything else is rejected with `Forbidden`.
pub fn validate_update(claims: &AuthenticatedAdmin, target_id: i64) -> Result<(), AppError> {
    let allowed = claims.role == AdminRole::SuperAdmin || claims.id == target_id;
    if !allowed {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// Ensure the caller is not deleting their own account.
pub fn validate_delete(claims: &AuthenticatedAdmin, target_id: i64) -> Result<(), AppError> {
    if claims.id == target_id {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

pub fn validate_create(req: &CreateAdminRequest) -> Result<(), AppError> {
    validate_username(&req.username)?;
    validate_password(&req.password)
}
pub fn validate_update_input(req: &UpdateAdminRequest) -> Result<(), AppError> {
    if req.username.is_none() && req.password.is_none() {
        return Err(AppError::BadRequest(
            "update must include a username or password".into(),
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
        return Err(AppError::BadRequest(
            "username must contain 1–20 characters without surrounding whitespace".into(),
        ));
    }
    Ok(())
}
fn validate_password(password: &str) -> Result<(), AppError> {
    if !(12..=128).contains(&password.chars().count()) {
        return Err(AppError::BadRequest(
            "new password must contain 12–128 characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/validations/admin.rs"]
mod tests;
