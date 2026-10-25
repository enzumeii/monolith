use crate::{
    error::AppError,
    models::{
        CountryCode,
        conventions::{CheckedConventionCreate, CheckedConventionPatch},
        shopify::ShopifyId,
    },
};
use axum::http::StatusCode;
use chrono::NaiveDate;
use domain::dto::{
    ConventionVariantInput, CreateConvention, PatchConvention, PatchField,
    ReplaceConventionVariants,
};
pub(crate) fn no_query(query: Option<&str>) -> Result<(), AppError> {
    if query.is_some_and(|query| !query.is_empty()) {
        return Err(invalid(
            "This convention request does not accept query parameters",
        ));
    }
    Ok(())
}
pub(crate) fn delete_body(
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<(), AppError> {
    let body = body.map_err(|_| invalid("Invalid convention request body"))?;
    if !body.is_empty() {
        return Err(invalid("DELETE convention requests must not have a body"));
    }
    Ok(())
}
pub(crate) fn create(mut value: CreateConvention) -> Result<CheckedConventionCreate, AppError> {
    slug(&value.slug)?;
    let event_starts_on = date(&value.event_starts_on)?;
    let event_ends_on = date(&value.event_ends_on)?;
    if event_starts_on > event_ends_on {
        return Err(invalid("event_starts_on must not follow event_ends_on"));
    }
    value.country_code = country_code(value.country_code)?;
    description(
        &value.description_markdown,
        "description_markdown must contain 1 to 20000 nonblank UTF-8 bytes without NUL",
    )?;
    if let Some(url) = &value.logo_url {
        logo(url)?;
    }
    value.name = super::common::name(&value.name)?;
    Ok(CheckedConventionCreate {
        request: value,
        event_starts_on,
        event_ends_on,
    })
}

pub(crate) fn patch(value: PatchConvention) -> Result<CheckedConventionPatch, AppError> {
    let fields = [
        value.name.is_missing(),
        value.logo_url.is_missing(),
        value.country_code.is_missing(),
        value.event_starts_on.is_missing(),
        value.event_ends_on.is_missing(),
        value.description_markdown.is_missing(),
        value.payment_methods.is_missing(),
        value.preorder_enabled.is_missing(),
    ];
    if fields.into_iter().all(|v| v) {
        return Err(invalid("PATCH body must contain at least one field"));
    }
    let clear = matches!(value.name, PatchField::Clear)
        || matches!(value.country_code, PatchField::Clear)
        || matches!(value.event_starts_on, PatchField::Clear)
        || matches!(value.event_ends_on, PatchField::Clear)
        || matches!(value.description_markdown, PatchField::Clear)
        || matches!(value.payment_methods, PatchField::Clear)
        || matches!(value.preorder_enabled, PatchField::Clear);
    if clear {
        return Err(invalid("Only logo_url may be cleared"));
    }
    // Validate supplied fields before any database read; date ordering is checked after merge.
    let name = map_field(value.name, |value| super::common::name(&value))?;
    if let PatchField::Set(v) = &value.logo_url {
        logo(v)?;
    }
    let country_code = map_field(value.country_code, country_code)?;
    let event_starts_on = map_field(value.event_starts_on, |value| date(&value))?;
    let event_ends_on = map_field(value.event_ends_on, |value| date(&value))?;
    if let PatchField::Set(value) = &value.description_markdown {
        description(value, "Invalid description_markdown")?;
    }
    if let PatchField::Set(v) = &value.payment_methods
        && (v.is_empty() || v.iter().enumerate().any(|(i, m)| v[..i].contains(m)))
    {
        return Err(invalid("Payment methods must be nonempty and unique"));
    }
    Ok(CheckedConventionPatch {
        name,
        logo_url: value.logo_url,
        country_code,
        event_starts_on,
        event_ends_on,
        description_markdown: value.description_markdown,
        payment_methods: value.payment_methods,
        preorder_enabled: value.preorder_enabled,
    })
}

pub(crate) fn variants(
    value: ReplaceConventionVariants,
) -> Result<Vec<ConventionVariantInput>, AppError> {
    if value.items.len() > 250 {
        return Err(invalid("At most 250 convention variants are allowed"));
    }
    let mut seen = std::collections::HashSet::new();
    for item in &value.items {
        for (value, kind) in [
            (&item.shopify_product_id, "Product"),
            (&item.shopify_variant_id, "ProductVariant"),
        ] {
            ShopifyId::new(value)
                .and_then(|id| id.number(kind))
                .map_err(|_| {
                    invalid("Expected positive Shopify Product and ProductVariant GIDs")
                })?;
        }
        if !seen.insert(&item.shopify_variant_id) {
            return Err(invalid("Duplicate Shopify variant"));
        }
        // Currency-specific precision is checked against the persisted convention before remote calls.
        if item.final_price.is_empty()
            || item.final_price.len() > 24
            || !item
                .final_price
                .bytes()
                .all(|c| c.is_ascii_digit() || c == b'.')
        {
            return Err(invalid(
                "final_price must be an unsigned plain decimal string",
            ));
        }
    }
    Ok(value.items)
}
pub(crate) fn country_code(value: String) -> Result<String, AppError> {
    let country = CountryCode::from(value);
    if matches!(country, CountryCode::Other(_)) {
        return Err(invalid(
            "country_code must be an assigned ISO 3166 alpha-2 code",
        ));
    }
    Ok(country.into())
}

pub(crate) fn description(value: &str, message: &'static str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > 20000 || value.contains('\0') {
        return Err(invalid(message));
    }
    Ok(())
}

pub(crate) fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}
pub(crate) fn slug(v: &str) -> Result<(), AppError> {
    if !(1..=63).contains(&v.len())
        || !v.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
        || [
            "api",
            "v1",
            "healthz",
            "_cron",
            "backoffice",
            "user",
            "assets",
        ]
        .contains(&v)
    {
        return Err(invalid("Invalid or reserved convention slug"));
    }
    Ok(())
}
pub(crate) fn logo_location(value: &str) -> Result<&'static str, AppError> {
    let (directory, filename) = value
        .rsplit_once('/')
        .ok_or_else(|| invalid("Invalid logo_url"))?;
    super::files::folder(directory)?;
    let (id, extension) = filename
        .rsplit_once('.')
        .ok_or_else(|| invalid("Invalid logo_url"))?;
    let asset: uuid::Uuid = id.parse().map_err(|_| invalid("Invalid logo_url"))?;
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

pub(crate) fn date(v: &str) -> Result<NaiveDate, AppError> {
    let parsed = NaiveDate::parse_from_str(v, "%Y-%m-%d")
        .map_err(|_| invalid("Dates must use valid YYYY-MM-DD values"))?;
    if parsed.format("%Y-%m-%d").to_string() != v || v.len() != 10 {
        return Err(invalid("Dates must use valid YYYY-MM-DD values"));
    }
    Ok(parsed)
}
fn logo(v: &str) -> Result<(), AppError> {
    logo_location(v).map(|_| ())
}
fn map_field<T, U>(
    value: PatchField<T>,
    map: impl FnOnce(T) -> Result<U, AppError>,
) -> Result<PatchField<U>, AppError> {
    match value {
        PatchField::Missing => Ok(PatchField::Missing),
        PatchField::Clear => Ok(PatchField::Clear),
        PatchField::Set(value) => map(value).map(PatchField::Set),
    }
}

#[cfg(test)]
#[path = "../tests/validations/conventions.rs"]
mod tests;
