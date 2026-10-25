use domain::dto::orders::FulfillmentStatus;

pub(crate) struct OrderListFilter {
    pub tags: Option<Vec<String>>,
    pub fulfillment: Option<FulfillmentStatus>,
}
