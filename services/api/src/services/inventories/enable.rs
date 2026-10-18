use super::validate_location;
use crate::{
    client::{ClientError, ClientErrorKind, shopify::ShopifyClient},
    error::{AppError, client_timeout, log_background_error},
    models::shopify::{
        ActivateInventoryRequest, InventoryActivationItem, InventoryDiscoveryRequest, PageRequest,
        ShopifyId,
    },
    services::common::upstream,
    state::AppState,
};
use axum::http::StatusCode;
use domain::dto::{InventoryEnableError, InventoryEnableResponse};
use std::collections::{HashMap, HashSet};

pub async fn enable_inventory(
    state: &AppState,
    location_id: ShopifyId,
) -> Result<InventoryEnableResponse, AppError> {
    let unavailable = || {
        AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Shopify inventory activation unavailable",
        )
    };
    let client = state
        .shopify
        .as_ref()
        .map_err(|error| unavailable().with_source(error.clone()))?
        .as_deref()
        .ok_or_else(unavailable)?;
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

async fn discover(
    client: &ShopifyClient,
    location: &ShopifyId,
) -> Result<Vec<InventoryActivationItem>, AppError> {
    let request = InventoryDiscoveryRequest {
        location_id: location.clone(),
    };
    let mut page_request = PageRequest {
        first: 250,
        after: None,
    };
    let mut cursors = HashSet::new();
    let mut seen = HashMap::new();
    let mut items = Vec::new();
    loop {
        let page = client
            .find_inventory_to_activate(&request, &page_request)
            .await
            .map_err(|error| upstream(error))?
            .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Shopify location not found"))?;
        page.page_info
            .validate_after(page_request.after.as_deref())
            .map_err(|error| upstream(error))?;
        for candidate in page.nodes {
            if let Some(previous) = seen.get(&candidate.item.id) {
                if previous != &candidate {
                    return Err(upstream(ClientError::new(ClientErrorKind::Protocol(
                        "conflicting inventory discovery entries",
                    ))));
                }
                continue;
            }
            if let Some(item) = candidate
                .activation_item()
                .map_err(|error| upstream(error))?
            {
                items.push(item);
            }
            seen.insert(candidate.item.id.clone(), candidate);
        }
        if !page.page_info.has_next_page {
            return Ok(items);
        }
        let next = page.page_info.end_cursor.ok_or_else(|| {
            upstream(ClientError::new(ClientErrorKind::Protocol(
                "next cursor is missing",
            )))
        })?;
        if !cursors.insert(next.clone()) {
            return Err(upstream(ClientError::new(ClientErrorKind::Protocol(
                "pagination cursor did not progress",
            ))));
        }
        page_request.after = Some(next);
    }
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
        on_hand: item.on_hand,
        available: item.available,
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
