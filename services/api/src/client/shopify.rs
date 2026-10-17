use super::pagination::collect_pages;
use crate::{
    client::{ClientError, GraphqlClient, GraphqlRequest, HttpClient},
    config::ShopifyConfig,
    models::shopify::{
        ActivateInventoryRequest, ActivationData, Connection, InventoryActivationCandidate,
        InventoryData, InventoryDiscoveryRequest, ListProductsRequest, Order, OrderLinesData,
        OrderLinesRequest, OrderUpdateData, OrdersByPaymentRequest, OrdersByTagData,
        OrdersByTagRequest, OrdersData, Product, ProductsData, ReplaceOrderTagsRequest, SalesData,
        SalesLine, SalesOrderHeader, SalesRangeRequest, ShopifyId, TaggedOrderHeader,
        TaggedOrderLineItem,
    },
};
use reqwest::{
    Url,
    header::{HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::Duration;

const ORIGIN: &str = "api::client::shopify";

pub struct ShopifyClient {
    graphql: GraphqlClient,
    headers: HeaderMap,
}
impl ShopifyClient {
    pub fn new(config: &ShopifyConfig) -> Result<Self, ClientError> {
        let endpoint = Url::parse(&config.admin_api_url)
            .map_err(|_| ClientError::InvalidInput("invalid Shopify URL").at(ORIGIN))?;
        let token = config
            .access_token
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| ClientError::InvalidInput("Shopify token is missing").at(ORIGIN))?;
        let mut value = HeaderValue::from_str(token)
            .map_err(|_| ClientError::InvalidInput("invalid Shopify token header").at(ORIGIN))?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert("X-Shopify-Access-Token", value);
        Ok(Self {
            graphql: GraphqlClient::new(
                HttpClient::new(Duration::from_secs(30)).map_err(|error| error.at(ORIGIN))?,
                endpoint,
            ),
            headers,
        })
    }
    pub async fn products_with_variants(
        &self,
        input: &ListProductsRequest,
    ) -> Result<Vec<Product>, ClientError> {
        collect_pages(async |after| {
            let data: ProductsData = self
                .execute(
                    "ProductsWithVariants",
                    include_str!("shopify/products_with_variants.graphql"),
                    input.variables(after.as_deref())?,
                )
                .await?;
            Ok(data.products)
        })
        .await
        .map_err(|error| error.at(ORIGIN))
    }
    pub async fn orders_by_payment(
        &self,
        input: &OrdersByPaymentRequest,
    ) -> Result<Vec<Order>, ClientError> {
        collect_pages(async |after| {
            let data: OrdersData = self
                .execute(
                    "OrdersByPayment",
                    include_str!("shopify/orders_by_payment.graphql"),
                    input.variables(after.as_deref())?,
                )
                .await?;
            Ok(data.orders)
        })
        .await
        .map_err(|error| error.at(ORIGIN))
    }
    pub async fn orders_by_tag(
        &self,
        input: &OrdersByTagRequest,
    ) -> Result<Vec<TaggedOrderHeader>, ClientError> {
        collect_pages(async |after| {
            let data: OrdersByTagData = self
                .execute(
                    "OrdersByTag",
                    include_str!("shopify/orders_by_tag.graphql"),
                    input.variables(after.as_deref())?,
                )
                .await?;
            Ok(data.orders)
        })
        .await
        .map_err(|error| error.at(ORIGIN))
    }
    pub async fn sold_variants_in_range(
        &self,
        input: &SalesRangeRequest,
    ) -> Result<Vec<SalesOrderHeader>, ClientError> {
        collect_pages(async |after| {
            let data: SalesData = self
                .execute(
                    "SoldVariantsInRange",
                    include_str!("shopify/sold_variants_in_range.graphql"),
                    input.variables(after.as_deref())?,
                )
                .await?;
            Ok(data.orders)
        })
        .await
        .map_err(|error| error.at(ORIGIN))
    }
    pub async fn sales_order_lines(
        &self,
        input: &OrderLinesRequest,
    ) -> Result<Vec<SalesLine>, ClientError> {
        collect_pages(async |after| {
            let data: OrderLinesData<SalesLine> = self
                .execute(
                    "SalesOrderLines",
                    include_str!("shopify/sales_order_lines.graphql"),
                    input.variables(after.as_deref())?,
                )
                .await?;
            require_lines(data, &input.order_id)
        })
        .await
        .map_err(|error| error.at(ORIGIN))
    }
    pub async fn tagged_order_lines(
        &self,
        input: &OrderLinesRequest,
    ) -> Result<Vec<TaggedOrderLineItem>, ClientError> {
        collect_pages(async |after| {
            let data: OrderLinesData<TaggedOrderLineItem> = self
                .execute(
                    "TaggedOrderLines",
                    include_str!("shopify/tagged_order_lines.graphql"),
                    input.variables(after.as_deref())?,
                )
                .await?;
            require_lines(data, &input.order_id)
        })
        .await
        .map_err(|error| error.at(ORIGIN))
    }
    pub async fn find_inventory_to_activate(
        &self,
        input: &InventoryDiscoveryRequest,
    ) -> Result<Vec<InventoryActivationCandidate>, ClientError> {
        collect_pages(async |after| {
            let data: InventoryData = self
                .execute(
                    "FindInventoryToActivate",
                    include_str!("shopify/find_inventory_to_activate.graphql"),
                    input.variables(after.as_deref())?,
                )
                .await?;
            Ok(data
                .location
                .ok_or(ClientError::InvalidInput("location was not found"))?
                .inventory_levels)
        })
        .await
        .map_err(|error| error.at(ORIGIN))
    }
    pub async fn replace_order_tags(
        &self,
        input: &ReplaceOrderTagsRequest,
    ) -> Result<OrderUpdateData, ClientError> {
        async {
            self.execute(
                "ReplaceOrderTags",
                include_str!("shopify/replace_order_tags.graphql"),
                input.variables()?,
            )
            .await
        }
        .await
        .map_err(|error: ClientError| error.at(ORIGIN))
    }
    pub async fn activate_existing_inventory(
        &self,
        input: &ActivateInventoryRequest,
    ) -> Result<ActivationData, ClientError> {
        async {
            self.execute(
                "ActivateExistingInventory",
                include_str!("shopify/activate_existing_inventory.graphql"),
                input.variables()?,
            )
            .await
        }
        .await
        .map_err(|error: ClientError| error.at(ORIGIN))
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
fn require_lines<T>(data: OrderLinesData<T>, id: &ShopifyId) -> Result<Connection<T>, ClientError> {
    let order = data.order.ok_or(ClientError::Protocol(
        "order is missing during line-item pagination",
    ))?;
    if &order.id != id {
        return Err(ClientError::Protocol(
            "line items belong to a different order",
        ));
    }
    Ok(order.line_items)
}
#[cfg(test)]
#[path = "../tests/client/shopify.rs"]
mod tests;

mod report;
