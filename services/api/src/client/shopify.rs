use super::pagination::{collect_connection, collect_pages};
use crate::{
    client::{ClientError, GraphqlClient, GraphqlRequest, HttpClient},
    config::ShopifyConfig,
    models::shopify::{
        ActivateInventoryRequest, ActivationData, Connection, InventoryActivationCandidate,
        InventoryData, InventoryDiscoveryRequest, InventoryLevel, ListProductsRequest,
        MutationUserError, Order, OrderLinesData, OrderTags, OrderUpdateData,
        OrdersByPaymentRequest, OrdersByTagData, OrdersByTagRequest, OrdersData, Product,
        ProductsData, ReplaceOrderTagsRequest, SalesData, SalesOrder, SalesRangeRequest, ShopifyId,
        TaggedOrder, TaggedOrderLineItems,
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
        collect_pages(async |after| {
            let data: ProductsData = self
                .execute(
                    "ProductsWithVariants",
                    include_str!("shopify/products.graphql"),
                    input.variables(after.as_deref())?,
                )
                .await?;
            Ok(data.products)
        })
        .await
    }

    pub async fn orders_by_payment_id(
        &self,
        input: &OrdersByPaymentRequest,
    ) -> Result<Vec<Order>, ClientError> {
        collect_pages(async |after| {
            let data: OrdersData = self
                .execute(
                    "OrdersByPayment",
                    include_str!("shopify/payment.graphql"),
                    input.variables(after.as_deref())?,
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
        let headers = collect_pages(async |after| {
            let data: OrdersByTagData = self
                .execute(
                    "OrdersByTag",
                    include_str!("shopify/orders_by_tag.graphql"),
                    input.variables(after.as_deref())?,
                )
                .await?;
            Ok(data.orders)
        })
        .await?;
        let mut orders = Vec::with_capacity(headers.len());
        for header in headers {
            let lines = self
                .order_lines(
                    &header.id,
                    "TaggedOrderLines",
                    include_str!("shopify/tagged_order_lines.graphql"),
                )
                .await?;
            orders.push(header.with_lines(TaggedOrderLineItems { nodes: lines.nodes }));
        }
        Ok(orders)
    }

    pub async fn sales_in_range(
        &self,
        input: &SalesRangeRequest,
    ) -> Result<Vec<SalesOrder>, ClientError> {
        let headers = collect_pages(async |after| {
            let data: SalesData = self
                .execute(
                    "SoldVariantsInRange",
                    include_str!("shopify/sales.graphql"),
                    input.variables(after.as_deref())?,
                )
                .await?;
            Ok(data.orders)
        })
        .await?;
        let mut orders = Vec::with_capacity(headers.len());
        for header in headers {
            let lines = self
                .order_lines(
                    &header.id,
                    "SalesOrderLines",
                    include_str!("shopify/sales_lines.graphql"),
                )
                .await?;
            orders.push(header.with_lines(lines));
        }
        Ok(orders)
    }

    pub async fn replace_order_tags(
        &self,
        input: &ReplaceOrderTagsRequest,
    ) -> Result<OrderTags, ClientError> {
        let data: OrderUpdateData = self
            .execute(
                "ReplaceOrderTags",
                include_str!("shopify/tags.graphql"),
                input.variables()?,
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
        let levels = collect_pages(async |after| {
            let data: InventoryData = self
                .execute(
                    "FindInventoryToActivate",
                    include_str!("shopify/inventory.graphql"),
                    input.variables(after.as_deref())?,
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
        let data: ActivationData = self
            .execute(
                "ActivateExistingInventory",
                include_str!("shopify/activate.graphql"),
                input.variables()?,
            )
            .await?;
        check_user_errors(data.inventory_activate.user_errors)?;
        data.inventory_activate
            .inventory_level
            .ok_or(ClientError::Protocol(
                "activated inventory level is missing",
            ))
    }

    async fn order_lines<T: DeserializeOwned>(
        &self,
        id: &ShopifyId,
        operation: &str,
        query: &str,
    ) -> Result<Connection<T>, ClientError> {
        collect_connection(async |after| {
            let data: OrderLinesData<T> = self
                .execute(operation, query, json!({"id": id, "after": after}))
                .await?;
            let order = data.order.ok_or(ClientError::Protocol(
                "order is missing during line-item pagination",
            ))?;
            if &order.id != id {
                return Err(ClientError::Protocol(
                    "line items belong to a different order",
                ));
            }
            Ok(order.line_items)
        })
        .await
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
