use super::conventions::{logo, money};
use crate::{
    client::{s3::S3Client, shopify::ShopifyClient},
    error::{AppError, upstream},
    models::{
        conventions::{Convention, ConventionVariant},
        shopify::{ProductStatus, ShopifyId, variant_nodes::LiveVariant},
    },
    repositories, transformers, validations,
};
use axum::http::StatusCode;
use domain::dto::{
    CatalogueInventoryPolicy, ConventionQuote, ConventionQuoteLine, PublicCatalogueResponse,
    PublicConvention, PublicConventionListResponse, PublicProduct, PublicVariant,
    QuoteConventionRequest,
};
use sqlx::PgPool;

pub(crate) async fn directory(
    pool: &PgPool,
    s3: &S3Client,
    today: chrono::NaiveDate,
    country: Option<String>,
    page: u32,
    per_page: u32,
) -> Result<PublicConventionListResponse, AppError> {
    let mut records = repositories::conventions::public_directory(
        pool,
        today,
        country.as_deref(),
        page,
        per_page,
    )
    .await?;
    let has_next = records.len() > per_page as usize;
    records.truncate(per_page as usize);
    let mut items = Vec::with_capacity(records.len());
    for record in records {
        items.push(project(record, s3).await?);
    }
    Ok(PublicConventionListResponse {
        items,
        page,
        per_page,
        has_next,
        visitor_country: country,
    })
}

pub(crate) async fn detail(
    pool: &PgPool,
    s3: Result<&S3Client, AppError>,
    slug: &str,
) -> Result<PublicConvention, AppError> {
    project(enabled(pool, slug).await?, s3?).await
}

pub(crate) async fn products(
    pool: &PgPool,
    s3: Result<&S3Client, AppError>,
    shopify: Result<&ShopifyClient, AppError>,
    slug: &str,
) -> Result<PublicCatalogueResponse, AppError> {
    let convention = enabled(pool, slug).await?;
    let s3 = s3?;
    let selected = repositories::conventions::variants(pool, convention.id).await?;
    let live = current(shopify, &selected).await?;
    let (convention, selected) = recheck(pool, slug, &convention, &selected).await?;
    let mut products: Vec<PublicProduct> = Vec::new();
    for (saved, live) in selected.into_iter().zip(live) {
        let Some(live) = live else {
            continue;
        };
        if !eligible(&live, &saved) {
            continue;
        }
        let variant = variant(&saved, &live, &convention.currency_code)?;
        let id = live.product.id.as_ref();
        if let Some(product) = products.iter_mut().find(|product| product.id == id) {
            product.variants.push(variant);
        } else {
            products.push(transformers::public_conventions::product(live, variant));
        }
    }
    Ok(PublicCatalogueResponse {
        convention: project(convention, s3).await?,
        products,
    })
}

pub(crate) async fn quote(
    pool: &PgPool,
    shopify: Result<&ShopifyClient, AppError>,
    slug: &str,
    request: QuoteConventionRequest,
) -> Result<ConventionQuote, AppError> {
    let convention = enabled(pool, slug).await?;
    let selected = repositories::conventions::variants(pool, convention.id).await?;
    let requested = request
        .items
        .iter()
        .map(|item| {
            selected
                .iter()
                .find(|saved| saved.shopify_variant_id == item.variant_id)
                .cloned()
                .ok_or_else(|| {
                    validations::conventions::invalid(
                        "Cart variant does not belong to this convention",
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let live = current(shopify, &requested).await?;
    let (convention, selected) = recheck(pool, slug, &convention, &selected).await?;
    let code = &convention.currency_code;
    let mut subtotal = 0_u64;
    let mut items = Vec::with_capacity(request.items.len());
    for (item, live) in request.items.into_iter().zip(live) {
        let saved = selected
            .iter()
            .find(|saved| saved.shopify_variant_id == item.variant_id)
            .ok_or_else(conflict)?;
        let live = live
            .filter(|live| eligible(live, saved))
            .ok_or_else(conflict)?;
        let quantity = live.available_quantity().map_err(|error| upstream(error))?;
        if item.quantity > maximum(&live, quantity) {
            return Err(conflict());
        }
        let minor = money::minor(&saved.final_price, code).map_err(invalid_saved_money)?;
        let line_amount = minor
            .checked_mul(u64::from(item.quantity))
            .filter(|v| *v <= i64::MAX.cast_unsigned())
            .ok_or_else(|| {
                validations::conventions::invalid("Cart total exceeds the supported amount")
            })?;
        subtotal = subtotal
            .checked_add(line_amount)
            .filter(|v| *v <= i64::MAX.cast_unsigned())
            .ok_or_else(|| {
                validations::conventions::invalid("Cart total exceeds the supported amount")
            })?;
        items.push(ConventionQuoteLine {
            variant_id: item.variant_id,
            quantity: item.quantity,
            unit_price: money::amount(minor, code).map_err(invalid_saved_money)?,
            line_total: money::amount(line_amount, code).map_err(invalid_saved_money)?,
        });
    }
    let total = money::amount(subtotal, code).map_err(invalid_saved_money)?;
    Ok(ConventionQuote {
        currency_code: code.clone(),
        items,
        subtotal: total.clone(),
        shipping: money::amount(0, code).map_err(invalid_saved_money)?,
        total,
    })
}

async fn enabled(pool: &PgPool, slug: &str) -> Result<Convention, AppError> {
    repositories::conventions::public_find(pool, slug)
        .await?
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Convention unavailable"))
}

async fn project(record: Convention, s3: &S3Client) -> Result<PublicConvention, AppError> {
    let dto = logo::project(record, s3).await?;
    Ok(transformers::public_conventions::convention(dto))
}

async fn current(
    shopify: Result<&ShopifyClient, AppError>,
    selected: &[ConventionVariant],
) -> Result<Vec<Option<LiveVariant>>, AppError> {
    if selected.is_empty() {
        return Ok(vec![]);
    }
    let shopify = shopify?;
    let ids = selected
        .iter()
        .map(|saved| ShopifyId::new(&saved.shopify_variant_id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| upstream(error))?;
    shopify
        .variant_nodes(&ids)
        .await
        .map_err(|error| upstream(error))
}

async fn recheck(
    pool: &PgPool,
    slug: &str,
    before: &Convention,
    selected: &[ConventionVariant],
) -> Result<(Convention, Vec<ConventionVariant>), AppError> {
    let after = enabled(pool, slug).await?;
    let current = repositories::conventions::variants(pool, after.id).await?;
    if after.id != before.id
        || after.currency_code != before.currency_code
        || selected.len() != current.len()
        || selected
            .iter()
            .zip(&current)
            .any(|(a, b)| a.input() != b.input())
    {
        return Err(conflict());
    }
    Ok((after, current))
}

fn eligible(live: &LiveVariant, saved: &ConventionVariant) -> bool {
    matches!(live.product.status, ProductStatus::Active)
        && live.inventory_item.tracked
        && live.product.id.as_ref() == saved.shopify_product_id
}

fn maximum(live: &LiveVariant, quantity: i64) -> u32 {
    match live.inventory_policy {
        CatalogueInventoryPolicy::Continue => 999,
        CatalogueInventoryPolicy::Deny => u32::try_from(quantity.clamp(0, 999)).unwrap_or(0),
    }
}

fn variant(
    saved: &ConventionVariant,
    live: &LiveVariant,
    code: &str,
) -> Result<PublicVariant, AppError> {
    let quantity = live.available_quantity().map_err(|error| upstream(error))?;
    let final_price = money::canonical(&saved.final_price, code).map_err(invalid_saved_money)?;
    Ok(transformers::public_conventions::variant(
        live,
        final_price,
        quantity,
        maximum(live, quantity),
    ))
}

fn conflict() -> AppError {
    AppError::new(
        StatusCode::CONFLICT,
        "Cart selection or availability changed; refresh the catalogue",
    )
}

fn invalid_saved_money(error: money::InvalidMoney) -> AppError {
    AppError::from(anyhow::Error::new(error))
}
