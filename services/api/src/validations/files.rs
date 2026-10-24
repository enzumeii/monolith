use crate::error::AppError;
use axum::extract::{Query, rejection::QueryRejection};
use axum::http::StatusCode;
use domain::dto::UploadFileQuery;
use image::{ImageFormat, ImageReader, Limits};
use std::io::Cursor;

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
    folder(&query.location)?;
    Ok(query)
}

pub(crate) const MAX_BYTES: usize = 2_097_152;
const MAX_DIMENSION: u32 = 4096;

#[derive(Debug)]
pub(crate) struct UploadImage {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

pub(crate) fn folder(value: &str) -> Result<&str, AppError> {
    let slug = value
        .strip_prefix("conventions/")
        .ok_or_else(|| invalid("Invalid upload location"))?;
    super::conventions::slug(slug)?;
    Ok(slug)
}

pub(crate) async fn image(bytes: Vec<u8>, mime: &str) -> Result<UploadImage, AppError> {
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
        Ok(UploadImage {
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

fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

#[cfg(test)]
#[path = "../tests/validations/files.rs"]
mod tests;
