use crate::{
    models::{
        Sku, SkuCategory,
        shopify::{AccountingLineItem, Order},
    },
    utils::contact::{format_address, phone_number},
};
use domain::dto::{AccountingGateway, AccountingItem, AccountingKind, AccountingTransaction};

pub(crate) fn transaction(
    gateway: AccountingGateway,
    kind: AccountingKind,
    shipping_fee: Option<f64>,
) -> AccountingTransaction {
    AccountingTransaction {
        gateway,
        kind,
        po_number: String::new(),
        po_date: None,
        transaction_date: String::new(),
        payment_reference: String::new(),
        description: String::new(),
        shopify_order_id: 0,
        shopify_order_number: 0,
        customer_name: String::new(),
        customer_address: String::new(),
        customer_phone: 0,
        currency: String::new(),
        exchange_rate: None,
        net_paid: 0.0,
        fee: 0.0,
        vat: 0.0,
        gross_paid: 0.0,
        shipping_fee,
        order_adjusted: false,
        shopify_order_tags: vec![],
    }
}

pub(crate) fn order_details(
    tx: &mut AccountingTransaction,
    order: &Order,
    id: u64,
    shipping: Option<f64>,
    adjusted: bool,
) {
    tx.shopify_order_id = id;
    tx.shopify_order_number = order.number;
    tx.shopify_order_tags.clone_from(&order.tags);
    tx.customer_name = order
        .shipping_address
        .as_ref()
        .and_then(|address| address.name.clone())
        .unwrap_or_default();
    tx.customer_address = address(order);
    tx.customer_phone = phone(order);
    tx.shipping_fee = shipping;
    tx.order_adjusted = adjusted;
}

pub(crate) fn item(
    line: &AccountingLineItem,
    quantity: i64,
    price: f64,
    kind: AccountingKind,
    date: &str,
    order_id: u64,
    reference: &str,
) -> AccountingItem {
    let sku = line.sku.clone().map(Sku::into_string).unwrap_or_default();
    AccountingItem {
        quantity,
        price,
        currency: line
            .original_unit_price_set
            .shop_money
            .currency_code
            .clone(),
        title: line.title.clone(),
        variant: line.variant_title.clone().unwrap_or_default(),
        category: SkuCategory::from_sku(&sku).as_str().to_owned(),
        sku,
        kind,
        transaction_date: date.to_owned(),
        shopify_order_id: order_id,
        payment_reference: reference.to_owned(),
    }
}

fn address(order: &Order) -> String {
    let Some(address) = &order.shipping_address else {
        return String::new();
    };
    format_address(&[
        address.address1.as_deref(),
        address.address2.as_deref(),
        address.city.as_deref(),
        address.province_code.as_deref(),
        address.country.as_deref(),
        address.zip.as_deref(),
    ])
}

fn phone(order: &Order) -> u64 {
    phone_number(
        order
            .shipping_address
            .as_ref()
            .and_then(|a| a.phone.as_deref())
            .unwrap_or_default(),
    )
}
