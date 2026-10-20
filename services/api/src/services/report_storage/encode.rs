use super::{CSV_CONTENT_TYPE, GeneratedFile, XLSX_CONTENT_TYPE};
use crate::error::AppError;
use domain::dto::{AccountingReportResponse, ProductReportRow, ReportFileFormat};
use rust_xlsxwriter::Workbook;
use serde_json::Value;

pub const INVENTORY_HEADERS: [&str; 14] = [
    "category",
    "collection",
    "sku",
    "title",
    "variant",
    "cost",
    "cost_currency",
    "weight",
    "unit",
    "selling_price",
    "selling_price_currency",
    "quantity",
    "total_selling_price",
    "total_weight",
];

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

pub(crate) enum ReportData {
    Inventory(Vec<ProductReportRow>),
    Transaction(AccountingReportResponse),
}

pub(crate) fn generate(
    data: ReportData,
    format: ReportFileFormat,
) -> Result<Vec<GeneratedFile>, AppError> {
    let (kind, value) = match data {
        ReportData::Inventory(rows) => ("INVENTORY", serde_json::json!({"inventory": rows})),
        ReportData::Transaction(report) => (
            "TRANSACTION",
            serde_json::to_value(report).map_err(encoding_error)?,
        ),
    };
    encode_value(kind, &value, format)
}

fn encode_value(
    kind: &str,
    data: &Value,
    format: ReportFileFormat,
) -> Result<Vec<GeneratedFile>, AppError> {
    let tables = tables(kind, data)?;
    match format {
        ReportFileFormat::Csv => Ok(tables
            .into_iter()
            .map(|table| GeneratedFile {
                filename: format!("{}.csv", table.name.to_ascii_lowercase()),
                content_type: CSV_CONTENT_TYPE.into(),
                bytes: csv(&table).into_bytes(),
            })
            .collect()),
        ReportFileFormat::Xlsx => {
            let mut workbook = Workbook::new();
            for table in tables {
                let sheet = workbook.add_worksheet();
                sheet.set_name(table.name).map_err(encoding_error)?;
                for (index, header) in table.headers.iter().enumerate() {
                    sheet
                        .write_string(0, u16::try_from(index).map_err(encoding_error)?, *header)
                        .map_err(encoding_error)?;
                }
                for (index, row) in table.rows.iter().enumerate() {
                    let number = u32::try_from(index + 1).map_err(encoding_error)?;
                    for (column, cell) in row.iter().enumerate() {
                        sheet
                            .write_string(
                                number,
                                u16::try_from(column).map_err(encoding_error)?,
                                cell_string(cell),
                            )
                            .map_err(encoding_error)?;
                    }
                }
            }
            Ok(vec![GeneratedFile {
                filename: if kind == "INVENTORY" {
                    "inventory.xlsx"
                } else {
                    "transactions.xlsx"
                }
                .into(),
                content_type: XLSX_CONTENT_TYPE.into(),
                bytes: workbook.save_to_buffer().map_err(encoding_error)?,
            }])
        }
    }
}

struct Table {
    name: &'static str,
    headers: &'static [&'static str],
    rows: Vec<Vec<Value>>,
}

fn tables(kind: &str, data: &Value) -> Result<Vec<Table>, AppError> {
    let entries = match kind {
        "INVENTORY" => vec![(
            "Inventory",
            INVENTORY_HEADERS.as_slice(),
            array(data.get("inventory"))?,
        )],
        "TRANSACTION" => {
            let transactions = data.get("transactions").ok_or_else(invalid_data)?;
            let rows = if transactions.is_array() {
                array(Some(transactions))?
            } else {
                let mut rows = array(transactions.get("stripe"))?;
                rows.extend(array(transactions.get("paypal"))?);
                rows
            };
            vec![
                ("Transactions", TRANSACTION_HEADERS, rows),
                ("Items", ITEM_HEADERS, array(data.get("items"))?),
            ]
        }
        _ => return Err(invalid_data()),
    };
    Ok(entries
        .into_iter()
        .map(|(name, headers, rows)| Table {
            name,
            headers,
            rows: rows
                .iter()
                .map(|row| {
                    headers
                        .iter()
                        .map(|key| row.get(key).cloned().unwrap_or(Value::Null))
                        .collect()
                })
                .collect(),
        })
        .collect())
}
fn array(value: Option<&Value>) -> Result<Vec<Value>, AppError> {
    value
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(invalid_data)
}
fn csv(table: &Table) -> String {
    let mut result = table.headers.join(",") + "\r\n";
    for row in &table.rows {
        let cells = row
            .iter()
            .map(|value| {
                let text = cell_string(value);
                let text = if value.is_string()
                    && text
                        .trim_start()
                        .starts_with(['=', '+', '-', '@', '\t', '\r'])
                {
                    format!("'{text}")
                } else {
                    text
                };
                if text.contains([',', '"', '\r', '\n']) {
                    format!("\"{}\"", text.replace('"', "\"\""))
                } else {
                    text
                }
            })
            .collect::<Vec<_>>();
        result.push_str(&cells.join(","));
        result.push_str("\r\n");
    }
    result
}
fn cell_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}
fn invalid_data() -> AppError {
    axum_error("Saved report data is incomplete")
}
fn axum_error(message: &'static str) -> AppError {
    AppError::new(axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
fn encoding_error(error: impl Into<anyhow::Error>) -> AppError {
    axum_error("Report file generation failed").with_source(error)
}

#[cfg(test)]
#[path = "../../tests/services/report_storage/encode.rs"]
mod tests;
