use crate::{error::AppError, services, state::AppState, transformers, validations};
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
    let q = validations::common::query(query, "Invalid convention query")?;
    let (page, per_page) = validations::common::validate_page(q.page, q.per_page)?;
    let response = services::conventions::list(
        &state.pool,
        super::dependencies::s3_client(&state)?,
        page,
        per_page,
        q.preorder_enabled,
    )
    .await?;
    Ok(Json(response))
}
pub async fn create(
    State(state): State<AppState>,
    uri: Uri,
    body: Result<Json<CreateConvention>, JsonRejection>,
) -> Result<(StatusCode, Json<ConventionDTO>), AppError> {
    validations::conventions::no_query(uri.query())?;
    let checked = validations::conventions::create(validations::common::body(
        body,
        "Invalid convention request body",
    )?)?;
    let input = transformers::conventions::create(checked);
    Ok((
        StatusCode::CREATED,
        Json(
            services::conventions::create(
                &state.pool,
                super::dependencies::s3_client(&state),
                &input,
            )
            .await?,
        ),
    ))
}
pub async fn get(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
) -> Result<Json<ConventionDTO>, AppError> {
    validations::conventions::no_query(uri.query())?;
    let id = validations::common::uuid_path(path, "Invalid convention identifier")?;
    let s3 = super::dependencies::s3_client(&state)?;
    Ok(Json(
        services::conventions::logo::project(
            services::conventions::get(&state.pool, id).await?,
            s3,
        )
        .await?,
    ))
}
pub async fn patch(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<PatchConvention>, JsonRejection>,
) -> Result<Json<ConventionDTO>, AppError> {
    validations::conventions::no_query(uri.query())?;
    let id = validations::common::uuid_path(path, "Invalid convention identifier")?;
    let changes = validations::conventions::patch(validations::common::body(
        body,
        "Invalid convention request body",
    )?)?;
    let s3 = super::dependencies::s3_client(&state)?;
    Ok(Json(
        services::conventions::patch(
            &state.pool,
            s3,
            super::dependencies::shopify_client(&state),
            id,
            changes,
        )
        .await?,
    ))
}

pub async fn delete(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<StatusCode, AppError> {
    validations::conventions::no_query(uri.query())?;
    validations::conventions::delete_body(body)?;
    services::conventions::delete(
        &state.pool,
        validations::common::uuid_path(path, "Invalid convention identifier")?,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn variants(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<ListConventionVariantsQuery>, QueryRejection>,
) -> Result<Json<ConventionVariantListResponse>, AppError> {
    let id = validations::common::uuid_path(path, "Invalid convention identifier")?;
    let q = validations::common::query(query, "Invalid convention query")?;
    let (page, per_page) = validations::common::validate_page(q.page, q.per_page)?;
    Ok(Json(
        services::conventions::list_variants(&state.pool, id, page, per_page).await?,
    ))
}
pub async fn replace(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<ReplaceConventionVariants>, JsonRejection>,
) -> Result<Json<ConventionVariantsResponse>, AppError> {
    validations::conventions::no_query(uri.query())?;
    let id = validations::common::uuid_path(path, "Invalid convention identifier")?;
    let items = validations::conventions::variants(validations::common::body(
        body,
        "Invalid convention request body",
    )?)?;
    Ok(Json(
        services::conventions::replace_variants(
            &state.pool,
            super::dependencies::shopify_client(&state),
            id,
            items,
        )
        .await?,
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmptyQuery {}

pub async fn options(
    query: Result<Query<EmptyQuery>, QueryRejection>,
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<domain::dto::ConventionOptionsResponse>, AppError> {
    validations::common::query(query, "Invalid convention query")?;
    let body = body.map_err(|source| {
        validations::conventions::invalid("Unexpected request body").with_source(source)
    })?;
    if !body.is_empty() {
        return Err(validations::conventions::invalid("Unexpected request body"));
    }
    Ok(Json(services::conventions::options()))
}
