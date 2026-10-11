use crate::{error::AppError, services, state::AppState};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use domain::dto::{
    AdminDTO, AdminListResponse, AdminPageRequest, Claims, CreateAdminRequest, ListAdminsQuery,
    UpdateAdminRequest,
};

pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListAdminsQuery>,
) -> Result<Json<AdminListResponse>, AppError> {
    let page = validate_page(&query)?;
    let response = services::admin::list(&state.pool, page).await?;
    Ok(Json(response))
}

pub async fn add(
    State(state): State<AppState>,
    Json(payload): Json<CreateAdminRequest>,
) -> Result<(StatusCode, Json<AdminDTO>), AppError> {
    let response = services::admin::add(&state.pool, payload).await?;

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateAdminRequest>,
) -> Result<Json<AdminDTO>, AppError> {
    services::admin::ensure_self(claims.admin_id, id)?;

    let response = services::admin::update(&state.pool, id, payload).await?;
    services::auth::logout(&state.pool, claims.admin_id).await?;

    Ok(Json(response))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AdminDTO>, AppError> {
    let response = services::admin::get(&state.pool, id).await?;

    Ok(Json(response))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<AdminDTO>, AppError> {
    let response = services::admin::delete(&state.pool, id).await?;
    services::auth::logout(&state.pool, id).await?;

    Ok(Json(response))
}

/// Map the raw query params to a validated `AdminPageRequest`.
/// Applies defaults (`page=1`, `per_page=25`) and enforces 1..=100.
fn validate_page(query: &ListAdminsQuery) -> Result<AdminPageRequest, AppError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(25);
    if page == 0 || per_page == 0 || per_page > 100 {
        return Err(AppError::BadRequest(
            "page must be positive and per_page must be between 1 and 100".into(),
        ));
    }
    Ok(AdminPageRequest { page, per_page })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn query(page: Option<u32>, per_page: Option<u32>) -> ListAdminsQuery {
        ListAdminsQuery { page, per_page }
    }

    #[test]
    fn validate_page_applies_defaults_when_omitted() {
        let page = validate_page(&query(None, None)).unwrap();
        assert_eq!(page.page, 1);
        assert_eq!(page.per_page, 25);
    }

    #[test]
    fn validate_page_rejects_zero_page() {
        assert!(matches!(
            validate_page(&query(Some(0), Some(25))),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn validate_page_rejects_zero_per_page() {
        assert!(matches!(
            validate_page(&query(Some(1), Some(0))),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn validate_page_rejects_per_page_over_100() {
        assert!(matches!(
            validate_page(&query(Some(1), Some(101))),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn validate_page_accepts_boundary_values() {
        assert!(validate_page(&query(Some(1), Some(1))).is_ok());
        assert!(validate_page(&query(Some(1), Some(100))).is_ok());
    }
}
