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
    ConventionVariantListResponse, ConventionVariantsResponse, CreateConvention,
    ListConventionVariantsQuery, ListConventionsQuery, PatchConvention, ReplaceConventionVariants,
};
pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<ListConventionsQuery>, QueryRejection>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let q = validate::query(query)?;
    let (page, per_page) = validate_page(q.page, q.per_page)?;
    let response = if q.preorder_enabled == Some(true) {
        service::enabled_list(&state.pool, optional_s3(&state), page, per_page).await?
    } else {
        service::list(
            &state.pool,
            optional_s3(&state),
            page,
            per_page,
            q.preorder_enabled,
        )
        .await?
    };
    Ok((no_store(), Json(response)))
}
pub async fn create(
    State(state): State<AppState>,
    uri: Uri,
    body: Result<Json<CreateConvention>, JsonRejection>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    validate::no_query(uri.query())?;
    let input = validate::create(validate::body(body)?)?;
    let s3 = if input.logo_location.is_some() {
        Some(super::dependencies::s3_client(&state)?)
    } else {
        optional_s3(&state)
    };
    Ok((
        StatusCode::CREATED,
        no_store(),
        Json(service::create(&state.pool, s3, &state.settings.conventions, &input).await?),
    ))
}
pub async fn get(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    validate::no_query(uri.query())?;
    Ok((
        no_store(),
        Json(
            service::logo::project(
                service::get(&state.pool, validate::id(path)?).await?,
                optional_s3(&state),
            )
            .await?,
        ),
    ))
}
pub async fn patch(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<PatchConvention>, JsonRejection>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    validate::no_query(uri.query())?;
    let id = validate::id(path)?;
    let changes = validate::body(body)?;
    validate::patch_shape(&changes)?;
    let s3 = if matches!(changes.logo_location, domain::dto::PatchField::Set(_)) {
        Some(super::dependencies::s3_client(&state)?)
    } else {
        optional_s3(&state)
    };
    let prepared =
        service::prepare_patch(&state.pool, s3, &state.settings.conventions, id, changes).await?;
    if !prepared.checks.is_empty() {
        service::validate_selection(
            super::dependencies::shopify_client(&state)?,
            &prepared.checks,
        )
        .await?;
    }
    Ok((
        no_store(),
        Json(
            service::finish_patch(
                &state.pool,
                optional_s3(&state),
                &state.settings.conventions,
                prepared,
            )
            .await?,
        ),
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

fn no_store() -> [(axum::http::HeaderName, &'static str); 1] {
    [(axum::http::header::CACHE_CONTROL, "no-store")]
}
fn optional_s3(state: &AppState) -> Option<&crate::client::s3::S3Client> {
    match state.s3.as_ref() {
        Ok(client) => client.as_deref(),
        Err(error) => {
            tracing::debug!(error = ?error, "Convention S3 client unavailable");
            None
        }
    }
}
