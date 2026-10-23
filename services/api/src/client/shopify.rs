use crate::models::shopify::{
    CreateData, CreateDraftOrderData, CreateDraftOrderInput, DraftOrderCreatePayload, OrderData,
    OrderSummary, OrdersPage, ShippingOrder, TagData,
};
use crate::{
    client::{ClientError, ClientErrorKind, GraphqlClient, GraphqlRequest, HttpClient},
    config::ShopifyConfig,
    models::shopify::{
        ActivateInventoryRequest, ActivationData, Connection, InventoryActivationCandidate,
        InventoryData, InventoryDiscoveryRequest, ListProductsRequest, Order, OrdersByTagData,
        OrdersByTagRequest, OrdersData, PageRequest, Product, ProductsData, ShopifyId, TaggedOrder,
    },
};
use reqwest::{
    Url,
    header::{HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub(crate) enum TagAction {
    Add,
    Remove,
}
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
    pub(crate) async fn catalogue_products(
        &self,
        location_id: &ShopifyId,
        search: Option<&str>,
        page: &PageRequest,
    ) -> Result<Connection<crate::models::shopify::catalogue::CatalogueProduct>, ClientError> {
        page.validate()?;
        location_id.require_kind("Location")?;
        let data: crate::models::shopify::catalogue::CatalogueData = self.execute(
            "CatalogueProducts",
            include_str!("shopify/catalogue_products.graphql"),
            json!({"locationId":location_id,"search":search,"first":page.first,"after":page.after}),
        ).await?;
        Ok(data.products)
    }
    pub(crate) async fn accounting_orders(
        &self,
        search: &str,
        page: &PageRequest,
    ) -> Result<Connection<Order>, ClientError> {
        page.validate()?;
        self.orders_query(json!({"search": search, "first":page.first, "after":page.after}))
            .await
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
        match data.location {
            None => Ok(false),
            Some(location) if location.id == *id => Ok(true),
            Some(_) => Err(ClientError::new(ClientErrorKind::Protocol(
                "unexpected Shopify location ID",
            ))),
        }
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

    /// Creates a draft without application retries; a lost response may be ambiguous.
    pub async fn create_draft_order(
        &self,
        input: &CreateDraftOrderInput,
    ) -> Result<DraftOrderCreatePayload, ClientError> {
        for line in &input.line_items {
            line.variant_id.require_kind("ProductVariant")?;
        }
        let data: CreateDraftOrderData = self
            .graphql
            .execute_once(
                self.headers.clone(),
                &GraphqlRequest {
                    query: include_str!("shopify/create_draft_order.graphql"),
                    operation_name: "CreateDraftOrder",
                    variables: json!({"input": input}),
                },
            )
            .await?;
        Ok(data.draft_order_create)
    }

    pub(crate) async fn get_order_by_order_number(
        &self,
        number: u16,
    ) -> Result<Option<crate::models::shopify::ReshipOrder>, ClientError> {
        let data: OrdersPage<crate::models::shopify::ReshipOrder> = self.execute("ListOrders", include_str!("shopify/list_orders.graphql"), json!({"first":1,"after":null,"search":format!("name:{number}"),"includeContact":true,"includeTags":false})).await?;
        Ok(data.orders.nodes.into_iter().next())
    }

    pub(crate) async fn list_orders(
        &self,
        search: &str,
        page: &PageRequest,
    ) -> Result<Connection<OrderSummary>, ClientError> {
        page.validate()?;
        let data: OrdersPage<OrderSummary> = self
            .execute(
                "ListOrders",
                include_str!("shopify/list_orders.graphql"),
                json!({"first":page.first,"after":page.after,"search":search,"includeContact":true,"includeTags":false}),
            )
            .await?;
        Ok(data.orders)
    }
    pub(crate) async fn order_tags(
        &self,
        id: &ShopifyId,
    ) -> Result<Option<crate::models::shopify::OrderTags>, ClientError> {
        let number = id.number("Order")?;
        let data: OrdersPage<crate::models::shopify::OrderTags> = self
            .execute(
                "ListOrders",
                include_str!("shopify/list_orders.graphql"),
                json!({"first":1,"after":null,"search":format!("id:{number}"),"includeContact":false,"includeTags":true}),
            )
            .await?;
        Ok(data.orders.nodes.into_iter().next())
    }
    pub(crate) async fn shipping_orders(
        &self,
        after: Option<String>,
    ) -> Result<Connection<ShippingOrder>, ClientError> {
        let data: OrdersPage<ShippingOrder> = self.execute("ShippingOrders",include_str!("shopify/shipping_orders.graphql"),json!({"first":250,"after":after,"search":"status:open AND financial_status:paid AND fulfillment_status:unfulfilled AND tag:\"PACKING\""})).await?;
        Ok(data.orders)
    }
    pub(crate) async fn fulfillment_orders(
        &self,
        id: &ShopifyId,
        after: Option<String>,
    ) -> Result<OrderData, ClientError> {
        self.execute(
            "FulfillmentOrders",
            include_str!("shopify/fulfillment_orders.graphql"),
            json!({"id":id,"first":250,"after":after}),
        )
        .await
    }
    pub(crate) async fn edit_order_tags(
        &self,
        id: &str,
        tags: &[&str],
        action: TagAction,
    ) -> Result<String, ClientError> {
        let add = matches!(action, TagAction::Add);
        let data: TagData = self
            .execute(
                "EditOrderTags",
                include_str!("shopify/edit_order_tags.graphql"),
                json!({"id":id,"tags":tags,"add":add}),
            )
            .await?;
        let result = if add { data.tags_add } else { data.tags_remove };
        result
            .ok_or_else(|| ClientError::new(ClientErrorKind::Protocol("mutation result missing")))?
            .into_id()
    }
    pub(crate) async fn create_fulfillment(&self, input: Value) -> Result<String, ClientError> {
        let data: CreateData = self
            .execute(
                "CreateFulfillment",
                include_str!("shopify/create_fulfillment.graphql"),
                json!({"fulfillment":input}),
            )
            .await?;
        data.fulfillment_create.into_id()
    }

    pub(crate) async fn convention_variant(
        &self,
        id: &ShopifyId,
    ) -> Result<Option<crate::models::shopify::convention_variant::Variant>, ClientError> {
        id.number("ProductVariant")?;
        let data: crate::models::shopify::convention_variant::VariantData = self
            .execute(
                "ConventionVariant",
                include_str!("shopify/convention_variant.graphql"),
                json!({"id":id}),
            )
            .await?;
        if let Some(v) = &data.product_variant
            && (v.id != *id
                || v.id.number("ProductVariant").is_err()
                || v.product.id.number("Product").is_err())
        {
            return Err(ClientError::new(ClientErrorKind::Protocol(
                "unexpected Shopify variant identity",
            )));
        }
        Ok(data.product_variant)
    }
    pub(crate) async fn checkout_draft(
        &self,
        id: &ShopifyId,
    ) -> Result<Option<crate::models::shopify::CompletionDraft>, ClientError> {
        use crate::models::shopify::CompletionDraftData;
        id.require_kind("DraftOrder")?;
        let data: CompletionDraftData = self
            .execute(
                "CheckoutDraft",
                include_str!("shopify/checkout_draft.graphql"),
                json!({"id":id}),
            )
            .await?;
        if let Some(draft) = &data.draft_order {
            draft.id.require_kind("DraftOrder")?;
            if &draft.id != id {
                return Err(ClientError::new(ClientErrorKind::Protocol(
                    "draft identity mismatch",
                )));
            }
        }
        Ok(data.draft_order)
    }
    pub(crate) async fn complete_checkout_draft(
        &self,
        id: &ShopifyId,
    ) -> Result<ShopifyId, ClientError> {
        use crate::models::shopify::CompleteDraftData;
        id.require_kind("DraftOrder")?;
        let data: CompleteDraftData = self
            .execute_once(
                "CompleteCheckoutDraft",
                include_str!("shopify/complete_checkout_draft.graphql"),
                json!({"id":id,"paymentPending":false}),
            )
            .await?;
        let payload = data.draft_order_complete;
        if !payload.user_errors.is_empty() {
            return Err(ClientError::new(ClientErrorKind::Mutation {
                errors: payload.user_errors,
            }));
        }
        let draft = payload.draft_order.ok_or_else(|| {
            ClientError::new(ClientErrorKind::Protocol("completed draft missing"))
        })?;
        if &draft.id != id {
            return Err(ClientError::new(ClientErrorKind::Protocol(
                "completed draft identity mismatch",
            )));
        }
        let order = draft.order.ok_or_else(|| {
            ClientError::new(ClientErrorKind::Protocol("completed order missing"))
        })?;
        order.id.require_kind("Order")?;
        Ok(order.id)
    }
    pub(crate) async fn checkout_order(
        &self,
        id: &ShopifyId,
    ) -> Result<Option<crate::models::shopify::CompletionOrder>, ClientError> {
        use crate::models::shopify::CompletionOrderData;
        id.require_kind("Order")?;
        let data: CompletionOrderData = self
            .execute(
                "CheckoutOrder",
                include_str!("shopify/checkout_order.graphql"),
                json!({"id":id}),
            )
            .await?;
        if let Some(order) = &data.order {
            order.id.require_kind("Order")?;
            if &order.id != id {
                return Err(ClientError::new(ClientErrorKind::Protocol(
                    "order identity mismatch",
                )));
            }
        }
        Ok(data.order)
    }
    async fn execute_once<D: DeserializeOwned>(
        &self,
        operation_name: &str,
        query: &str,
        variables: Value,
    ) -> Result<D, ClientError> {
        self.graphql
            .execute_once(
                self.headers.clone(),
                &GraphqlRequest {
                    query,
                    operation_name,
                    variables,
                },
            )
            .await
    }
    async fn orders_query(&self, variables: Value) -> Result<Connection<Order>, ClientError> {
        let data: OrdersData = self
            .execute(
                "OrdersByPayment",
                include_str!("shopify/orders_by_payment.graphql"),
                variables,
            )
            .await?;
        Ok(data.orders)
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
