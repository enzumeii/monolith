use crate::{
    error::AppError,
    models::{
        CountryCode,
        conventions::{Convention, ConventionInput},
        shopify::ShopifyId,
    },
};
use axum::{
    extract::{
        Json, Path, Query,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::StatusCode,
};
use chrono::NaiveDate;
use domain::dto::{
    ConventionVariantInput, CreateConvention, PatchConvention, PatchField,
    ReplaceConventionVariants,
};
pub(crate) fn body<T>(value: Result<Json<T>, JsonRejection>) -> Result<T, AppError> {
    value
        .map(|Json(v)| v)
        .map_err(|_| invalid("Invalid convention request body"))
}
pub(crate) fn query<T>(value: Result<Query<T>, QueryRejection>) -> Result<T, AppError> {
    value
        .map(|Query(v)| v)
        .map_err(|_| invalid("Invalid convention query"))
}
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
pub(crate) fn id(value: Result<Path<String>, PathRejection>) -> Result<uuid::Uuid, AppError> {
    let Path(v) = value.map_err(|_| invalid("Invalid convention identifier"))?;
    v.parse()
        .map_err(|_| invalid("Invalid convention identifier"))
}
pub(crate) fn create(value: CreateConvention) -> Result<ConventionInput, AppError> {
    slug(&value.slug)?;
    let event_starts_on = date(&value.event_starts_on)?;
    let event_ends_on = date(&value.event_ends_on)?;
    if event_starts_on > event_ends_on {
        return Err(invalid("event_starts_on must not follow event_ends_on"));
    }
    let country_code = CountryCode::from(value.country_code);
    if matches!(country_code, CountryCode::Other(_)) {
        return Err(invalid(
            "country_code must be an assigned ISO 3166 alpha-2 code",
        ));
    }
    if value.description_markdown.trim().is_empty()
        || value.description_markdown.len() > 20000
        || value.description_markdown.contains('\0')
    {
        return Err(invalid(
            "description_markdown must contain 1 to 20000 nonblank UTF-8 bytes without NUL",
        ));
    }
    if let Some(url) = &value.logo_url {
        logo(url)?;
    }
    crate::services::conventions::money::methods(&value.currency_code, &value.payment_methods)
        .map_err(|_| invalid("Invalid currency or payment methods"))?;
    Ok(ConventionInput {
        slug: value.slug,
        name: super::cron::configs::validate_name(&value.name)?,
        logo_url: value.logo_url,
        country_code: country_code.into(),
        event_starts_on,
        event_ends_on,
        description_markdown: value.description_markdown,
        currency_code: value.currency_code,
        payment_methods: value.payment_methods,
        preorder_enabled: false,
    })
}
pub(crate) fn patch_shape(value: &PatchConvention) -> Result<(), AppError> {
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
    if let PatchField::Set(v) = &value.name {
        super::cron::configs::validate_name(v)?;
    }
    if let PatchField::Set(v) = &value.logo_url {
        logo(v)?;
    }
    if let PatchField::Set(v) = &value.country_code
        && matches!(CountryCode::from(v.clone()), CountryCode::Other(_))
    {
        return Err(invalid(
            "country_code must be an assigned ISO 3166 alpha-2 code",
        ));
    }
    for v in [&value.event_starts_on, &value.event_ends_on] {
        if let PatchField::Set(v) = v {
            date(v)?;
        }
    }
    if let PatchField::Set(v) = &value.description_markdown
        && (v.trim().is_empty() || v.len() > 20000 || v.contains('\0'))
    {
        return Err(invalid("Invalid description_markdown"));
    }
    if let PatchField::Set(v) = &value.payment_methods
        && (v.is_empty() || v.iter().enumerate().any(|(i, m)| v[..i].contains(m)))
    {
        return Err(invalid("Payment methods must be nonempty and unique"));
    }
    Ok(())
}
pub(crate) fn merge(
    value: PatchConvention,
    current: &Convention,
) -> Result<ConventionInput, AppError> {
    let logo_url = match value.logo_url {
        PatchField::Missing => current.logo_url.clone(),
        PatchField::Clear => None,
        PatchField::Set(v) => Some(v),
    };
    let enabled = field(value.preorder_enabled, current.preorder_enabled)?;
    let mut merged = create(CreateConvention {
        slug: current.slug.clone(),
        name: field(value.name, current.name.clone())?,
        logo_url,
        country_code: field(value.country_code, current.country_code.clone())?,
        event_starts_on: field(value.event_starts_on, current.event_starts_on.to_string())?,
        event_ends_on: field(value.event_ends_on, current.event_ends_on.to_string())?,
        description_markdown: field(
            value.description_markdown,
            current.description_markdown.clone(),
        )?,
        currency_code: current.currency_code.clone(),
        payment_methods: field(value.payment_methods, current.methods()?)?,
    })?;
    merged.preorder_enabled = enabled;
    Ok(merged)
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
pub(crate) fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}
fn slug(v: &str) -> Result<(), AppError> {
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
fn date(v: &str) -> Result<NaiveDate, AppError> {
    let parsed = NaiveDate::parse_from_str(v, "%Y-%m-%d")
        .map_err(|_| invalid("Dates must use valid YYYY-MM-DD values"))?;
    if parsed.format("%Y-%m-%d").to_string() != v || v.len() != 10 {
        return Err(invalid("Dates must use valid YYYY-MM-DD values"));
    }
    Ok(parsed)
}
fn logo(v: &str) -> Result<(), AppError> {
    let u =
        reqwest::Url::parse(v).map_err(|_| invalid("logo_url must be an absolute HTTPS URL"))?;
    if v.len() > 2048
        || v.trim() != v
        || v.chars().any(char::is_control)
        || u.scheme() != "https"
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.fragment().is_some()
    {
        return Err(invalid(
            "logo_url must be HTTPS without credentials or fragment, up to 2048 bytes",
        ));
    }
    Ok(())
}
fn field<T>(patch: PatchField<T>, current: T) -> Result<T, AppError> {
    match patch {
        PatchField::Missing => Ok(current),
        PatchField::Set(v) => Ok(v),
        PatchField::Clear => Err(invalid("Only logo_url may be cleared")),
    }
}

#[cfg(test)]
#[path = "../tests/validations/conventions.rs"]
mod tests;
