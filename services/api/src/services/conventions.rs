pub(crate) mod logo;
pub(crate) mod money;
use crate::{
    client::{s3::S3Client, shopify::ShopifyClient},
    config::ConventionsConfig,
    error::{AppError, upstream},
    models::{
        conventions::{Convention, ConventionInput, ConventionVariant},
        shopify::{ProductStatus, ShopifyId},
    },
    repositories::conventions as repo,
    validations::conventions as validate,
};
use axum::http::StatusCode;
use domain::dto::{
    ConventionDTO, ConventionListResponse, ConventionPaymentMethod, ConventionVariantInput,
    ConventionVariantListResponse, ConventionVariantsResponse, PatchConvention, PatchField,
};
use sqlx::PgPool;
use uuid::Uuid;
pub(crate) async fn enabled_list(
    pool: &PgPool,
    s3: Option<&S3Client>,
    page: u32,
    per_page: u32,
) -> Result<ConventionListResponse, AppError> {
    let (page, per_page) =
        crate::validations::cron::configs::validate_page(Some(page), Some(per_page))?;
    list(pool, s3, page, per_page, Some(true)).await
}
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Shared enabled-only lookup for the preorder flow")
)]
pub(crate) async fn enabled_by_id(
    pool: &PgPool,
    s3: Option<&S3Client>,
    id: Uuid,
) -> Result<Option<ConventionDTO>, AppError> {
    match repo::find(pool, id, true).await? {
        Some(record) => Ok(Some(logo::project(record, s3).await?)),
        None => Ok(None),
    }
}
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Shared enabled-only lookup for the preorder flow")
)]
pub(crate) async fn enabled_by_slug(
    pool: &PgPool,
    s3: Option<&S3Client>,
    slug: &str,
) -> Result<Option<ConventionDTO>, AppError> {
    match repo::enabled_slug(pool, slug).await? {
        Some(record) => Ok(Some(logo::project(record, s3).await?)),
        None => Ok(None),
    }
}
pub(crate) async fn list(
    pool: &PgPool,
    s3: Option<&S3Client>,
    page: u32,
    per_page: u32,
    enabled: Option<bool>,
) -> Result<ConventionListResponse, AppError> {
    let mut records = repo::list(pool, page, per_page, enabled).await?;
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
    repo::find(pool, id, false).await?.ok_or_else(missing)
}
pub(crate) async fn create(
    pool: &PgPool,
    s3: Option<&S3Client>,
    config: &ConventionsConfig,
    input: &ConventionInput,
) -> Result<ConventionDTO, AppError> {
    capability(config, &input.currency_code, &input.payment_methods)?;
    if let Some(location) = &input.logo_location {
        logo::attach(s3, location, &input.slug).await?;
    }
    logo::project(repo::insert(pool, input).await?, s3).await
}
pub(crate) async fn list_variants(
    pool: &PgPool,
    id: Uuid,
    page: u32,
    per_page: u32,
) -> Result<ConventionVariantListResponse, AppError> {
    let convention = get(pool, id).await?;
    let records = repo::variants(pool, id).await?;
    let offset = u64::from(page - 1) * u64::from(per_page);
    let offset = usize::try_from(offset).unwrap_or(usize::MAX);
    let mut items = records
        .into_iter()
        .skip(offset)
        .take(per_page as usize + 1)
        .map(|v| v.into_dto(&convention.currency_code))
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
pub(crate) struct PreparedPatch {
    id: Uuid,
    patch: PatchConvention,
    observed: Convention,
    selection: Vec<ConventionVariantInput>,
    pub checks: Vec<ConventionVariantInput>,
}
pub(crate) async fn prepare_patch(
    pool: &PgPool,
    s3: Option<&S3Client>,
    config: &ConventionsConfig,
    id: Uuid,
    patch: PatchConvention,
) -> Result<PreparedPatch, AppError> {
    let observed = get(pool, id).await?;
    let input = validate::merge(patch.clone(), &observed)?;
    if let PatchField::Set(location) = &patch.logo_location {
        logo::attach(s3, location, &observed.slug).await?;
    }
    let selection = inputs(repo::variants(pool, id).await?);
    let enabling = matches!(patch.preorder_enabled, PatchField::Set(true));
    let changing_methods = !patch.payment_methods.is_missing();
    if enabling || changing_methods {
        capability(config, &input.currency_code, &input.payment_methods)?;
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
pub(crate) async fn finish_patch(
    pool: &PgPool,
    s3: Option<&S3Client>,
    config: &ConventionsConfig,
    prepared: PreparedPatch,
) -> Result<ConventionDTO, AppError> {
    let mut tx = pool.begin().await?;
    let current = repo::lock(&mut tx, prepared.id)
        .await?
        .ok_or_else(missing)?;
    let input = validate::merge(prepared.patch.clone(), &current)?;
    let selection = inputs(repo::variants(&mut *tx, prepared.id).await?);
    if matches!(prepared.patch.preorder_enabled, PatchField::Set(true)) {
        if !same_selection(&selection, &prepared.selection)
            || current.currency_code != prepared.observed.currency_code
            || current.payment_methods != prepared.observed.payment_methods
        {
            return Err(conflict(
                "Convention selection or payment methods changed during validation; retry",
            ));
        }
        capability(config, &input.currency_code, &input.payment_methods)?;
        if selection.is_empty() {
            return Err(conflict(
                "Select eligible variants before enabling preorders",
            ));
        }
    }
    if !prepared.patch.payment_methods.is_missing() {
        capability(config, &input.currency_code, &input.payment_methods)?;
        prices(&selection, &input.currency_code, &input.payment_methods)?;
    }
    let saved = repo::update(&mut tx, prepared.id, &input).await?;
    tx.commit().await?;
    logo::project(saved, s3).await
}
pub(crate) struct PreparedSave {
    id: Uuid,
    observed: Convention,
    pub items: Vec<ConventionVariantInput>,
    pub checks: Vec<ConventionVariantInput>,
}
pub(crate) async fn prepare_save(
    pool: &PgPool,
    config: &ConventionsConfig,
    id: Uuid,
    items: Vec<ConventionVariantInput>,
) -> Result<PreparedSave, AppError> {
    let observed = get(pool, id).await?;
    let items = prices(&items, &observed.currency_code, &observed.methods()?)?;
    let existing = inputs(repo::variants(pool, id).await?);
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
    if !checks.is_empty() {
        capability(config, &observed.currency_code, &observed.methods()?)?;
    }
    Ok(PreparedSave {
        id,
        observed,
        items,
        checks,
    })
}
pub(crate) async fn validate_selection(
    client: &ShopifyClient,
    items: &[ConventionVariantInput],
) -> Result<(), AppError> {
    for item in items {
        let id = ShopifyId::new(&item.shopify_variant_id)
            .map_err(|_| validate::invalid("Invalid Shopify variant ID"))?;
        let remote = client
            .convention_variant(&id)
            .await
            .map_err(|error| upstream(error))?
            .ok_or_else(|| validate::invalid("Shopify variant no longer exists"))?;
        if remote.product.id.as_ref() != item.shopify_product_id {
            return Err(validate::invalid(
                "Shopify variant does not belong to the selected product",
            ));
        }
        if !matches!(remote.product.status, ProductStatus::Active)
            || !remote.inventory_item.tracked
            || remote.inventory_policy != domain::dto::CatalogueInventoryPolicy::Deny
        {
            return Err(validate::invalid(
                "Shopify variant is not eligible for preorders",
            ));
        }
    }
    Ok(())
}
pub(crate) async fn finish_save(
    pool: &PgPool,
    config: &ConventionsConfig,
    prepared: PreparedSave,
) -> Result<ConventionVariantsResponse, AppError> {
    let mut tx = pool.begin().await?;
    let current = repo::lock(&mut tx, prepared.id)
        .await?
        .ok_or_else(missing)?;
    if current.currency_code != prepared.observed.currency_code
        || current.payment_methods != prepared.observed.payment_methods
    {
        return Err(conflict(
            "Convention payment methods changed during validation; retry",
        ));
    }
    let existing = inputs(repo::variants(&mut *tx, prepared.id).await?);
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
    if !prepared.checks.is_empty() {
        capability(config, &current.currency_code, &current.methods()?)?;
    }
    let records = repo::replace(&mut tx, prepared.id, &prepared.items).await?;
    tx.commit().await?;
    Ok(ConventionVariantsResponse {
        items: records
            .into_iter()
            .map(|v| v.into_dto(&current.currency_code))
            .collect::<Result<_, _>>()?,
    })
}
pub(crate) async fn delete(pool: &PgPool, id: Uuid) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    repo::lock(&mut tx, id).await?.ok_or_else(missing)?;
    repo::delete(&mut tx, id).await?;
    tx.commit().await?;
    Ok(())
}
fn capability(
    config: &ConventionsConfig,
    code: &str,
    methods: &[ConventionPaymentMethod],
) -> Result<(), AppError> {
    let allowed = config
        .currencies
        .iter()
        .find(|c| c.code == code)
        .ok_or_else(|| validate::invalid("Convention currency is not currently enabled"))?;
    if methods.is_empty() || methods.iter().any(|m| !allowed.payment_methods.contains(m)) {
        return Err(validate::invalid(
            "Convention payment method is not currently enabled for this currency",
        ));
    }
    Ok(())
}
fn prices(
    items: &[ConventionVariantInput],
    code: &str,
    methods: &[ConventionPaymentMethod],
) -> Result<Vec<ConventionVariantInput>, AppError> {
    items.iter().map(|item| {
        let final_price = money::validate(&item.final_price, code, methods).map_err(|_| {
            validate::invalid("final_price must be positive, bounded and compatible with currency/payment precision")
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
