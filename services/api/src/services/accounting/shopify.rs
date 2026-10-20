use super::{Payment, date, money, protocol, quantity};
use crate::{
    client::shopify::ShopifyClient,
    error::AppError,
    models::shopify::{AccountingLineItem, Money, Order, OrderTransaction, PageRequest},
    services::{common::upstream, pagination::for_each_page},
    validations::accounting::AccountingDateRange,
};
use domain::dto::{AccountingGateway, AccountingItem, AccountingKind, AccountingReportResponse};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(super) async fn enrich(
    client: &ShopifyClient,
    payments: Vec<Payment>,
    range: &AccountingDateRange,
) -> Result<AccountingReportResponse, AppError> {
    let mut response = AccountingReportResponse {
        transactions: Vec::new(),
        items: Vec::new(),
    };
    if payments.is_empty() {
        return Ok(response);
    }
    let mut start = range.start;
    let mut end = range.end;
    for payment in &payments {
        let instant = date(&payment.transaction.transaction_date)?;
        start = start.min(instant);
        end = end.max(instant);
    }
    let search = format!(
        "(created_at:>='{}' AND created_at:<='{}') OR (updated_at:>='{}' AND updated_at:<='{}')",
        start.to_rfc3339(),
        end.to_rfc3339(),
        start.to_rfc3339(),
        end.to_rfc3339()
    );
    let mut orders = Vec::new();
    let mut seen = HashSet::new();
    for_each_page(
        None,
        |after| {
            let page = PageRequest { first: 10, after };
            let search = &search;
            async move {
                client
                    .accounting_orders(search, &page)
                    .await
                    .map_err(|error| upstream(error))
            }
        },
        |nodes| {
            for order in nodes {
                complete(&order)?;
                if seen.insert(order.id.clone()) {
                    orders.push(order);
                }
            }
            Ok(())
        },
        |error| upstream(error),
    )
    .await?;
    let mut index = HashMap::new();
    for (position, order) in orders.iter().enumerate() {
        for tx in order.transactions.iter().chain(
            order
                .refunds
                .iter()
                .flat_map(|refund| &refund.transactions.nodes),
        ) {
            if !tx.status.eq_ignore_ascii_case("success") {
                continue;
            }
            if let Some((gateway, reference)) = reference(tx)? {
                let key = (gateway, tx.kind.eq_ignore_ascii_case("refund"), reference);
                if let Some(previous) = index.insert(key, position)
                    && previous != position
                {
                    return Err(protocol("ambiguous Shopify accounting reference"));
                }
            }
        }
    }
    for mut payment in payments {
        let tx = &payment.transaction;
        let key = (
            gateway_key(tx.gateway),
            tx.kind == AccountingKind::Refund,
            tx.payment_reference.clone(),
        );
        if let Some(position) = index.get(&key) {
            apply(&mut payment, &orders[*position], &mut response.items)?;
        }
        response.transactions.push(payment.transaction);
    }
    Ok(response)
}

fn complete(order: &Order) -> Result<(), AppError> {
    if order.line_items.page_info.has_next_page
        || order.shipping_lines.page_info.has_next_page
        || order.refunds.iter().any(|refund| {
            refund.transactions.page_info.has_next_page
                || refund.refund_shipping_lines.page_info.has_next_page
                || refund.refund_line_items.page_info.has_next_page
        })
    {
        return Err(protocol(
            "Shopify accounting order exceeds nested connection limit",
        ));
    }
    Ok(())
}

fn reference(tx: &OrderTransaction) -> Result<Option<(u8, String)>, AppError> {
    let gateway = tx
        .gateway
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        gateway.as_str(),
        "stripe card payment" | "stripe card payments"
    ) {
        return Ok(tx
            .payment_id
            .as_ref()
            .filter(|id| !id.is_empty())
            .map(|id| (0, id.clone())));
    }
    let key = match gateway.as_str() {
        "stripe" => 0,
        "paypal" => 1,
        _ => return Ok(None),
    };
    let Some(raw) = tx.receipt_json.as_deref() else {
        return Ok(None);
    };
    let receipt: Value =
        serde_json::from_str(raw).map_err(|_| protocol("invalid Shopify payment receipt"))?;
    let refund = tx.kind.eq_ignore_ascii_case("refund");
    let value = match (gateway.as_str(), refund) {
        ("stripe", false) => receipt.pointer("/charges/data/0/id"),
        ("stripe", true) => receipt.get("id"),
        ("paypal", false) => receipt.get("transaction_id"),
        ("paypal", true) => receipt.get("refund_transaction_id"),
        _ => None,
    };
    Ok(value
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(|id| (key, id.to_owned())))
}

fn gateway_key(gateway: AccountingGateway) -> u8 {
    match gateway {
        AccountingGateway::Stripe => 0,
        AccountingGateway::Paypal => 1,
    }
}

fn matches(tx: &OrderTransaction, payment: &Payment) -> Result<bool, AppError> {
    Ok(tx.status.eq_ignore_ascii_case("success")
        && reference(tx)?.is_some_and(|(gateway, reference)| {
            gateway == gateway_key(payment.transaction.gateway)
                && reference == payment.transaction.payment_reference
        }))
}

fn apply(
    payment: &mut Payment,
    order: &Order,
    items: &mut Vec<AccountingItem>,
) -> Result<(), AppError> {
    let id = order.id.number("Order").map_err(|error| upstream(error))?;
    let mut shipping = 0.0;
    let mut adjusted = false;
    let mut include_items = false;
    if payment.transaction.kind == AccountingKind::Refund {
        for refund in &order.refunds {
            let matched = refund
                .transactions
                .nodes
                .iter()
                .map(|tx| matches(tx, payment))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .any(|found| found);
            if !matched {
                continue;
            }
            for line in &refund.refund_shipping_lines.nodes {
                shipping -= thb(&line.subtotal_amount_set.shop_money, payment)?.abs();
            }
            for line in &refund.refund_line_items.nodes {
                items.push(item(
                    &line.line_item,
                    -line.quantity,
                    -money(&line.subtotal_set.shop_money.amount)?,
                    AccountingKind::Refund,
                    &payment.transaction.transaction_date,
                    id,
                    &payment.transaction.payment_reference,
                ));
            }
        }
    } else {
        let (has_shipping, is_adjusted, has_items) = sale_details(order, payment)?;
        adjusted = is_adjusted;
        include_items = has_items;
        if has_shipping {
            for line in &order.shipping_lines.nodes {
                shipping += thb(&line.price_set.shop_money, payment)?;
            }
        }
    }
    if include_items {
        for line in &order.line_items.nodes {
            items.push(item(
                line,
                line.quantity,
                money(&line.original_unit_price_set.shop_money.amount)?,
                AccountingKind::Sale,
                &payment.transaction.transaction_date,
                id,
                &payment.transaction.payment_reference,
            ));
        }
    }
    let tx = &mut payment.transaction;
    tx.shopify_order_id = id;
    tx.shopify_order_number = order.number;
    tx.customer_name = order
        .shipping_address
        .as_ref()
        .and_then(|address| address.name.clone())
        .unwrap_or_default();
    tx.customer_address = address(order);
    tx.customer_phone = phone(order);
    tx.shipping_fee = shipping;
    tx.order_adjusted = adjusted;
    Ok(())
}

fn sale_details(order: &Order, payment: &Payment) -> Result<(bool, bool, bool), AppError> {
    let shipping = order
        .shipping_lines
        .nodes
        .iter()
        .map(|line| money(&line.price_set.shop_money.amount))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum::<f64>();
    let mut total_items = 0.0;
    for line in &order.line_items.nodes {
        total_items +=
            money(&line.original_unit_price_set.shop_money.amount)? * quantity(line.quantity);
    }
    let captured = order
        .transactions
        .iter()
        .filter(|tx| {
            tx.status.eq_ignore_ascii_case("success")
                && (tx.kind.eq_ignore_ascii_case("capture") || tx.kind.eq_ignore_ascii_case("sale"))
        })
        .collect::<Vec<_>>();
    let mut position = None;
    let mut total = 0.0;
    for (index, tx) in captured.iter().enumerate() {
        total += money(&tx.amount_set.shop_money.amount)?;
        if matches(tx, payment)? {
            position = Some(index);
        }
    }
    let Some(index) = position else {
        return Ok((false, false, false));
    };
    let amount = money(&captured[index].amount_set.shop_money.amount)?;
    if (amount - total_items - shipping).abs() < 0.005 {
        return Ok((true, false, true));
    }
    if index == 0 && total > 0.0 && total <= total_items + shipping {
        Ok((true, true, true))
    } else {
        Ok((false, false, false))
    }
}

fn thb(amount: &Money, payment: &Payment) -> Result<f64, AppError> {
    let value = money(&amount.amount)?;
    if amount.currency_code.eq_ignore_ascii_case("THB") {
        return Ok(value);
    }
    if !amount.currency_code.eq_ignore_ascii_case(&payment.currency)
        || payment.currency.eq_ignore_ascii_case("THB")
        || !payment.transaction.exchange_rate.is_finite()
        || payment.transaction.exchange_rate <= 0.0
    {
        return Err(protocol("incompatible Shopify shipping currency"));
    }
    let result = value * payment.transaction.exchange_rate;
    if !result.is_finite() {
        return Err(protocol("invalid shipping amount"));
    }
    Ok(result)
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
#[cfg(test)]
#[path = "../../tests/services/accounting/shopify.rs"]
mod tests;
