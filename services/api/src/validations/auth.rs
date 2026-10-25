use crate::error::AppError;
use axum::http::StatusCode;
use domain::dto::LoginRequest;

pub(crate) fn login(req: &LoginRequest) -> Result<(), AppError> {
    if req.password.chars().count() > 128 {
        return Err(AppError::new(
            StatusCode::UNAUTHORIZED,
            "Invalid username or password",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/validations/auth.rs"]
mod tests;
