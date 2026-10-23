use crate::{
    client::s3::S3Client, error::AppError, models::conventions::Convention,
    storage::convention_logo as storage,
};
use domain::dto::ConventionDTO;

pub(crate) async fn attach(
    s3: Option<&S3Client>,
    location: &str,
    slug: &str,
) -> Result<(), AppError> {
    storage::location(location, Some(slug))?;
    let s3 = s3.ok_or_else(|| {
        AppError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "S3 client unavailable",
        )
    })?;
    storage::validate_attachment(s3, location, slug).await
}

pub(crate) async fn project(
    record: Convention,
    s3: Option<&S3Client>,
) -> Result<ConventionDTO, AppError> {
    let payment_methods = record.methods()?;
    let preview = if let Some(location) = &record.logo_url {
        if let Some(s3) = s3 {
            match storage::preview(s3, location).await {
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
