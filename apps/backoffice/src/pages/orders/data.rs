use crate::{
    api::{self, ApiResult},
    pages::reports::data::{PageData, collect_pages},
};
use domain::dto::orders::{Courier, FulfillOrderResponse, ListOrdersResponse, OrderSummary};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
};

#[derive(Clone, PartialEq)]
pub struct FulfillmentOrder {
    pub order: OrderSummary,
    pub courier: Courier,
}

#[derive(Clone, PartialEq)]
pub struct FulfillmentResult {
    pub order_number: i64,
    pub tracking_number: String,
    pub result: ApiResult<FulfillOrderResponse>,
}

pub async fn load_fulfillment_orders() -> ApiResult<Vec<FulfillmentOrder>> {
    let fastship = orders_by_tag("SHIPPING,FASTSHIP".into()).await?;
    let thai_post = orders_by_tag("SHIPPING,THAI_POST".into()).await?;
    Ok(merge_courier_orders(fastship, thai_post))
}

async fn orders_by_tag(tag: String) -> ApiResult<Vec<OrderSummary>> {
    collect_pages(move |cursor| {
        let tag = tag.clone();
        async move {
            let response = api::orders::list_orders(&tag, cursor).await?;
            Ok(page_data(response))
        }
    })
    .await
}

fn page_data(response: ListOrdersResponse) -> PageData<OrderSummary> {
    PageData::new(response.orders, response.page_info)
}

fn merge_courier_orders(
    fastship: Vec<OrderSummary>,
    thai_post: Vec<OrderSummary>,
) -> Vec<FulfillmentOrder> {
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for order in fastship {
        if seen.insert(order.id) {
            rows.push(FulfillmentOrder {
                order,
                courier: Courier::Fastship,
            });
        }
    }
    for order in thai_post {
        if seen.insert(order.id) {
            rows.push(FulfillmentOrder {
                order,
                courier: Courier::ThaiPost,
            });
        }
    }
    rows
}

pub async fn fulfill_selected<F, Fut>(
    orders: Vec<FulfillmentOrder>,
    tracking_numbers: HashMap<u64, String>,
    mut fulfill: F,
) -> ApiResult<Vec<FulfillmentResult>>
where
    F: FnMut(u64, Courier, String) -> Fut,
    Fut: Future<Output = ApiResult<FulfillOrderResponse>>,
{
    let mut results = Vec::new();
    for row in orders {
        let Some(tracking_number) = tracking_numbers
            .get(&row.order.id)
            .filter(|tracking_number| !tracking_number.trim().is_empty())
            .map(|tracking_number| tracking_number.trim().to_owned())
        else {
            continue;
        };
        let result = fulfill(row.order.id, row.courier, tracking_number.clone()).await;
        results.push(FulfillmentResult {
            order_number: row.order.number,
            tracking_number,
            result,
        });
    }
    Ok(results)
}

#[cfg(test)]
#[path = "../../tests/pages/orders/data.rs"]
mod tests;
