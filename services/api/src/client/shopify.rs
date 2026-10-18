use crate::{
    client::{ClientError, ClientErrorKind, GraphqlClient, GraphqlRequest, HttpClient},
    config::ShopifyConfig,
    models::shopify::{
        ActivateInventoryRequest, ActivationData, Connection, InventoryActivationCandidate,
        InventoryData, InventoryDiscoveryRequest, ListProductsRequest, Order, OrderLinesData,
        OrderLinesRequest, OrderUpdateData, OrdersByPaymentRequest, OrdersByTagData,
        OrdersByTagRequest, OrdersData, PageRequest, Product, ProductsData,
        ReplaceOrderTagsRequest, SalesData, SalesLine, SalesOrderHeader, SalesRangeRequest,
        ShopifyId, TaggedOrder,
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
            .map_err(|_| ClientError::new(ClientErrorKind::InvalidInput("invalid Shopify URL")))?;
        let token = config
            .access_token
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                ClientError::new(ClientErrorKind::InvalidInput("Shopify token is missing"))
            })?;
        let mut value = HeaderValue::from_str(token).map_err(|_| {
            ClientError::new(ClientErrorKind::InvalidInput(
                "invalid Shopify token header",
            ))
        })?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert("X-Shopify-Access-Token", value);
        Ok(Self {
            graphql: GraphqlClient::new(
                HttpClient::new(Duration::from_secs(config.timeout_secs))?,
                endpoint,
            ),
            headers,
        })
    }
    pub async fn products_with_variants(
        &self,
        input: &ListProductsRequest,
        page: &PageRequest,
    ) -> Result<Connection<Product>, ClientError> {
        let data: ProductsData = self
            .execute(
                "ProductsWithVariants",
                include_str!("shopify/products_with_variants.graphql"),
                input.variables(page)?,
            )
            .await?;
        Ok(data.products)
    }
    pub async fn orders_by_payment(
        &self,
        input: &OrdersByPaymentRequest,
        page: &PageRequest,
    ) -> Result<Connection<Order>, ClientError> {
        let data: OrdersData = self
            .execute(
                "OrdersByPayment",
                include_str!("shopify/orders_by_payment.graphql"),
                input.variables(page)?,
            )
            .await?;
        Ok(data.orders)
    }
    pub async fn orders_by_tag(
        &self,
        input: &OrdersByTagRequest,
        page: &PageRequest,
    ) -> Result<Connection<TaggedOrder>, ClientError> {
        let data: OrdersByTagData = self
            .execute(
                "OrdersByTag",
                include_str!("shopify/orders_by_tag.graphql"),
                input.variables(page)?,
            )
            .await?;
        Ok(data.orders)
    }
    pub async fn sold_variants_in_range(
        &self,
        input: &SalesRangeRequest,
        page: &PageRequest,
    ) -> Result<Connection<SalesOrderHeader>, ClientError> {
        let data: SalesData = self
            .execute(
                "SoldVariantsInRange",
                include_str!("shopify/sold_variants_in_range.graphql"),
                input.variables(page)?,
            )
            .await?;
        Ok(data.orders)
    }
    pub async fn sales_order_lines(
        &self,
        input: &OrderLinesRequest,
    ) -> Result<OrderLinesData<SalesLine>, ClientError> {
        let data: OrderLinesData<SalesLine> = self
            .execute(
                "SalesOrderLines",
                include_str!("shopify/sales_order_lines.graphql"),
                input.variables()?,
            )
            .await?;
        Ok(data)
    }
    pub async fn find_inventory_to_activate(
        &self,
        input: &InventoryDiscoveryRequest,
        page: &PageRequest,
    ) -> Result<Option<Connection<InventoryActivationCandidate>>, ClientError> {
        let data: InventoryData = self
            .execute(
                "FindInventoryToActivate",
                include_str!("shopify/find_inventory_to_activate.graphql"),
                input.variables(page)?,
            )
            .await?;
        Ok(data.location.map(|location| location.inventory_levels))
    }
    pub async fn location_exists(&self, id: &ShopifyId) -> Result<bool, ClientError> {
        id.require_kind("Location")?;
        let data: crate::models::shopify::LocationData = self
            .execute(
                "Location",
                include_str!("shopify/location.graphql"),
                serde_json::json!({"id":id}),
            )
            .await?;
        Ok(data.location.is_some())
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

mod fulfillment;
