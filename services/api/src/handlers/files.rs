use crate::{error::AppError, services, state::AppState, validations};
use axum::{
    Json,
    body::Bytes,
    extract::{
        Query, State,
        rejection::{BytesRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode, header},
};
use domain::dto::UploadFileQuery;

pub async fn upload(
    State(state): State<AppState>,
    query: Result<Query<UploadFileQuery>, QueryRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let query = validations::files::upload(query)?;
    let body = body.map_err(|source| {
        AppError::new(source.status(), "Unable to read logo body").with_source(source)
    })?;
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::new(StatusCode::BAD_REQUEST, "Logo Content-Type is required"))?;
    let logo = validations::files::image(body.to_vec(), mime).await?;
    let result = services::files::upload(
        super::dependencies::s3_client(&state)?,
        &query.location,
        logo,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        Json(result),
    ))
}
