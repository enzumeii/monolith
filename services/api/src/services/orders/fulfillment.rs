use super::{
    super::common::{shopify_client, upstream},
    workflow::ensure_same_order,
};
use crate::{
    client::{ClientError, ClientErrorKind},
    error::AppError,
    models::fulfillment::resource_number,
    state::AppState,
};
use axum::http::StatusCode;
use domain::dto::orders::{FulfillOrderRequest, FulfillOrderResponse};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

pub async fn fulfill(
    state: &AppState,
    id: String,
    request: FulfillOrderRequest,
) -> Result<FulfillOrderResponse, AppError> {
    let client = shopify_client(state)?;
    let mut after = None;
    let mut cursors = HashSet::new();
    let mut seen = HashSet::new();
    let mut locations: BTreeMap<String, Vec<Vec<Value>>> = BTreeMap::new();
    loop {
        let result = client
            .fulfillment_orders(&id, after.clone())
            .await
            .map_err(|error| upstream(error))?;
        let order = result
            .order
            .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Shopify order not found"))?;
        ensure_same_order(&id, &order.id).map_err(|error| upstream(error))?;
        let page = order.fulfillment_orders;
        page.page_info
            .validate_after(after.as_deref())
            .map_err(|error| upstream(error))?;
        for order in page.nodes {
            resource_number(&order.id, "FulfillmentOrder").map_err(|error| upstream(error))?;
            if !seen.insert(order.id.clone()) {
                return Err(protocol("duplicate fulfillment order"));
            }
            let lines =
                super::super::pagination::collect_connection(order.line_items, async |after| {
                    let result = client.fulfillment_order_items(&order.id, after).await?;
                    let next = result.fulfillment_order.ok_or_else(|| {
                        ClientError::new(ClientErrorKind::Protocol("fulfillment order disappeared"))
                    })?;
                    ensure_same_order(&order.id, &next.id)?;
                    Ok(next.line_items)
                })
                .await
                .map_err(|error| upstream(error))?;
            if matches!(order.status.as_str(), "CLOSED" | "CANCELLED") {
                continue;
            }
            let lines: Vec<_> = lines
                .into_iter()
                .filter(|line| line.remaining_quantity > 0)
                .collect();
            if lines.is_empty() {
                continue;
            }
            if !order
                .supported_actions
                .iter()
                .any(|action| action.action == "CREATE_FULFILLMENT")
            {
                return Err(AppError::new(
                    StatusCode::CONFLICT,
                    "Order cannot be fulfilled in its current state",
                ));
            }
            let location = order
                .assigned_location
                .location
                .ok_or_else(|| protocol("fulfillment location missing"))?;
            resource_number(&location.id, "Location").map_err(|error| upstream(error))?;
            let items = fulfillment_lines(lines)?;
            let batches = locations.entry(location.id).or_default();
            for (index, chunk) in items.chunks(250).enumerate() {
                if batches.len() <= index {
                    batches.push(Vec::new());
                }
                batches[index]
                    .push(json!({"fulfillmentOrderId":order.id,"fulfillmentOrderLineItems":chunk}));
            }
        }
        if !page.page_info.has_next_page {
            break;
        }
        after = page.page_info.end_cursor;
        if !cursors.insert(after.clone()) {
            return Err(protocol("fulfillment cursor did not progress"));
        }
    }
    if locations.is_empty() {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "Order has no remaining fulfillable items",
        ));
    }
    let url = create_fulfillments(client, locations, &request).await?;
    let updated = client
        .remove_order_tags(&id, &["SHIPPING"])
        .await
        .map_err(|error| upstream(error))?;
    ensure_same_order(&id, &updated).map_err(|error| upstream(error))?;
    Ok(FulfillOrderResponse {
        success: true,
        tracking_url: url,
    })
}
async fn create_fulfillments(
    client: &crate::client::shopify::ShopifyClient,
    locations: BTreeMap<String, Vec<Vec<Value>>>,
    request: &FulfillOrderRequest,
) -> Result<String, AppError> {
    let (company, url) = tracking(&request.tracking_number)?;
    for batches in locations.into_values() {
        for batch in batches {
            for orders in batch.chunks(250) {
                let created = client
                    .create_fulfillment(json!({
                        "notifyCustomer": true,
                        "trackingInfo": {
                            "company": company,
                            "number": request.tracking_number,
                            "url": url,
                        },
                        "lineItemsByFulfillmentOrder": orders,
                    }))
                    .await
                    .map_err(|error| upstream(error))?;
                resource_number(&created, "Fulfillment").map_err(|error| upstream(error))?;
            }
        }
    }
    Ok(url)
}

fn fulfillment_lines(
    lines: Vec<crate::models::fulfillment::FulfillmentLine>,
) -> Result<Vec<Value>, AppError> {
    let mut items = Vec::new();
    let mut ids = HashSet::new();
    for line in lines {
        resource_number(&line.id, "FulfillmentOrderLineItem").map_err(|error| upstream(error))?;
        if !ids.insert(line.id.clone()) {
            return Err(protocol("duplicate fulfillment line"));
        }
        items.push(json!({"id":line.id,"quantity":line.remaining_quantity}));
    }
    Ok(items)
}
fn tracking(number: &str) -> Result<(&'static str, String), AppError> {
    let (company, base) =
        if (number.len() == 26 && number.starts_with("92")) || number.starts_with("1Z") {
            ("UPS", "https://www.ups.com/track?tracknum=")
        } else if number.len() == 22 {
            (
                "USPS",
                "https://tools.usps.com/go/TrackConfirmAction?tLabels=",
            )
        } else if number.starts_with("YT") {
            ("YunExpress", "https://www.yuntrack.com/parcelTracking?id=")
        } else if number.starts_with("VP") {
            ("Other", "https://www.aftership.com/track/800bestex/")
        } else if ["LR", "LX", "FDA", "FXA", "WG"]
            .iter()
            .any(|prefix| number.starts_with(prefix))
        {
            ("Other", "https://track.fastship.co/")
        } else if number.ends_with("TH") {
            ("Other", "https://www.aftership.com/track/thailand-post/")
        } else {
            ("Other", "https://www.aftership.com/track/")
        };
    let mut url = reqwest::Url::parse(base).map_err(|_| protocol("invalid tracking URL"))?;
    if base.ends_with('=') {
        let key = url
            .query_pairs()
            .next()
            .map(|(key, _)| key.into_owned())
            .ok_or_else(|| protocol("invalid tracking URL"))?;
        url.set_query(None);
        url.query_pairs_mut().append_pair(&key, number);
    } else {
        url.path_segments_mut()
            .map_err(|()| protocol("invalid tracking URL"))?
            .pop_if_empty()
            .push(number);
    }
    Ok((company, url.into()))
}
fn protocol(message: &'static str) -> AppError {
    upstream(ClientError::new(ClientErrorKind::Protocol(message)))
}

#[cfg(test)]
#[path = "../../tests/services/orders/fulfillment.rs"]
mod tests;
