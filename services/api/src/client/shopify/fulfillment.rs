use super::ShopifyClient;
use crate::{
    client::ClientError,
    models::{
        fulfillment::{
            CreateData, OrderData, OrderSummary, OrdersPage, ShippingOrder, TagData,
            mutation_result,
        },
        shopify::{Connection, PageRequest},
    },
};
use serde_json::{Value, json};

impl ShopifyClient {
    pub(crate) async fn reship_order(
        &self,
        id: &str,
    ) -> Result<crate::models::fulfillment::ReshipOrderData, ClientError> {
        self.execute(
            "SalesOrderLines",
            include_str!("sales_order_lines.graphql"),
            json!({"id":id,"includeLines":false,"includeContact":true}),
        )
        .await
    }

    pub(crate) async fn shipping_order_items(
        &self,
        id: &str,
        after: Option<String>,
    ) -> Result<crate::models::fulfillment::ShippingItemsData, ClientError> {
        self.execute(
            "ShippingOrderItems",
            include_str!("shipping_order_items.graphql"),
            json!({"id":id,"first":250,"after":after}),
        )
        .await
    }
    pub(crate) async fn shipping_order_methods(
        &self,
        id: &str,
        after: Option<String>,
    ) -> Result<crate::models::fulfillment::ShippingMethodsData, ClientError> {
        self.execute(
            "ShippingOrderMethods",
            include_str!("shipping_order_methods.graphql"),
            json!({"id":id,"first":250,"after":after}),
        )
        .await
    }
    pub(crate) async fn fulfillment_order_items(
        &self,
        id: &str,
        after: Option<String>,
    ) -> Result<crate::models::fulfillment::FulfillmentItemsData, ClientError> {
        self.execute(
            "FulfillmentOrderItems",
            include_str!("fulfillment_order_items.graphql"),
            json!({"id":id,"first":250,"after":after}),
        )
        .await
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
                include_str!("list_orders.graphql"),
                json!({"first":page.first,"after":page.after,"search":search}),
            )
            .await?;
        Ok(data.orders)
    }
    pub(crate) async fn shipping_orders(
        &self,
        after: Option<String>,
    ) -> Result<Connection<ShippingOrder>, ClientError> {
        let data: OrdersPage<ShippingOrder> = self.execute("ShippingOrders",include_str!("shipping_orders.graphql"),json!({"first":250,"after":after,"search":"status:open AND financial_status:paid AND fulfillment_status:unfulfilled AND tag:\"PACKING\""})).await?;
        Ok(data.orders)
    }
    pub(crate) async fn fulfillment_orders(
        &self,
        id: &str,
        after: Option<String>,
    ) -> Result<OrderData, ClientError> {
        self.execute(
            "FulfillmentOrders",
            include_str!("fulfillment_orders.graphql"),
            json!({"id":id,"first":250,"after":after}),
        )
        .await
    }
    pub(crate) async fn add_order_tags(
        &self,
        id: &str,
        tags: &[&str],
    ) -> Result<String, ClientError> {
        let data: TagData = self
            .execute(
                "AddOrderTags",
                include_str!("add_order_tags.graphql"),
                json!({"id":id,"tags":tags}),
            )
            .await?;
        mutation_result(data.tags_add.user_errors, data.tags_add.node)
    }
    pub(crate) async fn remove_order_tags(
        &self,
        id: &str,
        tags: &[&str],
    ) -> Result<String, ClientError> {
        let data: TagData = self
            .execute(
                "RemoveOrderTags",
                include_str!("remove_order_tags.graphql"),
                json!({"id":id,"tags":tags}),
            )
            .await?;
        mutation_result(data.tags_add.user_errors, data.tags_add.node)
    }
    pub(crate) async fn create_fulfillment(&self, input: Value) -> Result<String, ClientError> {
        let data: CreateData = self
            .execute(
                "CreateFulfillment",
                include_str!("create_fulfillment.graphql"),
                json!({"fulfillment":input}),
            )
            .await?;
        mutation_result(
            data.fulfillment_create.user_errors,
            data.fulfillment_create.fulfillment,
        )
    }
}
