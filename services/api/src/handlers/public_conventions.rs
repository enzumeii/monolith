use crate::{
    error::AppError,
    services::public_conventions as service,
    state::AppState,
    validations::{
        conventions as validate, cron::configs::validate_page, public_conventions as public,
    },
};
use axum::{
    Json,
    body::Bytes,
    extract::{
        Path, Query, State,
        rejection::{BytesRejection, JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, Uri},
};
use domain::dto::{
    ConventionQuote, ListPublicConventionsQuery, PublicCatalogueResponse, PublicConvention,
    PublicConventionListResponse, QuoteConventionRequest,
};

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: Result<Query<ListPublicConventionsQuery>, QueryRejection>,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<PublicConventionListResponse>, AppError> {
    no_body(body)?;
    let query = validate::query(query)?;
    let today = validate::date(&query.today)?;
    let (page, per_page) = validate_page(query.page, query.per_page)?;
    let locale = query
        .locale_country
        .as_deref()
        .map(|value| {
            public::country(value).ok_or_else(|| validate::invalid("Invalid locale_country"))
        })
        .transpose()?;
    let country = headers
        .get("x-vercel-ip-country")
        .and_then(|value| value.to_str().ok())
        .and_then(public::country)
        .or(locale);
    Ok(Json(
        service::directory(
            &state.pool,
            super::dependencies::s3_client(&state)?,
            today,
            country,
            page,
            per_page,
        )
        .await?,
    ))
}

pub async fn get(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<PublicConvention>, AppError> {
    validate::no_query(uri.query())?;
    no_body(body)?;
    let slug = public::slug(path)?;
    Ok(Json(
        service::detail(&state.pool, super::dependencies::s3_client(&state), &slug).await?,
    ))
}

pub async fn products(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<PublicCatalogueResponse>, AppError> {
    validate::no_query(uri.query())?;
    no_body(body)?;
    let slug = public::slug(path)?;
    Ok(Json(
        service::products(
            &state.pool,
            super::dependencies::s3_client(&state),
            super::dependencies::shopify_client(&state),
            &slug,
        )
        .await?,
    ))
}

pub async fn quote(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<QuoteConventionRequest>, JsonRejection>,
) -> Result<Json<ConventionQuote>, AppError> {
    validate::no_query(uri.query())?;
    let slug = public::slug(path)?;
    let request = public::quote(validate::body(body)?)?;
    Ok(Json(
        service::quote(
            &state.pool,
            super::dependencies::shopify_client(&state),
            &slug,
            request,
        )
        .await?,
    ))
}

fn no_body(value: Result<Bytes, BytesRejection>) -> Result<(), AppError> {
    if !value
        .map_err(|source| validate::invalid("Unexpected request body").with_source(source))?
        .is_empty()
    {
        return Err(validate::invalid("Unexpected request body"));
    }
    Ok(())
}
