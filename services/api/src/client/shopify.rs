use crate::{
    client::{ClientError, GraphqlClient, GraphqlRequest, HttpClient},
    config::ShopifyConfig,
    models::shopify::{
        ActivateInventoryRequest, ActivationData, InventoryData, InventoryDiscoveryRequest,
        ListProductsRequest, OrderLinesData, OrderLinesRequest, OrderUpdateData,
        OrdersByPaymentRequest, OrdersByTagData, OrdersByTagRequest, OrdersData, ProductsData,
        ReplaceOrderTagsRequest, SalesData, SalesLine, SalesRangeRequest, TaggedOrderLineItem,
    },
};
use reqwest::{
    Url,
    header::{HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::Duration;

pub struct ShopifyClient {
    graphql: GraphqlClient,
    headers: HeaderMap,
}
impl ShopifyClient {
    pub fn new(config: &ShopifyConfig) -> Result<Self, ClientError> {
        let endpoint = Url::parse(&config.admin_api_url)
            .map_err(|_| ClientError::InvalidInput("invalid Shopify URL"))?;
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
    pub async fn products_with_variants(
        &self,
        input: &ListProductsRequest,
        after: Option<&str>,
    ) -> Result<ProductsData, ClientError> {
        self.execute(
            "ProductsWithVariants",
            include_str!("shopify/products_with_variants.graphql"),
            input.variables(after)?,
        )
        .await
    }
    pub async fn orders_by_payment(
        &self,
        input: &OrdersByPaymentRequest,
        after: Option<&str>,
    ) -> Result<OrdersData, ClientError> {
        self.execute(
            "OrdersByPayment",
            include_str!("shopify/orders_by_payment.graphql"),
            input.variables(after)?,
        )
        .await
    }
    pub async fn orders_by_tag(
        &self,
        input: &OrdersByTagRequest,
        after: Option<&str>,
    ) -> Result<OrdersByTagData, ClientError> {
        self.execute(
            "OrdersByTag",
            include_str!("shopify/orders_by_tag.graphql"),
            input.variables(after)?,
        )
        .await
    }
    pub async fn sold_variants_in_range(
        &self,
        input: &SalesRangeRequest,
        after: Option<&str>,
    ) -> Result<SalesData, ClientError> {
        self.execute(
            "SoldVariantsInRange",
            include_str!("shopify/sold_variants_in_range.graphql"),
            input.variables(after)?,
        )
        .await
    }
    pub async fn sales_order_lines(
        &self,
        input: &OrderLinesRequest,
        after: Option<&str>,
    ) -> Result<OrderLinesData<SalesLine>, ClientError> {
        self.execute(
            "SalesOrderLines",
            include_str!("shopify/sales_order_lines.graphql"),
            input.variables(after)?,
        )
        .await
    }
    pub async fn tagged_order_lines(
        &self,
        input: &OrderLinesRequest,
        after: Option<&str>,
    ) -> Result<OrderLinesData<TaggedOrderLineItem>, ClientError> {
        self.execute(
            "TaggedOrderLines",
            include_str!("shopify/tagged_order_lines.graphql"),
            input.variables(after)?,
        )
        .await
    }
    pub async fn replace_order_tags(
        &self,
        input: &ReplaceOrderTagsRequest,
    ) -> Result<OrderUpdateData, ClientError> {
        self.execute(
            "ReplaceOrderTags",
            include_str!("shopify/replace_order_tags.graphql"),
            input.variables()?,
        )
        .await
    }
    pub async fn find_inventory_to_activate(
        &self,
        input: &InventoryDiscoveryRequest,
        after: Option<&str>,
    ) -> Result<InventoryData, ClientError> {
        self.execute(
            "FindInventoryToActivate",
            include_str!("shopify/find_inventory_to_activate.graphql"),
            input.variables(after)?,
        )
        .await
    }
    pub async fn activate_existing_inventory(
        &self,
        input: &ActivateInventoryRequest,
    ) -> Result<ActivationData, ClientError> {
        self.execute(
            "ActivateExistingInventory",
            include_str!("shopify/activate_existing_inventory.graphql"),
            input.variables()?,
        )
        .await
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
#[cfg(test)]
#[path = "../tests/client/shopify.rs"]
mod tests;
