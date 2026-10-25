use crate::{
    client::s3::S3Client,
    error::{AppError, upstream},
    validations::{self, files::UploadImage},
};
use axum::http::StatusCode;
use uuid::Uuid;

pub(crate) async fn upload(
    s3: &S3Client,
    directory: &str,
    image: UploadImage,
) -> Result<domain::dto::UploadedFileDTO, AppError> {
    validations::files::folder(directory)?;
    let extension = if image.content_type == "image/png" {
        "png"
    } else {
        "jpg"
    };
    let location = format!("{directory}/{}.{extension}", Uuid::new_v4());
    let (bucket, key) = location
        .split_once('/')
        .ok_or_else(|| AppError::new(StatusCode::BAD_REQUEST, "Invalid upload location"))?;
    let signed = s3
        .presign_image(bucket, key, image.content_type)
        .await
        .map_err(|error| upstream(error))?;
    s3.put(bucket, key, image.content_type, image.bytes)
        .await
        .map_err(|error| upstream(error))?;
    Ok(domain::dto::UploadedFileDTO {
        location,
        url: signed.url,
        expires_at: signed.expires_at.to_rfc3339(),
    })
}

#[cfg(test)]
#[path = "../tests/services/files.rs"]
mod tests;
