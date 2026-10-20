use super::{Payment, empty_transaction, money, protocol};
use crate::{
    client::paypal::PaypalClient, error::AppError, models::paypal::TransactionDetail,
    services::common::upstream, validations::accounting::AccountingDateRange,
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
        let mut page = 1;
        loop {
            let response = client
                .get_transactions(start, window_end, 500, page)
                .await
                .map_err(|error| upstream(error))?;
            if response.page != page
                || (response.total_pages == 0 && !response.transaction_details.is_empty())
                || (response.total_pages > 0 && page > response.total_pages)
                || (page < response.total_pages && response.transaction_details.is_empty())
            {
                return Err(protocol("invalid PayPal pagination metadata"));
            }
            for detail in response.transaction_details {
                if seen.insert(identity(&detail)) {
                    details.push(detail);
                }
            }
            if page >= response.total_pages {
                break;
            }
            page = page
                .checked_add(1)
                .ok_or_else(|| protocol("invalid PayPal pagination metadata"))?;
        }
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
                info.transaction_event_code.as_str(),
                "T0400" | "T0403" | "T0201" | "T0200"
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
    let mut transaction = empty_transaction(
        AccountingGateway::Paypal,
        if info.transaction_event_code == "T1107" {
            AccountingKind::Refund
        } else {
            AccountingKind::Sale
        },
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
        currency,
    );
    transaction.currency.clone_from(currency);
    transaction.net_paid = rounded(gross - fee, factor)?;
    transaction.fee = fee;
    transaction.gross_paid = gross;
    transaction.shipping_fee = shipping;
    Ok(Payment {
        transaction,
        currency: currency.clone(),
    })
}

fn precision_factor(currency: &str) -> Result<f64, AppError> {
    match currency {
        "HUF" | "JPY" | "TWD" => Ok(1.0),
        "BHD" | "IQD" | "JOD" | "KWD" | "LYD" | "OMR" | "TND" => Ok(1000.0),
        "AUD" | "BRL" | "CAD" | "CHF" | "CNY" | "CZK" | "DKK" | "EUR" | "GBP" | "HKD" | "ILS"
        | "MYR" | "MXN" | "NOK" | "NZD" | "PHP" | "PLN" | "SEK" | "SGD" | "THB" | "USD" => {
            Ok(100.0)
        }
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

fn paypal_description(event: &str, email: Option<&str>, currency: &str) -> String {
    let dispute_label = match event {
        "T2105" => Some("Payment Held for fraudulent activity"),
        "T2106" => Some("Payment Released from fraud review"),
        "T1110" => Some("Hold the amount for chargeback review from customer Dispute"),
        "T1111" => Some("Cancel the charge back and release the amount back to us"),
        "T1106" => Some("Refund the amount to customer due to dispute lost"),
        "T0114" => Some("Dispute Fee"),
        _ => None,
    };
    if let Some(label) = dispute_label {
        return format!("{label} in currency {currency}");
    }
    match event {
        "T0000" | "T0011" => format!(
            "PAID TO {} in currency {currency}",
            email.unwrap_or_default()
        ),
        _ => email.unwrap_or_default().to_owned(),
    }
}
#[cfg(test)]
#[path = "../../tests/services/accounting/paypal.rs"]
mod tests;
