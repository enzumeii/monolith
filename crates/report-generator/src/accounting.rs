use domain::dto::accounting::{
    AccountingGateway, AccountingItem, AccountingKind, AccountingTransaction,
};
pub const TRANSACTION_NUMERIC_COLUMNS: &[usize] = &[7, 8, 11, 13, 14, 15, 16, 17, 18];
pub const ITEM_NUMERIC_COLUMNS: &[usize] = &[0, 1, 9];

pub const TRANSACTION_HEADERS: &[&str] = &[
    "gateway",
    "type",
    "po_number",
    "po_date",
    "transaction_date",
    "payment_reference",
    "description",
    "shopify_order_id",
    "shopify_order_number",
    "customer_name",
    "customer_address",
    "customer_phone",
    "currency",
    "exchange_rate",
    "net_paid",
    "fee",
    "vat",
    "gross_paid",
    "shipping_fee",
    "order_adjusted",
];
pub const ITEM_HEADERS: &[&str] = &[
    "quantity",
    "price",
    "currency",
    "title",
    "variant",
    "category",
    "sku",
    "kind",
    "transaction_date",
    "shopify_order_id",
    "payment_reference",
];

pub fn transaction_table<'a>(
    rows: impl IntoIterator<Item = &'a AccountingTransaction>,
    null: &str,
) -> crate::Table {
    crate::Table {
        headers: TRANSACTION_HEADERS,
        rows: rows
            .into_iter()
            .map(|row| transaction_cells(row, null))
            .collect(),
    }
}
#[must_use]
pub fn transaction_cells(row: &AccountingTransaction, null: &str) -> Vec<String> {
    vec![
        match row.gateway {
            AccountingGateway::Stripe => "STRIPE",
            AccountingGateway::Paypal => "PAYPAL",
        }
        .into(),
        kind(row.kind).into(),
        row.po_number.clone(),
        row.po_date.as_deref().unwrap_or(null).into(),
        row.transaction_date.clone(),
        row.payment_reference.clone(),
        row.description.clone(),
        row.shopify_order_id.to_string(),
        row.shopify_order_number.to_string(),
        row.customer_name.clone(),
        row.customer_address.clone(),
        row.customer_phone.to_string(),
        row.currency.clone(),
        row.exchange_rate
            .map_or_else(|| null.into(), |value| value.to_string()),
        row.net_paid.to_string(),
        row.fee.to_string(),
        row.vat.to_string(),
        row.gross_paid.to_string(),
        row.shipping_fee
            .map_or_else(|| null.into(), |value| value.to_string()),
        row.order_adjusted.to_string(),
    ]
}
#[must_use]
pub fn item_table(rows: &[AccountingItem]) -> crate::Table {
    crate::Table {
        headers: ITEM_HEADERS,
        rows: rows.iter().map(item_cells).collect(),
    }
}
#[must_use]
pub fn item_cells(row: &AccountingItem) -> Vec<String> {
    vec![
        row.quantity.to_string(),
        row.price.to_string(),
        row.currency.clone(),
        row.title.clone(),
        row.variant.clone(),
        row.category.clone(),
        row.sku.clone(),
        kind(row.kind).into(),
        row.transaction_date.clone(),
        row.shopify_order_id.to_string(),
        row.payment_reference.clone(),
    ]
}
#[must_use]
pub fn encode_transactions<'a>(
    rows: impl IntoIterator<Item = &'a AccountingTransaction>,
) -> String {
    let mut csv = String::new();
    let _ = crate::csv::write_row(&mut csv, TRANSACTION_HEADERS.iter(), &[]);
    for row in rows {
        let _ = crate::csv::write_row(
            &mut csv,
            transaction_cells(row, ""),
            TRANSACTION_NUMERIC_COLUMNS,
        );
    }
    csv
}

#[must_use]
pub fn encode_items(rows: &[AccountingItem]) -> String {
    let mut csv = String::new();
    let _ = crate::csv::write_row(&mut csv, ITEM_HEADERS.iter(), &[]);
    for row in rows {
        let _ = crate::csv::write_row(&mut csv, item_cells(row), ITEM_NUMERIC_COLUMNS);
    }
    csv
}

fn kind(kind: AccountingKind) -> &'static str {
    match kind {
        AccountingKind::Sale => "SALE",
        AccountingKind::Refund => "REFUND",
    }
}

#[cfg(test)]
#[path = "tests/accounting.rs"]
mod tests;
