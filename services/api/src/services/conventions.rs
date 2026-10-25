pub(crate) mod logo;
pub(crate) mod money;
use crate::{
    client::{s3::S3Client, shopify::ShopifyClient},
    error::{AppError, upstream},
    models::{
        CountryCode,
        conventions::{
            CheckedConventionPatch, Convention, ConventionInput, ConventionVariant,
            SelectedVariant, VariantDisplay,
        },
        shopify::{ProductStatus, ShopifyId},
    },
    repositories, transformers, validations,
};
use axum::http::StatusCode;
use domain::dto::{
    ConventionDTO, ConventionListResponse, ConventionPaymentMethod, ConventionVariantInput,
    ConventionVariantListResponse, ConventionVariantsResponse, PatchField,
};
use sqlx::PgPool;
use uuid::Uuid;
pub(crate) fn options() -> domain::dto::ConventionOptionsResponse {
    let mut countries = Vec::new();
    for first in b'A'..=b'Z' {
        for second in b'A'..=b'Z' {
            let code = String::from_utf8_lossy(&[first, second]).into_owned();
            if !matches!(CountryCode::from(code.clone()), CountryCode::Other(_)) {
                countries.push(code);
            }
        }
    }
    let mut currencies = Vec::new();
    for first in b'A'..=b'Z' {
        for second in b'A'..=b'Z' {
            for third in b'A'..=b'Z' {
                let code = String::from_utf8_lossy(&[first, second, third]).into_owned();
                if money::precision(&code).is_err() {
                    continue;
                }
                let payment_methods = [
                    ConventionPaymentMethod::Stripe,
                    ConventionPaymentMethod::Paypal,
                    ConventionPaymentMethod::Promptpay,
                ]
                .into_iter()
                .filter(|method| money::methods(&code, std::slice::from_ref(method)).is_ok())
                .collect();
                currencies.push(domain::dto::ConventionCurrencyOption {
                    code,
                    payment_methods,
                });
            }
        }
    }
    domain::dto::ConventionOptionsResponse {
        countries,
        currencies,
    }
}
pub(crate) async fn list(
    pool: &PgPool,
    s3: &S3Client,
    page: u32,
    per_page: u32,
    enabled: Option<bool>,
) -> Result<ConventionListResponse, AppError> {
    let mut records = repositories::conventions::list(pool, page, per_page, enabled).await?;
    let has_next = records.len() > per_page as usize;
    records.truncate(per_page as usize);
    let mut items = Vec::with_capacity(records.len());
    for record in records {
        items.push(logo::project(record, s3).await?);
    }
    Ok(ConventionListResponse {
        items,
        page,
        per_page,
        has_next,
    })
}
pub(crate) async fn get(pool: &PgPool, id: Uuid) -> Result<Convention, AppError> {
    repositories::conventions::find(pool, id)
        .await?
        .ok_or_else(missing)
}
pub(crate) async fn create(
    pool: &PgPool,
    s3: Result<&S3Client, AppError>,
    input: &ConventionInput,
) -> Result<ConventionDTO, AppError> {
    methods(input)?;
    let s3 = s3?;
    if let Some(location) = &input.logo_url {
        logo::attach(s3, location, &input.slug).await?;
    }
    logo::project(repositories::conventions::insert(pool, input).await?, s3).await
}
pub(crate) async fn list_variants(
    pool: &PgPool,
    id: Uuid,
    page: u32,
    per_page: u32,
) -> Result<ConventionVariantListResponse, AppError> {
    let convention = get(pool, id).await?;
    let records = repositories::conventions::variants(pool, id).await?;
    let offset = u64::from(page - 1) * u64::from(per_page);
    let offset = usize::try_from(offset).unwrap_or(usize::MAX);
    let mut items = records
        .into_iter()
        .skip(offset)
        .take(per_page as usize + 1)
        .map(|record| variant_response(record, &convention.currency_code))
        .collect::<Result<Vec<_>, _>>()?;
    let has_next = items.len() > per_page as usize;
    items.truncate(per_page as usize);
    Ok(ConventionVariantListResponse {
        items,
        page,
        per_page,
        has_next,
    })
}
pub(crate) async fn patch(
    pool: &PgPool,
    s3: &S3Client,
    shopify: Result<&ShopifyClient, AppError>,
    id: Uuid,
    patch: CheckedConventionPatch,
) -> Result<ConventionDTO, AppError> {
    let prepared = prepare_patch(pool, s3, id, patch).await?;
    if !prepared.checks.is_empty() {
        validate_selection(shopify?, &prepared.checks).await?;
    }
    finish_patch(pool, s3, prepared).await
}

pub(crate) async fn replace_variants(
    pool: &PgPool,
    shopify: Result<&ShopifyClient, AppError>,
    id: Uuid,
    items: Vec<ConventionVariantInput>,
) -> Result<ConventionVariantsResponse, AppError> {
    let prepared = prepare_save(pool, id, items).await?;
    let validated = if prepared.checks.is_empty() {
        vec![]
    } else {
        validate_selection(shopify?, &prepared.checks).await?
    };
    finish_save(pool, prepared, validated).await
}

pub(crate) async fn delete(pool: &PgPool, id: Uuid) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    repositories::conventions::lock(&mut tx, id)
        .await?
        .ok_or_else(missing)?;
    repositories::conventions::delete(&mut tx, id).await?;
    tx.commit().await?;
    Ok(())
}
struct PreparedPatch {
    id: Uuid,
    patch: CheckedConventionPatch,
    observed: Convention,
    selection: Vec<ConventionVariantInput>,
    checks: Vec<ConventionVariantInput>,
}
async fn prepare_patch(
    pool: &PgPool,
    s3: &S3Client,
    id: Uuid,
    patch: CheckedConventionPatch,
) -> Result<PreparedPatch, AppError> {
    let observed = get(pool, id).await?;
    let input = merge(patch.clone(), &observed)?;
    methods(&input)?;
    if let PatchField::Set(location) = &patch.logo_url {
        logo::attach(s3, location, &observed.slug).await?;
    }
    let selection = inputs(repositories::conventions::variants(pool, id).await?);
    let enabling = matches!(patch.preorder_enabled, PatchField::Set(true));
    let changing_methods = !patch.payment_methods.is_missing();
    if enabling || changing_methods {
        prices(&selection, &input.currency_code, &input.payment_methods)?;
    }
    if enabling && selection.is_empty() {
        return Err(conflict(
            "Select eligible variants before enabling preorders",
        ));
    }
    let checks = if enabling { selection.clone() } else { vec![] };
    Ok(PreparedPatch {
        id,
        patch,
        observed,
        selection,
        checks,
    })
}
async fn finish_patch(
    pool: &PgPool,
    s3: &S3Client,
    prepared: PreparedPatch,
) -> Result<ConventionDTO, AppError> {
    let mut tx = pool.begin().await?;
    let current = repositories::conventions::lock(&mut tx, prepared.id)
        .await?
        .ok_or_else(missing)?;
    let input = merge(prepared.patch.clone(), &current)?;
    methods(&input)?;
    let selection = inputs(repositories::conventions::variants(&mut *tx, prepared.id).await?);
    if matches!(prepared.patch.preorder_enabled, PatchField::Set(true)) {
        if !same_selection(&selection, &prepared.selection)
            || current.currency_code != prepared.observed.currency_code
            || current.payment_methods != prepared.observed.payment_methods
        {
            return Err(conflict(
                "Convention selection or payment methods changed during validation; retry",
            ));
        }
        if selection.is_empty() {
            return Err(conflict(
                "Select eligible variants before enabling preorders",
            ));
        }
    }
    if !prepared.patch.payment_methods.is_missing() {
        prices(&selection, &input.currency_code, &input.payment_methods)?;
    }
    let saved = repositories::conventions::update(&mut tx, prepared.id, &input).await?;
    tx.commit().await?;
    logo::project(saved, s3).await
}
struct PreparedSave {
    id: Uuid,
    observed: Convention,
    items: Vec<ConventionVariantInput>,
    checks: Vec<ConventionVariantInput>,
}
async fn prepare_save(
    pool: &PgPool,
    id: Uuid,
    items: Vec<ConventionVariantInput>,
) -> Result<PreparedSave, AppError> {
    let observed = get(pool, id).await?;
    let items = prices(&items, &observed.currency_code, &observed.methods()?)?;
    let existing = inputs(repositories::conventions::variants(pool, id).await?);
    if observed.preorder_enabled && items.is_empty() {
        return Err(conflict(
            "Disable preorders before clearing selected variants",
        ));
    }
    let checks: Vec<_> = items
        .iter()
        .filter(|item| observed.preorder_enabled || !existing.contains(item))
        .cloned()
        .collect();
    Ok(PreparedSave {
        id,
        observed,
        items,
        checks,
    })
}
async fn validate_selection(
    client: &ShopifyClient,
    items: &[ConventionVariantInput],
) -> Result<Vec<SelectedVariant>, AppError> {
    let mut validated = Vec::with_capacity(items.len());
    for item in items {
        let id = ShopifyId::new(&item.shopify_variant_id)
            .map_err(|_| validations::conventions::invalid("Invalid Shopify variant ID"))?;
        let remote = client
            .convention_variant(&id)
            .await
            .map_err(|error| upstream(error))?
            .ok_or_else(|| validations::conventions::invalid("Shopify variant no longer exists"))?;
        if remote.product.id.as_ref() != item.shopify_product_id {
            return Err(validations::conventions::invalid(
                "Shopify variant does not belong to the selected product",
            ));
        }
        if !matches!(remote.product.status, ProductStatus::Active) || !remote.inventory_item.tracked
        {
            return Err(validations::conventions::invalid(
                "Shopify variant is not eligible for preorders",
            ));
        }
        validated.push(SelectedVariant {
            input: item.clone(),
            display: VariantDisplay {
                inventory_policy: Some(remote.inventory_policy),
                product_title: Some(remote.product.title),
                variant_title: Some(remote.title),
                sku: remote.sku,
                product_image: remote.product.featured_image,
                variant_image: remote.image,
            },
        });
    }
    Ok(validated)
}
async fn finish_save(
    pool: &PgPool,
    prepared: PreparedSave,
    validated: Vec<SelectedVariant>,
) -> Result<ConventionVariantsResponse, AppError> {
    let mut tx = pool.begin().await?;
    let current = repositories::conventions::lock(&mut tx, prepared.id)
        .await?
        .ok_or_else(missing)?;
    if current.currency_code != prepared.observed.currency_code
        || current.payment_methods != prepared.observed.payment_methods
    {
        return Err(conflict(
            "Convention payment methods changed during validation; retry",
        ));
    }
    let existing_records = repositories::conventions::variants(&mut *tx, prepared.id).await?;
    let existing: Vec<_> = existing_records
        .iter()
        .map(ConventionVariant::input)
        .collect();
    if current.preorder_enabled && prepared.items.is_empty() {
        return Err(conflict(
            "Disable preorders before clearing selected variants",
        ));
    }
    if prepared.items.iter().any(|item| {
        (current.preorder_enabled || !existing.contains(item)) && !prepared.checks.contains(item)
    }) {
        return Err(conflict(
            "Convention selection changed during validation; retry",
        ));
    }
    let items = prepared
        .items
        .into_iter()
        .map(|input| {
            let display = validated
                .iter()
                .find(|item| item.input == input)
                .map(|item| Ok(item.display.clone()))
                .or_else(|| {
                    existing_records
                        .iter()
                        .find(|item| item.input() == input)
                        .map(ConventionVariant::display)
                })
                .ok_or_else(|| {
                    conflict("Convention selection changed during validation; retry")
                })??;
            Ok(SelectedVariant { input, display })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let records = repositories::conventions::replace(&mut tx, prepared.id, &items).await?;
    tx.commit().await?;
    Ok(ConventionVariantsResponse {
        items: records
            .into_iter()
            .map(|record| variant_response(record, &current.currency_code))
            .collect::<Result<_, _>>()?,
    })
}
fn merge(value: CheckedConventionPatch, current: &Convention) -> Result<ConventionInput, AppError> {
    let logo_url = match value.logo_url {
        PatchField::Missing => current.logo_url.clone(),
        PatchField::Clear => None,
        PatchField::Set(value) => Some(value),
    };
    let mut input = ConventionInput {
        slug: current.slug.clone(),
        name: field(value.name, current.name.clone())?,
        logo_url,
        country_code: field(value.country_code, current.country_code.clone())?,
        event_starts_on: field(value.event_starts_on, current.event_starts_on)?,
        event_ends_on: field(value.event_ends_on, current.event_ends_on)?,
        description_markdown: field(
            value.description_markdown,
            current.description_markdown.clone(),
        )?,
        currency_code: current.currency_code.clone(),
        payment_methods: field(value.payment_methods, current.methods()?)?,
        preorder_enabled: field(value.preorder_enabled, current.preorder_enabled)?,
    };
    validations::conventions::slug(&input.slug)?;
    if input.event_starts_on > input.event_ends_on {
        return Err(validations::conventions::invalid(
            "event_starts_on must not follow event_ends_on",
        ));
    }
    input.country_code = validations::conventions::country_code(input.country_code)?;
    validations::conventions::description(
        &input.description_markdown,
        "description_markdown must contain 1 to 20000 nonblank UTF-8 bytes without NUL",
    )?;
    input.name = validations::common::name(&input.name)?;
    Ok(input)
}
fn field<T>(patch: PatchField<T>, current: T) -> Result<T, AppError> {
    match patch {
        PatchField::Missing => Ok(current),
        PatchField::Set(value) => Ok(value),
        PatchField::Clear => Err(validations::conventions::invalid(
            "Only logo_url may be cleared",
        )),
    }
}
fn variant_response(
    record: ConventionVariant,
    currency: &str,
) -> Result<domain::dto::ConventionVariantDTO, AppError> {
    let display = record.display()?;
    let price = money::canonical(&record.final_price, currency)
        .map_err(|error| AppError::from(anyhow::Error::new(error)))?;
    Ok(transformers::conventions::variant(record, display, price))
}
fn methods(input: &ConventionInput) -> Result<(), AppError> {
    money::methods(&input.currency_code, &input.payment_methods)
        .map_err(|_| validations::conventions::invalid("Invalid currency or payment methods"))
}
fn prices(
    items: &[ConventionVariantInput],
    code: &str,
    methods: &[ConventionPaymentMethod],
) -> Result<Vec<ConventionVariantInput>, AppError> {
    items.iter().map(|item| {
        let final_price = money::validate(&item.final_price, code, methods).map_err(|_| {
            validations::conventions::invalid("final_price must be positive, bounded and compatible with currency/payment precision")
        })?;
        Ok(ConventionVariantInput {final_price, ..item.clone()})
    }).collect()
}
fn inputs(records: Vec<ConventionVariant>) -> Vec<ConventionVariantInput> {
    records.into_iter().map(|r| r.input()).collect()
}
fn same_selection(a: &[ConventionVariantInput], b: &[ConventionVariantInput]) -> bool {
    a.len() == b.len() && a.iter().all(|item| b.contains(item))
}
fn missing() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "Convention not found")
}
fn conflict(message: &'static str) -> AppError {
    AppError::new(StatusCode::CONFLICT, message)
}

#[cfg(test)]
#[path = "../tests/services/conventions.rs"]
mod tests;
