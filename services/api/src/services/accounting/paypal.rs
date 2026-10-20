use super::{Payment, empty_transaction, money, protocol};
use crate::{
    client::paypal::PaypalClient, error::AppError, models::paypal::TransactionDetail,
    services::common::upstream, validations::accounting::AccountingDateRange,
};
use chrono::{DateTime, Duration, FixedOffset, Months, Utc};
use domain::dto::{AccountingGateway, AccountingKind};
use std::collections::HashSet;

pub(super) async fn collect(
    client: &PaypalClient,
    range: &AccountingDateRange,
) -> Result<Vec<Payment>, AppError> {
    let mut start = range
        .start
        .checked_sub_months(Months::new(1))
        .unwrap_or(range.start)
        .with_timezone(&Utc);
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
    records(details, range)?
        .into_iter()
        .map(|(detail, rate, payout_date)| payment(&detail, rate, payout_date))
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

fn records(
    mut details: Vec<TransactionDetail>,
    range: &AccountingDateRange,
) -> Result<Vec<(TransactionDetail, f64, DateTime<FixedOffset>)>, AppError> {
    details.sort_by(|a, b| {
        b.transaction
            .transaction_initiation_date
            .cmp(&a.transaction.transaction_initiation_date)
    });
    let mut po_date = None;
    let mut withdraw_thb = 0.0;
    let mut withdraw_usd = 0.0;
    let mut records = Vec::new();
    for detail in details {
        let info = &detail.transaction;
        let event = info.transaction_event_code.as_str();
        let mut rate = 1.0;
        let mut record_date = None;
        match event {
            "T0400" | "T0403" => po_date = Some(info.transaction_initiation_date),
            "T0201" => {
                let value = money(&info.transaction_amount.value)?.abs();
                if info.transaction_amount.currency_code == "USD" {
                    withdraw_usd = value;
                } else {
                    withdraw_thb = value;
                }
            }
            "T0006" | "T1107" => {
                if !matches!(
                    info.transaction_amount.currency_code.as_str(),
                    "USD" | "SGD"
                ) {
                    record_date = Some(info.transaction_initiation_date);
                } else if let Some(withdrawal_date) = po_date
                    && withdraw_thb != 0.0
                    && withdraw_usd != 0.0
                {
                    rate = (withdraw_thb / withdraw_usd * 100.0).round() / 100.0;
                    record_date = Some(withdrawal_date);
                }
            }
            "T0200" => {}
            _ => record_date = Some(info.transaction_initiation_date),
        }
        if let Some(date) = record_date.filter(|date| *date >= range.start && *date <= range.end)
            && !matches!(event, "T0400" | "T0403" | "T0201" | "T0200")
        {
            records.push((detail, rate, date));
        }
    }
    records.sort_by(|a, b| {
        a.0.transaction
            .transaction_initiation_date
            .cmp(&b.0.transaction.transaction_initiation_date)
    });
    Ok(records)
}

fn payment(
    detail: &TransactionDetail,
    rate: f64,
    payout_date: DateTime<FixedOffset>,
) -> Result<Payment, AppError> {
    let info = &detail.transaction;
    let gross = money(&info.transaction_amount.value)? * rate;
    let fee = info
        .fee_amount
        .as_ref()
        .map(|fee| money(&fee.value))
        .transpose()?
        .unwrap_or_default()
        .abs()
        * rate;
    if !rate.is_finite() || rate <= 0.0 || !gross.is_finite() || !fee.is_finite() {
        return Err(protocol("invalid PayPal accounting amount"));
    }
    let mut transaction = empty_transaction(
        AccountingGateway::Paypal,
        if info.transaction_event_code == "T1107" {
            AccountingKind::Refund
        } else {
            AccountingKind::Sale
        },
    );
    transaction.po_date = payout_date.to_rfc3339();
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
        &info.transaction_amount.currency_code,
    );
    transaction.exchange_rate = rate;
    transaction.net_paid = gross - fee;
    transaction.fee = fee;
    transaction.gross_paid = gross;
    Ok(Payment {
        transaction,
        currency: info.transaction_amount.currency_code.clone(),
    })
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
