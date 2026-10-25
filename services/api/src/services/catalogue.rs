use crate::{
    client::shopify::ShopifyClient,
    error::{AppError, upstream},
    models::shopify::{PageRequest, ShopifyId, catalogue::SelectionVariant},
    services, transformers,
};
use domain::dto::{CatalogueProduct, CatalogueResponse, CatalogueVariant};

pub async fn products(
    client: &ShopifyClient,
    location_id: ShopifyId,
    page: PageRequest,
    search: Option<&str>,
) -> Result<CatalogueResponse, AppError> {
    services::inventories::validate_location(client, &location_id).await?;
    let search = search.map_or_else(
        || "status:active AND tracks_inventory:true".to_owned(),
        |search| format!("status:active AND tracks_inventory:true AND ({search})"),
    );
    let response = client
        .catalogue_products(&location_id, Some(&search), &page)
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
        let mut variants = Vec::new();
        for variant in product
            .variants
            .into_complete_nodes()
            .map_err(|error| upstream(error))?
        {
            let tracked = variant.inventory_item.tracked;
            let variant = selection_variant(variant)?;
            if tracked && variant.eligible {
                variants.push(variant);
            }
        }
        if variants.is_empty() {
            continue;
        }
        products.push(CatalogueProduct {
            id: product.id.as_ref().to_owned(),
            title: product.title,
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

fn selection_variant(variant: SelectionVariant) -> Result<CatalogueVariant, AppError> {
    let quantity = variant
        .available_quantity()
        .map_err(|error| upstream(error))?;
    Ok(transformers::catalogue::variant(
        variant,
        quantity,
        quantity.is_some(),
    ))
}
