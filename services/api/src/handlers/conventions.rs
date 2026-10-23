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
    let response = if q.preorder_enabled == Some(true) {
        service::enabled_list(&state.pool, page, per_page).await?
    } else {
        service::list(&state.pool, page, per_page, q.preorder_enabled).await?
    };
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
        Json(service::create(&state.pool, &state.settings.conventions, &input).await?),
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
    let prepared =
        service::prepare_patch(&state.pool, &state.settings.conventions, id, changes).await?;
    if !prepared.checks.is_empty() {
        service::validate_selection(
            super::dependencies::shopify_client(&state)?,
            &prepared.checks,
        )
        .await?;
    }
    Ok(Json(
        service::finish_patch(&state.pool, &state.settings.conventions, prepared).await?,
    ))
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
    let prepared =
        service::prepare_save(&state.pool, &state.settings.conventions, id, items).await?;
    if !prepared.checks.is_empty() {
        service::validate_selection(
            super::dependencies::shopify_client(&state)?,
            &prepared.checks,
        )
        .await?;
    }
    Ok(Json(
        service::finish_save(&state.pool, &state.settings.conventions, prepared).await?,
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmptyQuery {}

pub async fn options(
    State(state): State<AppState>,
    query: Result<Query<EmptyQuery>, QueryRejection>,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<domain::dto::ConventionOptionsResponse>, AppError> {
    validate::query(query)?;
    let body =
        body.map_err(|source| validate::invalid("Unexpected request body").with_source(source))?;
    if !body.is_empty() {
        return Err(validate::invalid("Unexpected request body"));
    }
    Ok(Json(domain::dto::ConventionOptionsResponse {
        currencies: state
            .settings
            .conventions
            .currencies
            .iter()
            .map(|currency| domain::dto::ConventionCurrencyOption {
                code: currency.code.clone(),
                payment_methods: currency.payment_methods.clone(),
            })
            .collect(),
    }))
}
pub async fn upload_logo(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<EmptyQuery>, QueryRejection>,
    headers: axum::http::HeaderMap,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<ConventionDTO>, AppError> {
    let id = validate::id(path)?;
    validate::query(query)?;
    let body = body.map_err(|source| {
        AppError::new(source.status(), "Unable to read logo body").with_source(source)
    })?;
    let mime = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| validate::invalid("Logo Content-Type is required"))?;
    let logo = crate::storage::convention_logo::validate(body.to_vec(), mime).await?;
    service::get(&state.pool, id).await?;
    Ok(Json(
        service::logo::upload(
            &state.pool,
            super::dependencies::s3_client(&state)?,
            id,
            logo,
        )
        .await?,
    ))
}
pub async fn read_logo(
    State(state): State<AppState>,
    path: Result<Path<(String, String)>, PathRejection>,
    query: Result<Query<EmptyQuery>, QueryRejection>,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    validate::query(query)?;
    let body =
        body.map_err(|source| validate::invalid("Unexpected request body").with_source(source))?;
    if !body.is_empty() {
        return Err(validate::invalid("Unexpected request body"));
    }
    let Path((id, asset)) = path.map_err(|_| validate::invalid("Invalid logo identifiers"))?;
    let id = id
        .parse()
        .map_err(|_| validate::invalid("Invalid convention identifier"))?;
    let asset = asset
        .parse()
        .map_err(|_| validate::invalid("Invalid logo asset identifier"))?;
    service::logo::current(&state.pool, id, asset).await?;
    let logo = crate::storage::convention_logo::download(
        super::dependencies::s3_client(&state)?,
        id,
        asset,
    )
    .await?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, logo.content_type),
            (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        logo.bytes,
    ))
}
