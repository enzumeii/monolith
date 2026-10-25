use super::{Payment, money, protocol};
use crate::{
    client::paypal::PaypalClient,
    error::{AppError, upstream},
    models::accounting::AccountingDateRange,
    models::{
        CurrencyCode,
        checkout_completion::{ExactMoney, currency_precision},
        paypal::{TransactionDetail, TransactionEvent},
    },
    services::{self, pagination::NumberedPage},
    transformers,
};
use chrono::{Duration, Utc};
use domain::dto::{AccountingGateway, AccountingKind};
use std::collections::HashSet;

pub(super) async fn collect(
    client: &PaypalClient,
    range: &AccountingDateRange,
) -> Result<Vec<Payment>, AppError> {
    let mut start = range.start.with_timezone(&Utc);
    let end = range.end.with_timezone(&Utc);
    let mut details = Vec::new();
    let mut seen = HashSet::new();
    loop {
        let window_end = start
            .checked_add_signed(Duration::days(31))
            .unwrap_or(end)
            .min(end);
        services::pagination::for_each_numbered_page(
            |page| async move {
                let response = client
                    .get_transactions(start, window_end, 500, page)
                    .await
                    .map_err(|error| upstream(error))?;
                Ok(NumberedPage {
                    nodes: response.transaction_details,
                    page: response.page,
                    total_pages: response.total_pages,
                })
            },
            |nodes| {
                for detail in nodes {
                    if seen.insert(identity(&detail)) {
                        details.push(detail);
                    }
                }
                Ok(())
            },
            |error| upstream(error),
        )
        .await?;
        if window_end == end {
            break;
        }
        start = window_end;
    }
    records(details, range)
        .into_iter()
        .map(|detail| payment(&detail))
        .collect()
}

fn identity(detail: &TransactionDetail) -> String {
    let t = &detail.transaction;
    serde_json::json!([
        t.transaction_id,
        t.transaction_event_code,
        t.transaction_initiation_date,
        t.transaction_updated_date,
        t.transaction_amount.currency_code,
        t.transaction_amount.value,
        t.transaction_status,
        t.fee_amount
            .as_ref()
            .map(|fee| (&fee.currency_code, &fee.value))
    ])
    .to_string()
}

fn records(details: Vec<TransactionDetail>, range: &AccountingDateRange) -> Vec<TransactionDetail> {
    details
        .into_iter()
        .filter(|detail| {
            let info = &detail.transaction;
            !matches!(
                info.transaction_event_code,
                TransactionEvent::BankWithdrawal
                    | TransactionEvent::ManualWithdrawal
                    | TransactionEvent::UserCurrencyConversion
                    | TransactionEvent::CurrencyConversion
            ) && info.transaction_initiation_date >= range.start
                && info.transaction_initiation_date <= range.end
        })
        .collect()
}

fn payment(detail: &TransactionDetail) -> Result<Payment, AppError> {
    let info = &detail.transaction;
    let currency = &info.transaction_amount.currency_code;
    let factor = precision_factor(currency)?;
    let gross = rounded(money(&info.transaction_amount.value)?, factor)?;
    let fee = info
        .fee_amount
        .as_ref()
        .map(|fee| {
            if fee.currency_code != *currency {
                return Err(protocol("incompatible PayPal fee currency"));
            }
            rounded(-money(&fee.value)?, factor)
        })
        .transpose()?
        .unwrap_or_default();
    let shipping = info
        .shipping_amount
        .as_ref()
        .map(|shipping| {
            if shipping.currency_code != *currency {
                return Err(protocol("incompatible PayPal shipping currency"));
            }
            rounded(money(&shipping.value)?, factor)
        })
        .transpose()?;
    let mut transaction = transformers::accounting::transaction(
        AccountingGateway::Paypal,
        if info.transaction_event_code == TransactionEvent::Refund {
            AccountingKind::Refund
        } else {
            AccountingKind::Sale
        },
        None,
    );
    transaction.transaction_date = info.transaction_initiation_date.to_rfc3339();
    transaction
        .payment_reference
        .clone_from(&info.transaction_id);
    transaction.description = paypal_description(
        &info.transaction_event_code,
        detail
            .payer
            .as_ref()
            .and_then(|p| p.email_address.as_deref()),
        currency.as_str(),
    );
    currency.as_str().clone_into(&mut transaction.currency);
    transaction.net_paid = rounded(gross - fee, factor)?;
    transaction.fee = fee;
    transaction.gross_paid = gross;
    transaction.shipping_fee = shipping;
    let source_money = source_money(detail)?;
    Ok(Payment {
        transaction,
        currency: currency.as_str().to_owned(),
        source_money,
    })
}

fn source_money(detail: &TransactionDetail) -> Result<Option<ExactMoney>, AppError> {
    let info = &detail.transaction;
    if info.transaction_status != "S"
        || !matches!(
            info.transaction_event_code,
            TransactionEvent::GeneralPayment
                | TransactionEvent::MobilePayment
                | TransactionEvent::ExpressCheckoutPayment
        )
    {
        return Ok(None);
    }
    let currency = info.transaction_amount.currency_code.as_str();
    let Some(precision) = currency_precision(currency) else {
        return Ok(None);
    };
    let mut source = ExactMoney::new(0, currency, precision)?;
    let Ok(minor) = source.parse_amount(&info.transaction_amount.value) else {
        return Ok(None);
    };
    source.minor = minor;
    Ok((source.minor > 0).then_some(source))
}

fn precision_factor(currency: &CurrencyCode) -> Result<f64, AppError> {
    match currency {
        CurrencyCode::HUF | CurrencyCode::JPY | CurrencyCode::TWD => Ok(1.0),
        CurrencyCode::BHD
        | CurrencyCode::IQD
        | CurrencyCode::JOD
        | CurrencyCode::KWD
        | CurrencyCode::LYD
        | CurrencyCode::OMR
        | CurrencyCode::TND => Ok(1000.0),
        CurrencyCode::AUD
        | CurrencyCode::BRL
        | CurrencyCode::CAD
        | CurrencyCode::CHF
        | CurrencyCode::CNY
        | CurrencyCode::CZK
        | CurrencyCode::DKK
        | CurrencyCode::EUR
        | CurrencyCode::GBP
        | CurrencyCode::HKD
        | CurrencyCode::ILS
        | CurrencyCode::MYR
        | CurrencyCode::MXN
        | CurrencyCode::NOK
        | CurrencyCode::NZD
        | CurrencyCode::PHP
        | CurrencyCode::PLN
        | CurrencyCode::SEK
        | CurrencyCode::SGD
        | CurrencyCode::THB
        | CurrencyCode::USD => Ok(100.0),
        _ => Err(protocol("unsupported PayPal accounting currency")),
    }
}

fn rounded(value: f64, factor: f64) -> Result<f64, AppError> {
    let result = (value * factor).round() / factor;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(protocol("invalid PayPal accounting amount"))
    }
}

fn paypal_description(event: &TransactionEvent, email: Option<&str>, currency: &str) -> String {
    let dispute_label = match event {
        TransactionEvent::PaymentReviewHold => Some("Payment Held for fraudulent activity"),
        TransactionEvent::PaymentReviewRelease => Some("Payment Released from fraud review"),
        TransactionEvent::DisputeHold => {
            Some("Hold the amount for chargeback review from customer Dispute")
        }
        TransactionEvent::DisputeHoldCancellation => {
            Some("Cancel the charge back and release the amount back to us")
        }
        TransactionEvent::PaymentReversal => {
            Some("Refund the amount to customer due to dispute lost")
        }
        TransactionEvent::DisputeFee => Some("Dispute Fee"),
        _ => None,
    };
    if let Some(label) = dispute_label {
        return format!("{label} in currency {currency}");
    }
    match event {
        TransactionEvent::GeneralPayment | TransactionEvent::MobilePayment => format!(
            "PAID TO {} in currency {currency}",
            email.unwrap_or_default()
        ),
        _ => email.unwrap_or_default().to_owned(),
    }
}
#[cfg(test)]
#[path = "../../tests/services/accounting/paypal.rs"]
mod tests;
