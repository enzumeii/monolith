use crate::{
    error::AppError,
    services::conventions as service,
    state::AppState,
    validations::{conventions as validate, cron::configs::validate_page},
};
use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{StatusCode, Uri},
};
use domain::dto::{
    ConventionDTO, ConventionListResponse, ConventionVariantListResponse,
    ConventionVariantsResponse, CreateConvention, ListConventionVariantsQuery,
    ListConventionsQuery, PatchConvention, ReplaceConventionVariants,
};
pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ListConventionsQuery>, QueryRejection>,
) -> Result<Json<ConventionListResponse>, AppError> {
    let q = validate::query(query)?;
    let (page, per_page) = validate_page(q.page, q.per_page)?;
    let response = service::list(&state.pool, page, per_page, q.preorder_enabled).await?;
    Ok(Json(response))
}
pub async fn create(
    State(state): State<AppState>,
    uri: Uri,
    body: Result<Json<CreateConvention>, JsonRejection>,
) -> Result<(StatusCode, Json<ConventionDTO>), AppError> {
    validate::no_query(uri.query())?;
    let input = validate::create(validate::body(body)?)?;
    Ok((
        StatusCode::CREATED,
        Json(service::create(&state.pool, &input).await?),
    ))
}
pub async fn get(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<ConventionDTO>, AppError> {
    validate::no_query(uri.query())?;
    Ok(Json(
        service::get(&state.pool, validate::id(path)?)
            .await?
            .into_dto()?,
    ))
}
pub async fn patch(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<PatchConvention>, JsonRejection>,
) -> Result<Json<ConventionDTO>, AppError> {
    validate::no_query(uri.query())?;
    let id = validate::id(path)?;
    let changes = validate::body(body)?;
    validate::patch_shape(&changes)?;
    let prepared = service::prepare_patch(&state.pool, id, changes).await?;
    if !prepared.checks.is_empty() {
        service::validate_selection(
            super::dependencies::shopify_client(&state)?,
            &prepared.checks,
        )
        .await?;
    }
    Ok(Json(service::finish_patch(&state.pool, prepared).await?))
}
pub async fn delete(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<StatusCode, AppError> {
    validate::no_query(uri.query())?;
    validate::delete_body(body)?;
    service::delete(&state.pool, validate::id(path)?).await?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn variants(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<ListConventionVariantsQuery>, QueryRejection>,
) -> Result<Json<ConventionVariantListResponse>, AppError> {
    let id = validate::id(path)?;
    let q = validate::query(query)?;
    let (page, per_page) = validate_page(q.page, q.per_page)?;
    Ok(Json(
        service::list_variants(&state.pool, id, page, per_page).await?,
    ))
}
pub async fn replace(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<ReplaceConventionVariants>, JsonRejection>,
) -> Result<Json<ConventionVariantsResponse>, AppError> {
    validate::no_query(uri.query())?;
    let id = validate::id(path)?;
    let items = validate::variants(validate::body(body)?)?;
    let prepared = service::prepare_save(&state.pool, id, items).await?;
    if !prepared.checks.is_empty() {
        service::validate_selection(
            super::dependencies::shopify_client(&state)?,
            &prepared.checks,
        )
        .await?;
    }
    Ok(Json(service::finish_save(&state.pool, prepared).await?))
}
