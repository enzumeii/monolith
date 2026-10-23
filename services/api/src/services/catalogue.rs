use crate::{
    client::shopify::ShopifyClient,
    error::{AppError, upstream},
    models::shopify::{PageRequest, ProductStatus, ShopifyId, catalogue::SelectionVariant},
};
use axum::http::StatusCode;
use domain::dto::{
    CatalogueInventoryPolicy, CatalogueProduct, CatalogueResponse, CatalogueUnavailableReason,
    CatalogueVariant,
};

pub async fn products(
    client: &ShopifyClient,
    location_id: ShopifyId,
    page: PageRequest,
    search: Option<&str>,
) -> Result<CatalogueResponse, AppError> {
    super::inventories::validate_location(client, &location_id).await?;
    let response = client
        .catalogue_products(&location_id, search, &page)
        .await
        .map_err(|error| upstream(error))?;
    response
        .validate_after(page.after.as_deref())
        .map_err(|error| upstream(error))?;
    let mut products = Vec::with_capacity(response.nodes.len());
    for product in response.nodes {
        product
            .id
            .number("Product")
            .map_err(|error| upstream(error))?;
        let active = matches!(product.status, ProductStatus::Active);
        let mut variants = Vec::new();
        for variant in product.variants.into_complete_nodes().map_err(|error| {
            AppError::new(
                StatusCode::BAD_GATEWAY,
                "Product variants exceed the supported catalogue browse limit of 250",
            )
            .with_source(error)
        })? {
            variants.push(selection_variant(variant, active)?);
        }
        products.push(CatalogueProduct {
            id: product.id.as_ref().to_owned(),
            title: product.title,
            status: match product.status {
                ProductStatus::Active => "ACTIVE",
                ProductStatus::Draft => "DRAFT",
                ProductStatus::Archived => "ARCHIVED",
                ProductStatus::Unlisted => "UNLISTED",
            }
            .to_owned(),
            description: product.description,
            featured_image: product.featured_image,
            variants,
        });
    }
    Ok(CatalogueResponse {
        products,
        page_info: response.page_info,
    })
}

fn selection_variant(
    variant: SelectionVariant,
    active_product: bool,
) -> Result<CatalogueVariant, AppError> {
    use CatalogueUnavailableReason as Reason;
    let quantity = variant
        .available_quantity()
        .map_err(|error| upstream(error))?;
    let reason = if !active_product {
        Some(Reason::InactiveProduct)
    } else if !variant.inventory_item.tracked {
        Some(Reason::UntrackedInventory)
    } else if variant.inventory_policy == CatalogueInventoryPolicy::Continue {
        Some(Reason::ContinueInventoryPolicy)
    } else {
        match &variant.inventory_item.inventory_level {
            None => Some(Reason::MissingInventoryLevel),
            Some(level) if !level.is_active => Some(Reason::InactiveInventoryLevel),
            _ => None,
        }
    };
    Ok(variant.into_dto(quantity, reason))
}
