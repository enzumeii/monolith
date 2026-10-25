use super::validate_location;
use crate::{
    client::{ClientError, ClientErrorKind, shopify::ShopifyClient},
    error::{AppError, client_timeout, log_background_error, upstream},
    models::shopify::{
        ActivateInventoryRequest, InventoryActivationCandidate, InventoryDiscoveryRequest,
        PageRequest, ShopifyId,
    },
    services,
};
use axum::http::StatusCode;
use domain::dto::{InventoryEnableError, InventoryEnableResponse};
use std::collections::HashMap;

pub async fn enable_inventory(
    client: &crate::client::shopify::ShopifyClient,
    location_id: ShopifyId,
) -> Result<InventoryEnableResponse, AppError> {
    validate_location(client, &location_id).await?;
    let items = discover(client, &location_id).await?;
    let mut result = InventoryEnableResponse {
        total: items.len(),
        success: 0,
        failed: 0,
        errors: Vec::new(),
    };
    for item in items {
        match activate(client, &location_id, &item).await {
            Ok(()) => result.success += 1,
            Err(error) => {
                let reason = if client_timeout(&error) {
                    "Inventory activation timed out; outcome could not be confirmed"
                } else if matches!(error.kind(), ClientErrorKind::Mutation { .. }) {
                    "Inventory activation rejected"
                } else {
                    "Inventory activation failed"
                };
                log_background_error("Inventory activation", &upstream(error));
                result.failed += 1;
                result.errors.push(InventoryEnableError {
                    product_id: item.product_id,
                    reason: reason.into(),
                });
            }
        }
    }
    Ok(result)
}

pub(crate) fn activation_item(
    candidate: &InventoryActivationCandidate,
) -> Result<Option<InventoryActivationItem>, ClientError> {
    if candidate.is_active || !candidate.item.tracked {
        return Ok(None);
    }
    let on_hand = candidate.quantity("on_hand")?;
    if on_hand <= 0 {
        return Ok(None);
    }
    candidate.item.id.numeric_id("InventoryItem")?;
    candidate.item.variant.id.numeric_id("ProductVariant")?;
    let product_id = candidate.item.variant.product.id.numeric_id("Product")?;
    Ok(Some(InventoryActivationItem {
        inventory_item_id: candidate.item.id.clone(),
        product_id,
    }))
}

pub(crate) struct InventoryActivationItem {
    inventory_item_id: ShopifyId,
    product_id: u64,
}

async fn discover(
    client: &ShopifyClient,
    location: &ShopifyId,
) -> Result<Vec<InventoryActivationItem>, AppError> {
    let request = InventoryDiscoveryRequest {
        location_id: location.clone(),
    };
    let mut seen = HashMap::new();
    let mut items = Vec::new();
    services::pagination::for_each_page(
        None,
        async |after| {
            client
                .find_inventory_to_activate(&request, &PageRequest { first: 250, after })
                .await
                .map_err(|error| upstream(error))?
                .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Shopify location not found"))
        },
        |candidates| {
            for candidate in candidates {
                if let Some(previous) = seen.get(&candidate.item.id) {
                    if previous != &candidate {
                        return Err(upstream(ClientError::new(ClientErrorKind::Protocol(
                            "conflicting inventory discovery entries",
                        ))));
                    }
                    continue;
                }
                if let Some(item) = activation_item(&candidate).map_err(|error| upstream(error))? {
                    items.push(item);
                }
                seen.insert(candidate.item.id.clone(), candidate);
            }
            Ok(())
        },
        |error| upstream(error),
    )
    .await?;
    Ok(items)
}

async fn activate(
    client: &ShopifyClient,
    location: &ShopifyId,
    item: &InventoryActivationItem,
) -> Result<(), ClientError> {
    let input = ActivateInventoryRequest {
        inventory_item_id: item.inventory_item_id.clone(),
        location_id: location.clone(),
        idempotency_key: uuid::Uuid::new_v4(),
    };
    let activation = client
        .activate_existing_inventory(&input)
        .await?
        .inventory_activate;
    if !activation.user_errors.is_empty() {
        return Err(ClientError::new(ClientErrorKind::Mutation {
            errors: activation.user_errors,
        }));
    }
    if activation
        .inventory_level
        .is_none_or(|level| !level.is_active)
    {
        return Err(ClientError::new(ClientErrorKind::Protocol(
            "inventory activation was not confirmed",
        )));
    }
    Ok(())
}
