use crate::{error::AppError, services, state::AppState, validations};
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
    validations::public_conventions::no_body(body)?;
    let query = validations::common::query(query, "Invalid convention query")?;
    let today = validations::conventions::date(&query.today)?;
    let (page, per_page) = validations::common::validate_page(query.page, query.per_page)?;
    let locale = query
        .locale_country
        .as_deref()
        .map(|value| {
            validations::public_conventions::country(value)
                .ok_or_else(|| validations::conventions::invalid("Invalid locale_country"))
        })
        .transpose()?;
    let country = headers
        .get("x-vercel-ip-country")
        .and_then(|value| value.to_str().ok())
        .and_then(validations::public_conventions::country)
        .or(locale);
    Ok(Json(
        services::public_conventions::directory(
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
    validations::conventions::no_query(uri.query())?;
    validations::public_conventions::no_body(body)?;
    let slug = validations::public_conventions::slug(path)?;
    Ok(Json(
        services::public_conventions::detail(
            &state.pool,
            super::dependencies::s3_client(&state),
            &slug,
        )
        .await?,
    ))
}

pub async fn products(
    State(state): State<AppState>,
    uri: Uri,
    path: Result<Path<String>, PathRejection>,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<PublicCatalogueResponse>, AppError> {
    validations::conventions::no_query(uri.query())?;
    validations::public_conventions::no_body(body)?;
    let slug = validations::public_conventions::slug(path)?;
    Ok(Json(
        services::public_conventions::products(
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
    validations::conventions::no_query(uri.query())?;
    let slug = validations::public_conventions::slug(path)?;
    let request = validations::public_conventions::quote(validations::common::body(
        body,
        "Invalid convention request body",
    )?)?;
    Ok(Json(
        services::public_conventions::quote(
            &state.pool,
            super::dependencies::shopify_client(&state),
            &slug,
            request,
        )
        .await?,
    ))
}
