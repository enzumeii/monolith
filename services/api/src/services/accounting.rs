use super::common::{shopify_client, upstream};
use crate::{
    client::{paypal::PaypalClient, shopify::ShopifyClient, stripe::StripeClient},
    error::AppError,
    models::{
        paypal::TransactionDetail,
        shopify::{AccountingLineItem, Order, OrdersByPaymentRequest, PageRequest, ShopifyId},
        stripe::{BalanceTransaction, Payout},
    },
    state::AppState,
    validations::accounting::AccountingDateRange,
};
use axum::http::StatusCode;
use chrono::{DateTime, Datelike, Months, TimeZone, Utc};
use domain::dto::{
    AccountingGateway, AccountingItem, AccountingKind, AccountingReportResponse,
    AccountingTransaction,
};

pub async fn report(
    state: &AppState,
    range: AccountingDateRange,
) -> Result<AccountingReportResponse, AppError> {
    let stripe = configured_stripe(state)?;
    let paypal = configured_paypal(state)?;
    if stripe.is_none() && paypal.is_none() {
        return Err(AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Accounting report provider unavailable",
        ));
    }
    let shopify = shopify_client(state)?;
    let mut result = AccountingReportResponse {
        transactions: Vec::new(),
        items: Vec::new(),
    };
    if let Some(client) = stripe {
        append_stripe(&mut result, client, shopify, &range).await?;
    }
    if let Some(client) = paypal {
        append_paypal(&mut result, client, shopify, &range).await?;
    }
    Ok(result)
}

fn configured_stripe(state: &AppState) -> Result<Option<&StripeClient>, AppError> {
    state
        .stripe
        .as_ref()
        .map(|value| value.as_deref())
        .map_err(|error| {
            AppError::new(StatusCode::SERVICE_UNAVAILABLE, "Stripe client unavailable")
                .with_source(error.clone())
        })
}

fn configured_paypal(state: &AppState) -> Result<Option<&PaypalClient>, AppError> {
    state
        .paypal
        .as_ref()
        .map(|value| value.as_deref())
        .map_err(|error| {
            AppError::new(StatusCode::SERVICE_UNAVAILABLE, "PayPal client unavailable")
                .with_source(error.clone())
        })
}

async fn append_stripe(
    response: &mut AccountingReportResponse,
    client: &StripeClient,
    shopify: &ShopifyClient,
    range: &AccountingDateRange,
) -> Result<(), AppError> {
    let mut payout_cursor = None;
    loop {
        let payouts = client
            .list_payouts(
                range.start.timestamp(),
                range.end.timestamp(),
                Some("paid"),
                100,
                payout_cursor.as_deref(),
            )
            .await
            .map_err(upstream)?;
        let has_more = payouts.has_more;
        let last = payouts.data.last().map(|payout| payout.id.clone());
        for payout in payouts.data {
            append_payout(response, client, shopify, &payout).await?;
        }
        if !has_more {
            break;
        }
        if last.is_none() || last == payout_cursor {
            return Err(upstream(crate::client::ClientError::new(
                crate::client::ClientErrorKind::Protocol(
                    "Stripe pagination cursor did not progress",
                ),
            )));
        }
        payout_cursor = last;
    }
    Ok(())
}

async fn append_payout(
    response: &mut AccountingReportResponse,
    client: &StripeClient,
    shopify: &ShopifyClient,
    payout: &Payout,
) -> Result<(), AppError> {
    let mut cursor = None;
    loop {
        let page = client
            .list_balance_transactions(&payout.id, 100, cursor.as_deref())
            .await
            .map_err(upstream)?;
        let has_more = page.has_more;
        let last = page.data.last().map(|transaction| transaction.id.clone());
        for balance in page.data {
            if !matches!(
                balance.r#type.as_str(),
                "charge" | "payment" | "refund" | "payment_refund"
            ) {
                continue;
            }
            append_stripe_balance(response, client, shopify, payout, balance).await?;
        }
        if !has_more {
            break;
        }
        if last.is_none() || last == cursor {
            return Err(upstream(crate::client::ClientError::new(
                crate::client::ClientErrorKind::Protocol(
                    "Stripe pagination cursor did not progress",
                ),
            )));
        }
        cursor = last;
    }
    Ok(())
}

async fn append_stripe_balance(
    response: &mut AccountingReportResponse,
    client: &StripeClient,
    shopify: &ShopifyClient,
    payout: &Payout,
    balance: BalanceTransaction,
) -> Result<(), AppError> {
    let source = balance.source.as_deref().unwrap_or_default();
    let is_refund = balance.r#type.contains("refund");
    let (reference, transaction_epoch, description) =
        stripe_reference(client, &balance, source, is_refund).await?;
    let exchange_rate = balance.exchange_rate.unwrap_or(1.0);
    let found = find_shopify_order(shopify, &reference).await?;
    let order = accounting_order(found.as_ref(), &reference, is_refund, exchange_rate)?;
    let transaction_date = iso_utc(transaction_epoch);
    let po_date = iso_utc(payout.arrival_date);
    let (net, fee, vat, gross) = stripe_amounts(&balance, exchange_rate);
    response.transactions.push(AccountingTransaction {
        gateway: AccountingGateway::Stripe,
        kind: if is_refund {
            AccountingKind::Refund
        } else {
            AccountingKind::Sale
        },
        po_number: payout.id.clone(),
        po_date,
        transaction_date: transaction_date.clone(),
        payment_reference: reference.clone(),
        description,
        shopify_order_id: order.id,
        shopify_order_number: order.number,
        customer_name: order.name,
        customer_address: order.address,
        customer_phone: order.phone,
        exchange_rate,
        net_paid: net,
        fee,
        vat,
        gross_paid: gross,
        shipping_fee: order.shipping_fee,
        order_adjusted: order.adjusted,
    });
    if let Some((id, shopify_order)) = found
        && (is_refund || order.include_items)
    {
        append_items(
            response,
            &id,
            &shopify_order,
            &reference,
            &transaction_date,
            is_refund,
        );
    }
    Ok(())
}

async fn stripe_reference(
    client: &StripeClient,
    balance: &BalanceTransaction,
    source: &str,
    is_refund: bool,
) -> Result<(String, i64, String), AppError> {
    if is_refund {
        let refund = client.get_refund(source).await.map_err(upstream)?;
        return Ok((
            refund.id,
            refund.created,
            balance.description.clone().unwrap_or_default(),
        ));
    }
    let charge = client.get_charge(source).await.map_err(upstream)?;
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
            .clone()
            .or(charge.metadata.email.clone())
            .or(balance.description.clone())
            .unwrap_or_default()
    };
    Ok((reference, charge.created, description))
}

fn payment_session_id(description: &str) -> Option<String> {
    let marker = "gid://shopify/PaymentSession/";
    let start = description.find(marker)? + marker.len();
    let id = description[start..]
        .split(|character: char| !character.is_ascii_digit())
        .next()?;
    (!id.is_empty()).then(|| id.to_owned())
}

fn stripe_amounts(balance: &BalanceTransaction, exchange_rate: f64) -> (f64, f64, f64, f64) {
    let fees = |kind: &str| {
        balance
            .fee_details
            .iter()
            .filter(|detail| detail.r#type == kind)
            .map(|detail| cents(detail.amount))
            .sum::<f64>()
            * exchange_rate
    };
    (
        cents(balance.net) * exchange_rate,
        fees("stripe_fee"),
        fees("tax"),
        cents(balance.amount) * exchange_rate,
    )
}

async fn append_paypal(
    response: &mut AccountingReportResponse,
    client: &PaypalClient,
    shopify: &ShopifyClient,
    range: &AccountingDateRange,
) -> Result<(), AppError> {
    let start = range
        .start
        .checked_sub_months(Months::new(1))
        .unwrap_or(range.start)
        .with_timezone(&Utc);
    let end = range.end.with_timezone(&Utc);
    let mut page = 1;
    let mut details = Vec::new();
    loop {
        let result = client
            .get_transactions(start, end, 500, page)
            .await
            .map_err(upstream)?;
        let last_page = page >= result.total_pages;
        details.extend(result.transaction_details);
        if last_page {
            break;
        }
        page += 1;
    }
    for (detail, rate, po_date) in paypal_records(details, range.end.month()) {
        append_paypal_detail(response, shopify, detail, rate, po_date).await?;
    }
    Ok(())
}

fn paypal_records(
    mut details: Vec<TransactionDetail>,
    report_month: u32,
) -> Vec<(TransactionDetail, f64, DateTime<chrono::FixedOffset>)> {
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
        let mut record_po_date = None;
        match event {
            "T0400" | "T0403" => po_date = Some(info.transaction_initiation_date),
            "T0201" => {
                let value = info
                    .transaction_amount
                    .value
                    .parse::<f64>()
                    .unwrap_or_default()
                    .abs();
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
                    record_po_date = Some(info.transaction_initiation_date);
                } else if let Some(withdrawal_date) = po_date
                    && withdraw_thb != 0.0
                    && withdraw_usd != 0.0
                {
                    rate = (withdraw_thb / withdraw_usd * 100.0).round() / 100.0;
                    record_po_date = Some(withdrawal_date);
                }
            }
            "T0200" => {}
            _ => record_po_date = Some(info.transaction_initiation_date),
        }
        if let Some(date) = record_po_date.filter(|date| date.month() == report_month)
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
    records
}

async fn append_paypal_detail(
    response: &mut AccountingReportResponse,
    shopify: &ShopifyClient,
    detail: TransactionDetail,
    exchange_rate: f64,
    po_date: DateTime<chrono::FixedOffset>,
) -> Result<(), AppError> {
    let event = detail.transaction.transaction_event_code.as_str();
    let is_refund = event == "T1107";
    let reference = detail.transaction.transaction_id.clone();
    let amount = detail
        .transaction
        .transaction_amount
        .value
        .parse::<f64>()
        .unwrap_or(0.0);
    let fee = detail
        .transaction
        .fee_amount
        .as_ref()
        .and_then(|m| m.value.parse::<f64>().ok())
        .unwrap_or(0.0)
        .abs();
    let gross = amount * exchange_rate;
    let fee = fee * exchange_rate;
    let net = gross - fee;
    let date = detail.transaction.transaction_initiation_date;
    let found = find_shopify_order(shopify, &reference).await?;
    let order = accounting_order(found.as_ref(), &reference, is_refund, exchange_rate)?;
    let transaction_date = date.to_rfc3339();
    response.transactions.push(AccountingTransaction {
        gateway: AccountingGateway::Paypal,
        kind: if is_refund {
            AccountingKind::Refund
        } else {
            AccountingKind::Sale
        },
        po_number: String::new(),
        po_date: po_date.to_rfc3339(),
        transaction_date: transaction_date.clone(),
        payment_reference: reference.clone(),
        description: paypal_description(
            event,
            detail
                .payer
                .as_ref()
                .and_then(|p| p.email_address.as_deref()),
            &detail.transaction.transaction_amount.currency_code,
        ),
        shopify_order_id: order.id,
        shopify_order_number: order.number,
        customer_name: order.name,
        customer_address: order.address,
        customer_phone: order.phone,
        exchange_rate,
        net_paid: net,
        fee,
        vat: 0.0,
        gross_paid: gross,
        shipping_fee: order.shipping_fee,
        order_adjusted: order.adjusted,
    });
    if let Some((id, shopify_order)) = found
        && (is_refund || order.include_items)
    {
        append_items(
            response,
            &id,
            &shopify_order,
            &reference,
            &transaction_date,
            is_refund,
        );
    }
    Ok(())
}

async fn find_shopify_order(
    client: &ShopifyClient,
    payment_reference: &str,
) -> Result<Option<(ShopifyId, Order)>, AppError> {
    if payment_reference.is_empty() {
        return Ok(None);
    }
    let orders = client
        .orders_by_payment(
            &OrdersByPaymentRequest {
                payment_id: payment_reference.to_owned(),
            },
            &PageRequest {
                first: 10,
                after: None,
            },
        )
        .await
        .map_err(upstream)?;
    orders.validate_after(None).map_err(upstream)?;
    for order in &orders.nodes {
        for complete in [
            order.line_items.page_info.has_next_page,
            order.shipping_lines.page_info.has_next_page,
        ] {
            if complete {
                return Err(upstream(crate::client::ClientError::new(
                    crate::client::ClientErrorKind::Protocol(
                        "Shopify accounting order exceeds nested connection limit",
                    ),
                )));
            }
        }
        for refund in &order.refunds {
            if refund.order_adjustments.page_info.has_next_page
                || refund.refund_line_items.page_info.has_next_page
            {
                return Err(upstream(crate::client::ClientError::new(
                    crate::client::ClientErrorKind::Protocol(
                        "Shopify accounting refund exceeds nested connection limit",
                    ),
                )));
            }
        }
    }
    Ok(orders
        .nodes
        .into_iter()
        .next()
        .map(|order| (order.id.clone(), order)))
}

fn append_items(
    response: &mut AccountingReportResponse,
    id: &ShopifyId,
    order: &Order,
    reference: &str,
    date: &str,
    refund: bool,
) {
    let order_id = id.number("Order").unwrap_or_default();
    if refund {
        for refund in &order.refunds {
            if !refund
                .transactions
                .iter()
                .any(|transaction| transaction.payment_id.as_deref() == Some(reference))
            {
                continue;
            }
            for line in &refund.refund_line_items.nodes {
                response.items.push(item(
                    &line.line_item,
                    -line.quantity,
                    -parse_money(&line.subtotal_set.shop_money.amount),
                    AccountingKind::Refund,
                    date,
                    order_id,
                    reference,
                ));
            }
        }
    } else {
        for line in &order.line_items.nodes {
            response.items.push(item(
                line,
                line.quantity,
                parse_money(&line.original_unit_price_set.shop_money.amount),
                AccountingKind::Sale,
                date,
                order_id,
                reference,
            ));
        }
    }
}

fn sale_details(order: &Order, reference: &str) -> (f64, bool, bool) {
    let shipping = shipping_total(order);
    let item_total = order
        .line_items
        .nodes
        .iter()
        .map(|line| {
            parse_money(&line.original_unit_price_set.shop_money.amount) * quantity(line.quantity)
        })
        .sum::<f64>();
    let captured = order
        .transactions
        .iter()
        .filter(|transaction| {
            transaction.status.eq_ignore_ascii_case("success")
                && matches!(
                    transaction.kind.to_ascii_lowercase().as_str(),
                    "capture" | "sale"
                )
        })
        .collect::<Vec<_>>();
    let Some(index) = captured
        .iter()
        .position(|transaction| transaction.payment_id.as_deref() == Some(reference))
    else {
        return (0.0, false, false);
    };
    let payment_amount = parse_money(&captured[index].amount_set.shop_money.amount);
    if (payment_amount - item_total - shipping).abs() < 0.005 {
        return (shipping, false, true);
    }
    let total_paid = captured
        .iter()
        .map(|transaction| parse_money(&transaction.amount_set.shop_money.amount))
        .sum::<f64>();
    if index == 0 && total_paid > 0.0 && total_paid <= item_total + shipping {
        (shipping, true, true)
    } else {
        (0.0, false, false)
    }
}

#[derive(Default)]
struct AccountingOrder {
    id: u64,
    number: i64,
    name: String,
    address: String,
    phone: u64,
    shipping_fee: f64,
    adjusted: bool,
    include_items: bool,
}

fn accounting_order(
    found: Option<&(ShopifyId, Order)>,
    reference: &str,
    is_refund: bool,
    exchange_rate: f64,
) -> Result<AccountingOrder, AppError> {
    let Some((id, order)) = found else {
        return Ok(AccountingOrder::default());
    };
    let (shipping_fee, adjusted, include_items) = if is_refund {
        (refund_shipping(found, reference), false, true)
    } else {
        sale_details(order, reference)
    };
    Ok(AccountingOrder {
        id: id.number("Order").map_err(upstream)?,
        number: order.number,
        name: order
            .shipping_address
            .as_ref()
            .and_then(|address| address.name.clone())
            .unwrap_or_default(),
        address: address(order),
        phone: phone(order),
        shipping_fee: shipping_fee * exchange_rate,
        adjusted,
        include_items,
    })
}

fn cents(value: i64) -> f64 {
    serde_json::Number::from(value).as_f64().unwrap_or_default() / 100.0
}

fn quantity(value: i64) -> f64 {
    serde_json::Number::from(value).as_f64().unwrap_or_default()
}

fn refund_shipping(found: Option<&(ShopifyId, Order)>, reference: &str) -> f64 {
    found
        .into_iter()
        .flat_map(|(_, order)| &order.refunds)
        .filter(|refund| {
            refund
                .transactions
                .iter()
                .any(|transaction| transaction.payment_id.as_deref() == Some(reference))
        })
        .flat_map(|refund| &refund.order_adjustments.nodes)
        .filter(|adjustment| adjustment.kind == "shipping_refund")
        .map(|adjustment| -parse_money(&adjustment.amount_set.shop_money.amount).abs())
        .sum()
}

fn item(
    line: &AccountingLineItem,
    quantity: i64,
    price: f64,
    kind: AccountingKind,
    date: &str,
    order_id: u64,
    reference: &str,
) -> AccountingItem {
    let sku = line
        .sku
        .clone()
        .map(crate::models::shopify::Sku::into_string)
        .unwrap_or_default();
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
        category: sku_category(&sku).to_owned(),
        sku,
        kind,
        transaction_date: date.to_owned(),
        shopify_order_id: order_id,
        payment_reference: reference.to_owned(),
    }
}

fn sku_category(sku: &str) -> &'static str {
    if ["_IP", "_SS", "_XM"]
        .iter()
        .any(|suffix| sku.contains(suffix))
    {
        return "STOCK";
    }
    let code = sku.get(3..6).unwrap_or_default();
    match code {
        "APP" | "BBT" | "TEE" => "APPAREL",
        "TOT" => "BAG",
        "BLK" => "BLANKET",
        "QCS" | "CSE" | "GTK" | "PSR" => "PHONE_ACCESSORY",
        "CLC" => "CLAWCLIP",
        "PIN" => "BADGE",
        "KEY" | "QSK" | "LNK" | "WDK" | "KMB" | "LQD" => "KEYCHAIN",
        "CRB" => "CARABINER",
        "PSH" => "PLUSHIE",
        "STD" => "STANDEE",
        "STS" | "PTC" => "STICKER_SHEET",
        "PCT" => "PHOTOCARD",
        "MAG" => "MAGNET",
        "STK" | "BST" => "STICKER",
        "ZPB" => "ZIPPER_BAG",
        "BDS" => "BEAD",
        "CLP" => "HAIRCLIP",
        "LYD" => "LANYARD",
        "CHD" => "CARDHOLDER",
        _ => "",
    }
}

fn address(order: &Order) -> String {
    let Some(address) = &order.shipping_address else {
        return String::new();
    };
    [
        address.address1.as_deref(),
        address.address2.as_deref(),
        address.city.as_deref(),
        address.province_code.as_deref(),
        address.country.as_deref(),
        address.zip.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(", ")
}

fn phone(order: &Order) -> u64 {
    order
        .shipping_address
        .as_ref()
        .and_then(|a| a.phone.as_deref())
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap_or_default()
}

fn shipping_total(order: &Order) -> f64 {
    order
        .shipping_lines
        .nodes
        .iter()
        .map(|line| parse_money(&line.price_set.shop_money.amount))
        .sum()
}

fn parse_money(value: &str) -> f64 {
    value.parse().unwrap_or_default()
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

fn iso_utc(timestamp: i64) -> String {
    Utc.timestamp_opt(timestamp, 0)
        .single()
        .unwrap_or(chrono::DateTime::<Utc>::UNIX_EPOCH)
        .to_rfc3339()
}

#[cfg(test)]
#[path = "../tests/services/accounting.rs"]
mod tests;
