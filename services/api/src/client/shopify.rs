use super::pagination::collect_pages;
use crate::{
    client::{ClientError, GraphqlClient, GraphqlRequest, HttpClient},
    config::ShopifyConfig,
    models::shopify::{
        ActivateInventoryRequest, ActivationData, InventoryActivationCandidate, InventoryData,
        InventoryDiscoveryRequest, InventoryLevel, ListProductsRequest, MutationUserError, Order,
        OrderTags, OrderUpdateData, OrdersByPaymentRequest, OrdersByTagData, OrdersByTagRequest,
        OrdersData, Product, ProductsData, ReplaceOrderTagsRequest, SalesData, SalesOrder,
        SalesRangeRequest, TaggedOrder,
    },
};
use reqwest::{
    Url,
    header::{HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::time::Duration;

pub struct ShopifyClient {
    graphql: GraphqlClient,
    headers: HeaderMap,
}
impl ShopifyClient {
    pub fn new(config: &ShopifyConfig) -> Result<Self, ClientError> {
        let endpoint = Url::parse(&config.admin_api_url)
            .map_err(|_| ClientError::InvalidInput("invalid Shopify URL"))?;
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
        collect_pages(async |after| {
            let data: ProductsData = self
                .execute(
                    "ProductsWithVariants",
                    include_str!("shopify/products.graphql"),
                    json!({"locationId":input.location_id,"search":search,"productsAfter":after}),
                )
                .await?;
            if data
                .products
                .nodes
                .iter()
                .any(|product| product.variants.page_info.has_next_page)
            {
                return Err(ClientError::Protocol(
                    "product exceeds 250 supported variants",
                ));
            }
            Ok(data.products)
        })
        .await
    }

    pub async fn orders_by_payment_id(
        &self,
        input: &OrdersByPaymentRequest,
    ) -> Result<Vec<Order>, ClientError> {
        if input.payment_id.trim().is_empty() {
            return Err(ClientError::InvalidInput("payment ID is empty"));
        }
        let search = format!("payment_id:\"{}\"", escape_search(&input.payment_id));
        collect_pages(async |after| {
            let data: OrdersData = self
                .execute(
                    "OrdersByPayment",
                    include_str!("shopify/payment.graphql"),
                    json!({"search":search,"after":after}),
                )
                .await?;
            Ok(data.orders)
        })
        .await
    }

    pub async fn orders_by_tag(
        &self,
        input: &OrdersByTagRequest,
    ) -> Result<Vec<TaggedOrder>, ClientError> {
        if input.tag.trim().is_empty() {
            return Err(ClientError::InvalidInput("order tag is empty"));
        }
        let search = format!("tag:\"{}\"", escape_search(&input.tag));
        collect_pages(async |after| {
            let data: OrdersByTagData = self
                .execute(
                    "OrdersByTag",
                    include_str!("shopify/orders_by_tag.graphql"),
                    json!({"search":search,"after":after}),
                )
                .await?;
            Ok(data.orders)
        })
        .await
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
        collect_pages(async |after| {
            let data: SalesData = self
                .execute(
                    "SoldVariantsInRange",
                    include_str!("shopify/sales.graphql"),
                    json!({"search":search,"ordersAfter":after}),
                )
                .await?;
            if data
                .orders
                .nodes
                .iter()
                .any(|order| order.line_items.page_info.has_next_page)
            {
                return Err(ClientError::Protocol(
                    "order exceeds 250 supported line items",
                ));
            }
            Ok(data.orders)
        })
        .await
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
        let levels = collect_pages(async |after| {
            let data: InventoryData = self
                .execute(
                    "FindInventoryToActivate",
                    include_str!("shopify/inventory.graphql"),
                    json!({"locationId":input.location_id,"after":after}),
                )
                .await?;
            Ok(data
                .location
                .ok_or(ClientError::InvalidInput("location was not found"))?
                .inventory_levels)
        })
        .await?;
        let mut candidates = Vec::new();
        for level in levels {
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
        Ok(candidates)
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
#[cfg(test)]
#[path = "../tests/client/shopify.rs"]
mod tests;
