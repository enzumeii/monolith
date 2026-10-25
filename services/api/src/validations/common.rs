use crate::error::AppError;
use axum::{
    Json,
    extract::{
        Path, Query,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::StatusCode,
};

pub(crate) fn validate_page(
    page: Option<u32>,
    per_page: Option<u32>,
) -> Result<(u32, u32), AppError> {
    let page = page.unwrap_or(1);
    let per_page = per_page.unwrap_or(25);
    if page == 0 || !(1..=100).contains(&per_page) {
        return Err(invalid(
            "page must be positive and per_page must be between 1 and 100",
        ));
    }
    Ok((page, per_page))
}

pub(crate) fn body<T>(
    value: Result<Json<T>, JsonRejection>,
    message: &'static str,
) -> Result<T, AppError> {
    value.map(|Json(value)| value).map_err(|_| invalid(message))
}

pub(crate) fn query<T>(
    value: Result<Query<T>, QueryRejection>,
    message: &'static str,
) -> Result<T, AppError> {
    value
        .map(|Query(value)| value)
        .map_err(|_| invalid(message))
}

pub(crate) fn uuid_path(
    value: Result<Path<String>, PathRejection>,
    message: &'static str,
) -> Result<uuid::Uuid, AppError> {
    let Path(value) = value.map_err(|_| invalid(message))?;
    value.parse().map_err(|_| invalid(message))
}

pub(crate) fn name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if !(1..=120).contains(&name.chars().count()) {
        return Err(invalid("name must contain 1 to 120 characters"));
    }
    Ok(name.to_owned())
}

pub(crate) fn email(email: &str, message: &'static str) -> Result<String, AppError> {
    let email = email.trim();
    let Some((local, domain)) = email.split_once('@') else {
        return Err(invalid(message));
    };
    if email.len() > 254
        || email.chars().any(char::is_whitespace)
        || local.is_empty()
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || !domain.contains('.')
        || email.matches('@').count() != 1
    {
        return Err(invalid(message));
    }
    Ok(email.to_owned())
}

fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}
