use crate::{
    client::s3::S3Client,
    error::{AppError, upstream},
};
use axum::http::StatusCode;
use image::{ImageFormat, ImageReader, Limits};
use std::io::Cursor;
use uuid::Uuid;

pub(crate) const MAX_BYTES: usize = 2_097_152;
const MAX_DIMENSION: u32 = 4096;

#[derive(Debug)]
pub(crate) struct Logo {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

pub(crate) fn url(id: Uuid, asset: Uuid) -> String {
    format!("/api/v1/conventions/{id}/logo/{asset}")
}
pub(crate) fn is_managed_url(id: Uuid, value: &str) -> bool {
    let prefix = format!("/api/v1/conventions/{id}/logo/");
    value
        .strip_prefix(&prefix)
        .and_then(|v| v.parse::<Uuid>().ok())
        .is_some_and(|asset| url(id, asset) == value)
}
pub(crate) async fn validate(bytes: Vec<u8>, mime: &str) -> Result<Logo, AppError> {
    let format = match mime {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        _ => {
            return Err(invalid(
                "Logo must have Content-Type image/png or image/jpeg",
            ));
        }
    };
    if bytes.is_empty() {
        return Err(invalid("Logo must not be empty"));
    }
    if bytes.len() > MAX_BYTES {
        return Err(AppError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Logo exceeds 2 MiB",
        ));
    }
    tokio::task::spawn_blocking(move || {
        if image::guess_format(&bytes).ok() != Some(format) {
            return Err(invalid("Logo bytes do not match Content-Type"));
        }
        let complete = match format {
            ImageFormat::Png => bytes.ends_with(b"\0\0\0\0IEND\xaeB`\x82"),
            ImageFormat::Jpeg => bytes.ends_with(&[0xff, 0xd9]),
            _ => false,
        };
        if !complete {
            return Err(invalid("Logo image is incomplete"));
        }
        let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_DIMENSION);
        limits.max_image_height = Some(MAX_DIMENSION);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let decoded = reader.decode().map_err(|source| {
            invalid("Logo must be a complete PNG or JPEG within 4096 by 4096 pixels")
                .with_source(source)
        })?;
        if decoded.width() == 0 || decoded.height() == 0 {
            return Err(invalid("Logo dimensions must be positive"));
        }
        Ok(Logo {
            bytes,
            content_type: if format == ImageFormat::Png {
                "image/png"
            } else {
                "image/jpeg"
            },
        })
    })
    .await
    .map_err(|source| AppError::from(anyhow::Error::new(source)))?
}
pub(crate) async fn put(s3: &S3Client, id: Uuid, asset: Uuid, logo: Logo) -> Result<(), AppError> {
    s3.put(&key(id, asset), logo.content_type, logo.bytes)
        .await
        .map_err(|error| upstream(error))
}
pub(crate) async fn download(s3: &S3Client, id: Uuid, asset: Uuid) -> Result<Logo, AppError> {
    let key = key(id, asset);
    let metadata = s3
        .metadata(&key)
        .await
        .map_err(|error| upstream(error))?
        .ok_or_else(missing)?;
    if !(1..=MAX_BYTES as u64).contains(&metadata.size)
        || !matches!(metadata.content_type.as_str(), "image/png" | "image/jpeg")
    {
        return Err(bad_object());
    }
    let bytes = s3
        .download(&key, MAX_BYTES)
        .await
        .map_err(|error| upstream(error))?;
    if bytes.len() as u64 != metadata.size {
        return Err(bad_object());
    }
    validate(bytes, &metadata.content_type)
        .await
        .map_err(|source| bad_object().with_source(source))
}
fn key(id: Uuid, asset: Uuid) -> String {
    format!("conventions/logos/{id}/{asset}")
}
fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}
fn missing() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "Convention logo not found")
}
fn bad_object() -> AppError {
    AppError::new(StatusCode::BAD_GATEWAY, "Invalid stored convention logo")
}

#[cfg(test)]
#[path = "../tests/storage/convention_logo.rs"]
mod tests;
