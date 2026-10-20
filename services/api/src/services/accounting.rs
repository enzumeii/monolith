use super::common::{shopify_client, upstream};
use crate::{
    client::{ClientError, ClientErrorKind},
    error::AppError,
    state::AppState,
    validations::accounting::AccountingDateRange,
};
use axum::http::StatusCode;
use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use domain::dto::{
    AccountingGateway, AccountingKind, AccountingReportResponse, AccountingTransaction,
};

struct Payment {
    transaction: AccountingTransaction,
    currency: String,
}

pub async fn report(
    state: &AppState,
    range: AccountingDateRange,
) -> Result<AccountingReportResponse, AppError> {
    let stripe = state
        .stripe
        .as_ref()
        .map(|value| value.as_deref())
        .map_err(|error| {
            AppError::new(StatusCode::SERVICE_UNAVAILABLE, "Stripe client unavailable")
                .with_source(error.clone())
        })?;
    let paypal = state
        .paypal
        .as_ref()
        .map(|value| value.as_deref())
        .map_err(|error| {
            AppError::new(StatusCode::SERVICE_UNAVAILABLE, "PayPal client unavailable")
                .with_source(error.clone())
        })?;
    if stripe.is_none() && paypal.is_none() {
        return Err(AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Accounting report provider unavailable",
        ));
    }
    let shopify = shopify_client(state)?;
    let mut payments = Vec::new();
    if let Some(client) = stripe {
        payments.extend(stripe::collect(client, &range).await?);
    }
    if let Some(client) = paypal {
        payments.extend(paypal::collect(client, &range).await?);
    }
    shopify::enrich(shopify, payments, &range).await
}

fn empty_transaction(gateway: AccountingGateway, kind: AccountingKind) -> AccountingTransaction {
    AccountingTransaction {
        gateway,
        kind,
        po_number: String::new(),
        po_date: String::new(),
        transaction_date: String::new(),
        payment_reference: String::new(),
        description: String::new(),
        shopify_order_id: 0,
        shopify_order_number: 0,
        customer_name: String::new(),
        customer_address: String::new(),
        customer_phone: 0,
        exchange_rate: 1.0,
        net_paid: 0.0,
        fee: 0.0,
        vat: 0.0,
        gross_paid: 0.0,
        shipping_fee: 0.0,
        order_adjusted: false,
    }
}

#[track_caller]
fn protocol(message: &'static str) -> AppError {
    upstream(ClientError::new(ClientErrorKind::Protocol(message)))
}

fn money(value: &str) -> Result<f64, AppError> {
    crate::utils::numbers::parse_number(value)
        .map_err(|error| upstream(error))?
        .as_f64()
        .ok_or_else(|| protocol("invalid report number"))
}

fn date(value: &str) -> Result<DateTime<FixedOffset>, AppError> {
    DateTime::parse_from_rfc3339(value).map_err(|_| protocol("invalid accounting date"))
}

fn cents(value: i64) -> f64 {
    serde_json::Number::from(value).as_f64().unwrap_or_default() / 100.0
}

fn quantity(value: i64) -> f64 {
    serde_json::Number::from(value).as_f64().unwrap_or_default()
}

fn iso_utc(timestamp: i64) -> String {
    Utc.timestamp_opt(timestamp, 0)
        .single()
        .unwrap_or(chrono::DateTime::<Utc>::UNIX_EPOCH)
        .to_rfc3339()
}
mod paypal;
mod shopify;
mod stripe;

#[cfg(test)]
#[path = "../tests/services/accounting.rs"]
mod tests;
