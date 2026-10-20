use crate::components::TableData;
use domain::dto::accounting::{
    AccountingGateway, AccountingItem, AccountingKind, AccountingTransaction,
};

const TRANSACTION_HEADERS: &[&str] = &[
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
const ITEM_HEADERS: &[&str] = &[
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
) -> TableData {
    TableData {
        headers: TRANSACTION_HEADERS,
        rows: rows
            .into_iter()
            .map(|row| {
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
            })
            .collect(),
    }
}
pub fn item_table(rows: &[AccountingItem]) -> TableData {
    TableData {
        headers: ITEM_HEADERS,
        rows: rows
            .iter()
            .map(|row| {
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
            })
            .collect(),
    }
}
pub fn encode_archive(transactions: &TableData, items: &TableData) -> Result<Vec<u8>, String> {
    use std::io::{Cursor, Write};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, table) in [("transactions.csv", transactions), ("items.csv", items)] {
        archive
            .start_file(name, options)
            .map_err(|error| error.to_string())?;
        let csv = crate::pages::reports::encode_table(table.headers, &table.rows);
        archive
            .write_all(csv.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    archive
        .finish()
        .map(Cursor::into_inner)
        .map_err(|error| error.to_string())
}

fn kind(kind: AccountingKind) -> &'static str {
    match kind {
        AccountingKind::Sale => "SALE",
        AccountingKind::Refund => "REFUND",
    }
}
#[cfg(test)]
#[path = "../../tests/pages/accounting/export.rs"]
mod tests;
