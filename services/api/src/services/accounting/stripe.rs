use super::{Payment, cents, iso_utc, protocol};
use crate::{
    client::stripe::StripeClient,
    error::{AppError, upstream},
    models::accounting::AccountingDateRange,
    models::{
        checkout_completion::{ExactMoney, currency_precision},
        stripe::{
            BalanceTransaction, BalanceTransactionKind, Charge, ChargeStatus, FeeKind, Payout,
        },
    },
    services::{self, pagination::CursorPage},
    transformers,
};
use domain::dto::{AccountingGateway, AccountingKind};

pub(super) async fn collect(
    client: &StripeClient,
    range: &AccountingDateRange,
) -> Result<Vec<Payment>, AppError> {
    services::pagination::collect_cursor_pages(
        |cursor| async move {
            let page = client
                .list_payouts(
                    range.start.timestamp(),
                    range.end.timestamp(),
                    Some("paid"),
                    100,
                    cursor.as_deref(),
                )
                .await
                .map_err(|error| upstream(error))?;
            Ok(CursorPage {
                nodes: page.data,
                has_more: page.has_more,
            })
        },
        |payout: &Payout| payout.id.clone(),
        |payouts| async move {
            let mut payments = Vec::new();
            for payout in payouts {
                payments.extend(collect_payout(client, &payout).await?);
            }
            Ok(payments)
        },
        |error| upstream(error),
    )
    .await
}

async fn collect_payout(client: &StripeClient, payout: &Payout) -> Result<Vec<Payment>, AppError> {
    services::pagination::collect_cursor_pages(
        |cursor| async move {
            let page = client
                .list_balance_transactions(&payout.id, 100, cursor.as_deref())
                .await
                .map_err(|error| upstream(error))?;
            Ok(CursorPage {
                nodes: page.data,
                has_more: page.has_more,
            })
        },
        |balance: &BalanceTransaction| balance.id.clone(),
        |balances| async move {
            let mut payments = Vec::new();
            for balance in balances {
                if !matches!(
                    balance.r#type,
                    BalanceTransactionKind::Charge
                        | BalanceTransactionKind::Payment
                        | BalanceTransactionKind::Refund
                        | BalanceTransactionKind::PaymentRefund
                ) {
                    continue;
                }
                let refund = matches!(
                    balance.r#type,
                    BalanceTransactionKind::Refund | BalanceTransactionKind::PaymentRefund
                );
                let source = balance
                    .source
                    .as_deref()
                    .ok_or_else(|| protocol("Stripe accounting source missing"))?;
                let (net, fee, vat, gross) = amounts(&balance)?;
                let (reference, created, description, currency, source_money) =
                    reference(client, &balance, source, refund).await?;
                let rate = exchange_rate(&balance, &currency)?;
                let mut transaction = transformers::accounting::transaction(
                    AccountingGateway::Stripe,
                    if refund {
                        AccountingKind::Refund
                    } else {
                        AccountingKind::Sale
                    },
                    Some(0.0),
                );
                transaction.po_number.clone_from(&payout.id);
                transaction.po_date = Some(iso_utc(payout.arrival_date));
                transaction.transaction_date = iso_utc(created);
                transaction.payment_reference = reference;
                transaction.description = description;
                transaction.currency = "THB".into();
                transaction.exchange_rate = Some(rate);
                transaction.net_paid = net;
                transaction.fee = fee;
                transaction.vat = vat;
                transaction.gross_paid = gross;
                payments.push(Payment {
                    transaction,
                    currency,
                    source_money,
                });
            }
            Ok(payments)
        },
        |error| upstream(error),
    )
    .await
}

async fn reference(
    client: &StripeClient,
    balance: &BalanceTransaction,
    source: &str,
    refund: bool,
) -> Result<(String, i64, String, String, Option<ExactMoney>), AppError> {
    if refund {
        let refund = client
            .get_refund(source)
            .await
            .map_err(|error| upstream(error))?;
        let payment_reference = balance
            .description
            .as_deref()
            .and_then(payment_session_id)
            .unwrap_or(refund.id);
        return Ok((
            payment_reference,
            refund.created,
            balance.description.clone().unwrap_or_default(),
            refund.currency,
            None,
        ));
    }
    let charge = client
        .get_charge(source)
        .await
        .map_err(|error| upstream(error))?;
    let source_money = source_money(&charge)?;
    let reference = balance
        .description
        .as_deref()
        .and_then(payment_session_id)
        .unwrap_or_else(|| charge.id.clone());
    let description = if balance
        .description
        .as_deref()
        .is_some_and(|value| value.contains("REFUND FOR CHARGE"))
    {
        balance.description.clone().unwrap_or_default()
    } else {
        charge
            .metadata
            .customer_email
            .or(charge.metadata.email)
            .or(balance.description.clone())
            .unwrap_or_default()
    };
    Ok((
        reference,
        charge.created,
        description,
        charge.currency,
        source_money,
    ))
}

fn source_money(charge: &Charge) -> Result<Option<ExactMoney>, AppError> {
    if charge.status != Some(ChargeStatus::Succeeded)
        || !charge.payment_status.paid
        || !charge.payment_status.captured
        || charge.payment_status.refunded
        || charge.disputed
        || charge.amount_refunded != 0
        || charge.amount_captured <= 0
        || charge.amount != Some(charge.amount_captured)
    {
        return Ok(None);
    }
    let currency = charge.currency.to_ascii_uppercase();
    let Some(precision) = currency_precision(&currency) else {
        return Ok(None);
    };
    ExactMoney::new(charge.amount_captured, &currency, precision).map(Some)
}

fn exchange_rate(balance: &BalanceTransaction, currency: &str) -> Result<f64, AppError> {
    let rate = balance
        .exchange_rate
        .or_else(|| currency.eq_ignore_ascii_case("THB").then_some(1.0))
        .ok_or_else(|| protocol("Stripe exchange rate missing for converted payment"))?;
    if !rate.is_finite() || rate <= 0.0 {
        return Err(protocol("invalid Stripe exchange rate"));
    }
    Ok(rate)
}

fn amounts(balance: &BalanceTransaction) -> Result<(f64, f64, f64, f64), AppError> {
    if !balance.currency.eq_ignore_ascii_case("THB")
        || balance
            .fee_details
            .iter()
            .any(|fee| !fee.currency.eq_ignore_ascii_case("THB"))
    {
        return Err(protocol("Stripe settlement currency is not THB"));
    }
    let fees = |kind: FeeKind| {
        balance
            .fee_details
            .iter()
            .filter(|fee| fee.r#type == kind)
            .map(|fee| cents(fee.amount))
            .sum()
    };
    Ok((
        cents(balance.net),
        fees(FeeKind::StripeFee),
        fees(FeeKind::Tax),
        cents(balance.amount),
    ))
}

fn payment_session_id(description: &str) -> Option<String> {
    let marker = "gid://shopify/PaymentSession/";
    let start = description.find(marker)? + marker.len();
    let id = description[start..]
        .split(|character: char| character == ')' || character.is_whitespace())
        .next()?;
    (!id.is_empty()).then(|| id.to_owned())
}
#[cfg(test)]
#[path = "../../tests/services/accounting/stripe.rs"]
mod tests;
