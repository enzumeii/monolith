use crate::{error::AppError, services::conventions::logo};
use axum::extract::{Query, rejection::QueryRejection};
use domain::dto::UploadFileQuery;

pub(crate) fn upload(
    query: Result<Query<UploadFileQuery>, QueryRejection>,
) -> Result<UploadFileQuery, AppError> {
    let Query(query) = query.map_err(|source| {
        AppError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "Invalid file upload query",
        )
        .with_source(source)
    })?;
    logo::folder(&query.location)?;
    Ok(query)
}
