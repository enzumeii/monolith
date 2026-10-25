use crate::{
    client::s3::S3Client,
    error::{AppError, upstream},
    models::conventions::Convention,
    transformers, validations,
};
use axum::http::StatusCode;
use domain::dto::ConventionDTO;
pub(crate) async fn attach(
    s3: &S3Client,
    stored_location: &str,
    slug: &str,
) -> Result<(), AppError> {
    let mime = validations::conventions::logo_location(stored_location)?;
    if validations::files::folder(
        stored_location
            .rsplit_once('/')
            .ok_or_else(|| invalid("Invalid logo_url"))?
            .0,
    )? != slug
    {
        return Err(invalid("Logo location must belong to the convention slug"));
    }
    let (bucket, key) = stored_location
        .split_once('/')
        .ok_or_else(|| invalid("Invalid logo_url"))?;
    let metadata = s3
        .metadata(bucket, key)
        .await
        .map_err(|error| upstream(error))?
        .ok_or_else(|| invalid("Logo object does not exist"))?;
    if !(1..=validations::files::MAX_BYTES as u64).contains(&metadata.size)
        || metadata.content_type != mime
    {
        return Err(bad_object());
    }
    let bytes = s3
        .download(bucket, key, validations::files::MAX_BYTES)
        .await
        .map_err(|error| upstream(error))?;
    if bytes.len() as u64 != metadata.size {
        return Err(bad_object());
    }
    validations::files::image(bytes, mime)
        .await
        .map_err(|source| bad_object().with_source(source))?;
    Ok(())
}

pub(crate) async fn project(record: Convention, s3: &S3Client) -> Result<ConventionDTO, AppError> {
    let payment_methods = record.methods()?;
    let preview = if let Some(location) = &record.logo_url {
        match preview(s3, location).await {
            Ok(signed) => Some(signed),
            Err(error) => {
                crate::error::log_background_error("Convention logo preview signing", &error);
                None
            }
        }
    } else {
        None
    };
    Ok(transformers::conventions::response(
        record,
        preview,
        payment_methods,
    ))
}

async fn preview(
    s3: &S3Client,
    stored_location: &str,
) -> Result<crate::client::s3::PresignedDownload, AppError> {
    let mime = validations::conventions::logo_location(stored_location)?;
    let (bucket, key) = stored_location
        .split_once('/')
        .ok_or_else(|| invalid("Invalid logo_url"))?;
    s3.presign_image(bucket, key, mime)
        .await
        .map_err(|error| upstream(error))
}

fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}
fn bad_object() -> AppError {
    AppError::new(StatusCode::BAD_GATEWAY, "Invalid stored convention logo")
}

#[cfg(test)]
#[path = "../../tests/services/conventions/logo.rs"]
mod tests;
