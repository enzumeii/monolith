use crate::{
    client::s3::S3Client,
    error::{AppError, upstream},
    models::conventions::Convention,
};
use axum::http::StatusCode;
use domain::dto::ConventionDTO;
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

pub(crate) fn folder(value: &str) -> Result<&str, AppError> {
    let slug = value
        .strip_prefix("conventions/")
        .ok_or_else(|| invalid("Invalid upload location"))?;
    crate::validations::conventions::slug(slug)?;
    Ok(slug)
}

pub(crate) fn location(value: &str, slug: Option<&str>) -> Result<&'static str, AppError> {
    let (directory, filename) = value
        .rsplit_once('/')
        .ok_or_else(|| invalid("Invalid logo_url"))?;
    let owner = folder(directory)?;
    if slug.is_some_and(|slug| slug != owner) {
        return Err(invalid("Logo location must belong to the convention slug"));
    }
    let (id, extension) = filename
        .rsplit_once('.')
        .ok_or_else(|| invalid("Invalid logo_url"))?;
    let asset: Uuid = id.parse().map_err(|_| invalid("Invalid logo_url"))?;
    if asset.to_string() != id
        || asset.get_version_num() != 4
        || asset.get_variant() != uuid::Variant::RFC4122
    {
        return Err(invalid("Invalid logo_url"));
    }
    match extension {
        "png" => Ok("image/png"),
        "jpg" => Ok("image/jpeg"),
        _ => Err(invalid("Invalid logo_url")),
    }
}

pub(crate) async fn upload(
    s3: &S3Client,
    directory: &str,
    logo: Logo,
) -> Result<domain::dto::UploadedFileDTO, AppError> {
    folder(directory)?;
    let extension = if logo.content_type == "image/png" {
        "png"
    } else {
        "jpg"
    };
    let location = format!("{directory}/{}.{extension}", Uuid::new_v4());
    let (bucket, key) = location
        .split_once('/')
        .ok_or_else(|| invalid("Invalid upload location"))?;
    let signed = s3
        .presign_image(bucket, key, logo.content_type)
        .await
        .map_err(|error| upstream(error))?;
    s3.put(bucket, key, logo.content_type, logo.bytes)
        .await
        .map_err(|error| upstream(error))?;
    Ok(domain::dto::UploadedFileDTO {
        location,
        url: signed.url,
        expires_at: signed.expires_at.to_rfc3339(),
    })
}

pub(crate) async fn attach(
    s3: &S3Client,
    stored_location: &str,
    slug: &str,
) -> Result<(), AppError> {
    let mime = location(stored_location, Some(slug))?;
    let (bucket, key) = stored_location
        .split_once('/')
        .ok_or_else(|| invalid("Invalid logo_url"))?;
    let metadata = s3
        .metadata(bucket, key)
        .await
        .map_err(|error| upstream(error))?
        .ok_or_else(|| invalid("Logo object does not exist"))?;
    if !(1..=MAX_BYTES as u64).contains(&metadata.size) || metadata.content_type != mime {
        return Err(bad_object());
    }
    let bytes = s3
        .download(bucket, key, MAX_BYTES)
        .await
        .map_err(|error| upstream(error))?;
    if bytes.len() as u64 != metadata.size {
        return Err(bad_object());
    }
    validate(bytes, mime)
        .await
        .map_err(|source| bad_object().with_source(source))?;
    Ok(())
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
pub(crate) async fn project(
    record: Convention,
    s3: Option<&S3Client>,
) -> Result<ConventionDTO, AppError> {
    let payment_methods = record.methods()?;
    let preview = if let Some(location) = &record.logo_url {
        if let Some(s3) = s3 {
            match preview(s3, location).await {
                Ok(signed) => Some(signed),
                Err(error) => {
                    crate::error::log_background_error("Convention logo preview signing", &error);
                    None
                }
            }
        } else {
            tracing::warn!("Convention logo preview unavailable: S3 client unavailable");
            None
        }
    } else {
        None
    };
    let (logo_preview_url, logo_preview_url_expires_at) = preview.map_or((None, None), |p| {
        (Some(p.url), Some(p.expires_at.to_rfc3339()))
    });
    Ok(ConventionDTO {
        id: record.id.to_string(),
        slug: record.slug,
        name: record.name,
        logo_url: record.logo_url,
        logo_preview_url,
        logo_preview_url_expires_at,
        country_code: record.country_code,
        event_starts_on: record.event_starts_on.to_string(),
        event_ends_on: record.event_ends_on.to_string(),
        description_markdown: record.description_markdown,
        preorder_enabled: record.preorder_enabled,
        currency_code: record.currency_code,
        payment_methods,
        created_at: record.created_at.to_rfc3339(),
        updated_at: record.updated_at.to_rfc3339(),
    })
}

async fn preview(
    s3: &S3Client,
    stored_location: &str,
) -> Result<crate::client::s3::PresignedDownload, AppError> {
    let mime = location(stored_location, None)?;
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
