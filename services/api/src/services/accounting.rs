use crate::utils::{
    dates::{iso_utc, parse_rfc3339},
    numbers::{integer_as_f64 as quantity, minor_units as cents, parse_f64},
};
use crate::{
    client::{ClientError, ClientErrorKind},
    error::{AppError, upstream},
    validations::accounting::AccountingDateRange,
};
use domain::dto::{
    AccountingGateway, AccountingKind, AccountingReportResponse, AccountingTransaction,
};

struct Payment {
    transaction: AccountingTransaction,
    currency: String,
}

pub async fn report(
    shopify: &crate::client::shopify::ShopifyClient,
    stripe: &crate::client::stripe::StripeClient,
    paypal: &crate::client::paypal::PaypalClient,
    range: AccountingDateRange,
) -> Result<AccountingReportResponse, AppError> {
    let mut payments = Vec::new();
    payments.extend(stripe::collect(stripe, &range).await?);
    payments.extend(paypal::collect(paypal, &range).await?);
    let mut response = shopify::enrich(shopify, sort_payments(payments)?, &range).await?;
    let mut items = response
        .items
        .into_iter()
        .map(|item| Ok((date(&item.transaction_date)?, item)))
        .collect::<Result<Vec<_>, AppError>>()?;
    items.sort_by_key(|(date, _)| *date);
    response.items = items.into_iter().map(|(_, item)| item).collect();
    Ok(response)
}

fn sort_payments(payments: Vec<Payment>) -> Result<Vec<Payment>, AppError> {
    let mut rows = payments
        .into_iter()
        .map(|payment| {
            let payout = payment
                .transaction
                .po_date
                .as_deref()
                .map(date)
                .transpose()?;
            let transaction = date(&payment.transaction.transaction_date)?;
            Ok((payout, transaction, payment))
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    rows.sort_by_key(|(payout, transaction, _)| (payout.is_none(), *payout, *transaction));
    Ok(rows.into_iter().map(|(_, _, payment)| payment).collect())
}

fn empty_transaction(gateway: AccountingGateway, kind: AccountingKind) -> AccountingTransaction {
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
        shipping_fee: if gateway == AccountingGateway::Stripe {
            Some(0.0)
        } else {
            None
        },
        order_adjusted: false,
    }
}

#[track_caller]
fn protocol(message: &'static str) -> AppError {
    upstream(ClientError::new(ClientErrorKind::Protocol(message)))
}

fn money(value: &str) -> Result<f64, AppError> {
    parse_f64(value).map_err(|error| upstream(error))
}

fn date(value: &str) -> Result<chrono::DateTime<chrono::FixedOffset>, AppError> {
    parse_rfc3339(value).map_err(|error| upstream(error))
}
mod paypal;
mod shopify;
mod stripe;

#[cfg(test)]
#[path = "../tests/services/accounting.rs"]
mod tests;
