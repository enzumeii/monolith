use super::super::{common::upstream, pagination::for_each_page};
use crate::{
    client::shopify::TagAction,
    client::{ClientError, ClientErrorKind},
    error::AppError,
    models::shopify::ShopifyId,
};
use axum::http::StatusCode;
use domain::dto::orders::{FulfillOrderRequest, FulfillOrderResponse};
use serde_json::{Value, json};
use std::collections::HashSet;

pub async fn fulfill(
    client: &crate::client::shopify::ShopifyClient,
    id: ShopifyId,
    request: FulfillOrderRequest,
) -> Result<FulfillOrderResponse, AppError> {
    let mut orders = Vec::new();
    for_each_page(
        None,
        async |after| {
            client
                .fulfillment_orders(&id, after)
                .await
                .map_err(upstream)?
                .order
                .map(|order| order.fulfillment_orders)
                .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Shopify order not found"))
        },
        |page| {
            orders.extend(page);
            Ok(())
        },
        upstream,
    )
    .await?;
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for order in orders {
        ShopifyId::new(order.id.clone())
            .and_then(|id| id.number("FulfillmentOrder"))
            .map_err(|error| upstream(error))?;
        if !seen.insert(order.id.clone()) {
            return Err(protocol("duplicate fulfillment order"));
        }
        if matches!(order.status.as_str(), "CLOSED" | "CANCELLED") {
            continue;
        }
        let lines: Vec<_> = order
            .line_items
            .nodes
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
        let items = fulfillment_lines(lines)?;
        entries.push(json!({"fulfillmentOrderId":order.id,"fulfillmentOrderLineItems":items}));
    }
    if entries.is_empty() {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "Order has no remaining fulfillable items",
        ));
    }
    let (company, url) = tracking(&request.tracking_number)?;
    let created = client
        .create_fulfillment(json!({
            "notifyCustomer": true,
            "trackingInfo": {"company":company,"number":request.tracking_number,"url":url},
            "lineItemsByFulfillmentOrder":entries,
        }))
        .await
        .map_err(upstream)?;
    ShopifyId::new(created)
        .and_then(|id| id.number("Fulfillment"))
        .map_err(upstream)?;
    super::edit_order_tags(client, id.as_ref(), &["SHIPPING"], TagAction::Remove).await?;
    Ok(FulfillOrderResponse {
        success: true,
        tracking_url: url,
    })
}
fn fulfillment_lines(
    lines: Vec<crate::models::shopify::FulfillmentLine>,
) -> Result<Vec<Value>, AppError> {
    let mut items = Vec::new();
    let mut ids = HashSet::new();
    for line in lines {
        ShopifyId::new(line.id.clone())
            .and_then(|id| id.number("FulfillmentOrderLineItem"))
            .map_err(|error| upstream(error))?;
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
