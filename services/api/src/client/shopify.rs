use crate::{
    client::{ClientError, GraphqlClient, GraphqlRequest, HttpClient},
    config::ShopifyConfig,
    models::shopify::{
        ActivateInventoryRequest, Connection, InventoryActivationCandidate,
        InventoryDiscoveryRequest, InventoryLevel, ListProductsRequest, MutationUserError, Order,
        OrderTags, OrdersByPaymentRequest, PageInfo, Product, ReplaceOrderTagsRequest, SalesOrder,
        SalesRangeRequest,
    },
};
use reqwest::{
    Url,
    header::{HeaderMap, HeaderValue},
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{collections::HashSet, time::Duration};

pub struct ShopifyClient {
    graphql: GraphqlClient,
    headers: HeaderMap,
}
impl ShopifyClient {
    pub fn new(config: &ShopifyConfig) -> Result<Self, ClientError> {
        let endpoint = Url::parse(&config.admin_api_url)
            .map_err(|_| ClientError::InvalidInput("invalid Shopify URL"))?;
        if endpoint.scheme() != "https"
            || !endpoint
                .host_str()
                .is_some_and(|h| h.ends_with(".myshopify.com") && h.len() > ".myshopify.com".len())
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.port().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || endpoint.path() != "/admin/api/2026-10/graphql.json"
        {
            return Err(ClientError::InvalidInput("invalid Shopify Admin API URL"));
        }
        Self::with_endpoint(config, endpoint)
    }

    pub async fn list_products(
        &self,
        input: &ListProductsRequest,
    ) -> Result<Vec<Product>, ClientError> {
        input.location_id.require_kind("Location")?;
        let search = if input.statuses.is_empty() {
            None
        } else {
            Some(format!(
                "status:{}",
                input
                    .statuses
                    .iter()
                    .map(|s| s.search_value())
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        };
        let mut products = Vec::new();
        let mut outer = Cursor::default();
        loop {
            let mut inner = Cursor::default();
            let mut assembled: Option<Product> = None;
            let outer_info = loop {
                let data: ProductsData = self
                    .execute(
                        "ProductsWithVariants",
                        include_str!("shopify/products.graphql"),
                        json!({
                            "locationId": input.location_id, "search":search,
                            "productsAfter":outer.after, "variantsAfter":inner.after,
                        }),
                    )
                    .await?;
                let mut connection = data.products;
                let Some(mut product) = connection.nodes.pop() else {
                    if connection.page_info.has_next_page || assembled.is_some() {
                        return Err(ClientError::Protocol(
                            "product disappeared during pagination",
                        ));
                    }
                    return Ok(products);
                };
                if !connection.nodes.is_empty() {
                    return Err(ClientError::Protocol("expected one product per page"));
                }
                let next = inner.advance(&product.variants.page_info)?;
                if let Some(collected) = assembled.as_mut() {
                    if collected.id != product.id {
                        return Err(ClientError::Protocol("product changed during pagination"));
                    }
                    collected.variants.nodes.append(&mut product.variants.nodes);
                    collected.variants.page_info = product.variants.page_info;
                } else {
                    assembled = Some(product);
                }
                if !next {
                    break connection.page_info;
                }
            };
            products.push(assembled.ok_or(ClientError::Protocol("product missing"))?);
            if !outer.advance(&outer_info)? {
                return Ok(products);
            }
        }
    }

    pub async fn orders_by_payment_id(
        &self,
        input: &OrdersByPaymentRequest,
    ) -> Result<Vec<Order>, ClientError> {
        if input.payment_id.trim().is_empty() {
            return Err(ClientError::InvalidInput("payment ID is empty"));
        }
        let search = format!("payment_id:\"{}\"", escape_search(&input.payment_id));
        let mut orders = Vec::new();
        let mut cursor = Cursor::default();
        loop {
            let data: OrdersData = self
                .execute(
                    "OrdersByPayment",
                    include_str!("shopify/payment.graphql"),
                    json!({"search":search,"after":cursor.after}),
                )
                .await?;
            orders.extend(data.orders.nodes);
            if !cursor.advance(&data.orders.page_info)? {
                return Ok(orders);
            }
        }
    }

    pub async fn sales_in_range(
        &self,
        input: &SalesRangeRequest,
    ) -> Result<Vec<SalesOrder>, ClientError> {
        if input.start >= input.end {
            return Err(ClientError::InvalidInput("sales start must precede end"));
        }
        let search = format!(
            "created_at:>='{}' created_at:<'{}'",
            input.start.to_rfc3339(),
            input.end.to_rfc3339()
        );
        let mut orders = Vec::new();
        let mut outer = Cursor::default();
        loop {
            let mut inner = Cursor::default();
            let mut assembled: Option<SalesOrder> = None;
            let outer_info = loop {
                let data: SalesData = self
                    .execute(
                        "SoldVariantsInRange",
                        include_str!("shopify/sales.graphql"),
                        json!({
                            "search":search,"ordersAfter":outer.after,"lineItemsAfter":inner.after,
                        }),
                    )
                    .await?;
                let mut connection = data.orders;
                let Some(mut order) = connection.nodes.pop() else {
                    if connection.page_info.has_next_page || assembled.is_some() {
                        return Err(ClientError::Protocol("order disappeared during pagination"));
                    }
                    return Ok(orders);
                };
                if !connection.nodes.is_empty() {
                    return Err(ClientError::Protocol("expected one order per page"));
                }
                let next = inner.advance(&order.line_items.page_info)?;
                if let Some(collected) = assembled.as_mut() {
                    if collected.id != order.id {
                        return Err(ClientError::Protocol("order changed during pagination"));
                    }
                    collected
                        .line_items
                        .nodes
                        .append(&mut order.line_items.nodes);
                    collected.line_items.page_info = order.line_items.page_info;
                } else {
                    assembled = Some(order);
                }
                if !next {
                    break connection.page_info;
                }
            };
            orders.push(assembled.ok_or(ClientError::Protocol("order missing"))?);
            if !outer.advance(&outer_info)? {
                return Ok(orders);
            }
        }
    }

    pub async fn replace_order_tags(
        &self,
        input: &ReplaceOrderTagsRequest,
    ) -> Result<OrderTags, ClientError> {
        input.order_id.require_kind("Order")?;
        let data: OrderUpdateData = self
            .execute(
                "ReplaceOrderTags",
                include_str!("shopify/tags.graphql"),
                json!({
                    "input":{"id":input.order_id,"tags":input.tags},
                }),
            )
            .await?;
        check_user_errors(data.order_update.user_errors)?;
        data.order_update
            .order
            .ok_or(ClientError::Protocol("updated order is missing"))
    }

    pub async fn find_inventory_to_activate(
        &self,
        input: &InventoryDiscoveryRequest,
    ) -> Result<Vec<InventoryActivationCandidate>, ClientError> {
        input.location_id.require_kind("Location")?;
        let mut candidates = Vec::new();
        let mut cursor = Cursor::default();
        loop {
            let data: InventoryData = self
                .execute(
                    "FindInventoryToActivate",
                    include_str!("shopify/inventory.graphql"),
                    json!({
                        "locationId":input.location_id,"after":cursor.after,
                    }),
                )
                .await?;
            let location = data
                .location
                .ok_or(ClientError::InvalidInput("location was not found"))?;
            for level in location.inventory_levels.nodes {
                let on_hand = level
                    .quantities
                    .iter()
                    .find(|q| q.name == "on_hand")
                    .ok_or(ClientError::Protocol("on_hand quantity is missing"))?
                    .quantity;
                if level.item.tracked && !level.is_active && on_hand > 0 {
                    candidates.push(level);
                }
            }
            if !cursor.advance(&location.inventory_levels.page_info)? {
                return Ok(candidates);
            }
        }
    }

    pub async fn activate_inventory(
        &self,
        input: &ActivateInventoryRequest,
    ) -> Result<InventoryLevel, ClientError> {
        input.inventory_item_id.require_kind("InventoryItem")?;
        input.location_id.require_kind("Location")?;
        let data: ActivationData = self.execute("ActivateExistingInventory", include_str!("shopify/activate.graphql"), json!({
            "inventoryItemId":input.inventory_item_id,"locationId":input.location_id,"idempotencyKey":input.idempotency_key,
        })).await?;
        check_user_errors(data.inventory_activate.user_errors)?;
        data.inventory_activate
            .inventory_level
            .ok_or(ClientError::Protocol(
                "activated inventory level is missing",
            ))
    }

    fn with_endpoint(config: &ShopifyConfig, endpoint: Url) -> Result<Self, ClientError> {
        let token = config
            .access_token
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or(ClientError::InvalidInput("Shopify token is missing"))?;
        let mut value = HeaderValue::from_str(token)
            .map_err(|_| ClientError::InvalidInput("invalid Shopify token header"))?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert("X-Shopify-Access-Token", value);
        Ok(Self {
            graphql: GraphqlClient::new(HttpClient::new(Duration::from_secs(30))?, endpoint),
            headers,
        })
    }
    async fn execute<D: DeserializeOwned>(
        &self,
        operation_name: &str,
        query: &str,
        variables: Value,
    ) -> Result<D, ClientError> {
        self.graphql
            .execute(
                self.headers.clone(),
                &GraphqlRequest {
                    query,
                    operation_name,
                    variables,
                },
            )
            .await
    }
}
#[derive(Default)]
struct Cursor {
    after: Option<String>,
    seen: HashSet<String>,
}
impl Cursor {
    fn advance(&mut self, page: &PageInfo) -> Result<bool, ClientError> {
        if !page.has_next_page {
            return Ok(false);
        }
        let next = page
            .end_cursor
            .as_ref()
            .filter(|v| !v.is_empty())
            .ok_or(ClientError::Protocol("next cursor is missing"))?;
        if !self.seen.insert(next.clone()) {
            return Err(ClientError::Protocol("pagination cursor did not progress"));
        }
        self.after = Some(next.clone());
        Ok(true)
    }
}
fn escape_search(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        if matches!(character, '\\' | '"' | ':' | '(' | ')') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}
fn check_user_errors(errors: Vec<MutationUserError>) -> Result<(), ClientError> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ClientError::Mutation { errors })
    }
}
#[derive(Deserialize)]
struct ProductsData {
    products: Connection<Product>,
}
#[derive(Deserialize)]
struct OrdersData {
    orders: Connection<Order>,
}
#[derive(Deserialize)]
struct SalesData {
    orders: Connection<SalesOrder>,
}
#[derive(Deserialize)]
struct InventoryData {
    location: Option<LocationInventory>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocationInventory {
    inventory_levels: Connection<InventoryActivationCandidate>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderUpdateData {
    order_update: OrderUpdate,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderUpdate {
    order: Option<OrderTags>,
    user_errors: Vec<MutationUserError>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivationData {
    inventory_activate: Activation,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Activation {
    inventory_level: Option<InventoryLevel>,
    user_errors: Vec<MutationUserError>,
}
#[cfg(test)]
#[path = "../tests/client/shopify.rs"]
mod tests;
