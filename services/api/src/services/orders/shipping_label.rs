use super::super::{
    common::{shopify_client, upstream},
    pagination::for_each_page,
};
use crate::{
    client::{ClientError, ClientErrorKind, shopify::TagAction},
    config::ShippingSender,
    error::AppError,
    models::{fulfillment::ShippingOrder, shopify::ShopifyId},
    state::AppState,
};
use axum::http::StatusCode;
use common::{Parcel, select_courier};
use domain::dto::orders::{Courier, ShippingLabels};

mod best;
mod common;
mod fastship;
mod teleport;
mod thaipost;

pub async fn labels(state: &AppState) -> Result<ShippingLabels, AppError> {
    let client = shopify_client(state)?;
    let mut orders = Vec::new();
    for_each_page(
        None,
        async |after| client.shipping_orders(after).await,
        |page| {
            orders.extend(page);
            Ok(())
        },
        std::convert::identity,
    )
    .await
    .map_err(|error| upstream(error))?;
    let mut labels = ShippingLabels::default();
    let mut tags = Vec::new();
    for mut order in orders {
        ShopifyId::new(order.id.clone())
            .and_then(|id| id.number("Order"))
            .map_err(|error| upstream(error))?;
        let id = order.id.clone();
        let mut items = Vec::new();
        for_each_page(
            Some(order.line_items),
            async |after| {
                let result = client.shipping_order_items(&id, after).await?;
                result.order.map(|next| next.line_items).ok_or_else(|| {
                    ClientError::new(ClientErrorKind::Protocol("shipping order disappeared"))
                })
            },
            |page| {
                items.extend(page);
                Ok(())
            },
            std::convert::identity,
        )
        .await
        .map_err(upstream)?;
        order.line_items = complete(items);
        let mut methods = Vec::new();
        for_each_page(
            Some(order.shipping_lines),
            async |after| {
                let result = client.shipping_order_methods(&id, after).await?;
                result.order.map(|next| next.shipping_lines).ok_or_else(|| {
                    ClientError::new(ClientErrorKind::Protocol("shipping order disappeared"))
                })
            },
            |page| {
                methods.extend(page);
                Ok(())
            },
            std::convert::identity,
        )
        .await
        .map_err(upstream)?;
        order.shipping_lines = complete(methods);
        let courier = append_label(order, state.settings.shipping_sender.as_ref(), &mut labels)?;
        tags.push((id, courier));
    }
    for (id, courier) in tags {
        client
            .edit_order_tags(&id, &["SHIPPING", courier.tag()], TagAction::Add)
            .await
            .map_err(upstream)?;
        client
            .edit_order_tags(&id, &["PACKING"], TagAction::Remove)
            .await
            .map_err(upstream)?;
    }
    Ok(labels)
}
pub(super) fn append_label(
    order: ShippingOrder,
    sender: Option<&ShippingSender>,
    labels: &mut ShippingLabels,
) -> Result<Courier, AppError> {
    let parcel = Parcel::from_order(order)?;
    let courier = select_courier(&parcel.method).ok_or_else(|| {
        AppError::new(
            StatusCode::CONFLICT,
            "Order has no supported shipping method",
        )
    })?;
    match courier {
        Courier::Fastship => labels.fastship.push(parcel.fastship()),
        Courier::ThaiPost => labels.thaipost.push(parcel.thaipost(sender.ok_or_else(|| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Shipping sender unavailable",
            )
        })?)),
        Courier::Best => labels.best.push(parcel.best()),
        Courier::Teleport => labels.teleport.push(parcel.teleport()?),
    }
    Ok(courier)
}

fn complete<T>(nodes: Vec<T>) -> crate::models::shopify::Connection<T> {
    crate::models::shopify::Connection {
        nodes,
        page_info: crate::models::shopify::PageInfo {
            has_next_page: false,
            end_cursor: None,
        },
    }
}

#[cfg(test)]
#[path = "../../tests/services/orders/shipping_label.rs"]
mod tests;
