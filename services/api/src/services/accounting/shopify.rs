use super::{Payment, date, money, protocol, quantity};
use crate::{
    client::shopify::ShopifyClient,
    error::{AppError, upstream},
    models::{
        Sku, SkuCategory,
        shopify::{
            AccountingLineItem, CompletionFinancialStatus, Money, Order, OrderTransaction,
            PageRequest,
        },
    },
    services::{checkout::correlation as checkout_metadata, pagination::for_each_page},
    utils::contact::{format_address, phone_number},
    validations::accounting::AccountingDateRange,
};
use domain::dto::accounting::AccountingTransactions;
use domain::dto::{AccountingGateway, AccountingItem, AccountingKind, AccountingReportResponse};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(super) async fn enrich(
    client: &ShopifyClient,
    payments: Vec<Payment>,
    range: &AccountingDateRange,
) -> Result<AccountingReportResponse, AppError> {
    let mut response = AccountingReportResponse {
        transactions: AccountingTransactions::default(),
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
    let positions = matching_positions(&orders, &payments)?;
    for (mut payment, position) in payments.into_iter().zip(positions) {
        if let Some(position) = position {
            apply(&mut payment, &orders[position], &mut response.items)?;
        }
        match payment.transaction.gateway {
            AccountingGateway::Stripe => response.transactions.stripe.push(payment.transaction),
            AccountingGateway::Paypal => response.transactions.paypal.push(payment.transaction),
        }
    }
    Ok(response)
}

struct OrderIndex {
    native: HashMap<(u8, bool, String), usize>,
    external: HashMap<String, Vec<usize>>,
    metadata: HashMap<(u8, String), Vec<usize>>,
}

fn native_index(orders: &[Order]) -> Result<OrderIndex, AppError> {
    let mut index = OrderIndex {
        native: HashMap::new(),
        external: HashMap::new(),
        metadata: HashMap::new(),
    };
    let mut identities = HashMap::new();
    for (position, order) in orders.iter().enumerate() {
        let metadata = metadata(order)?;
        let identity = metadata
            .as_ref()
            .map(|proof| proof.identity.checkout_id)
            .or_else(|| legacy_checkout_id(&order.tags));
        if let Some(identity) = identity
            && let Some((previous, previous_metadata)) =
                identities.insert(identity, (position, metadata.is_some()))
            && previous != position
            && (previous_metadata || metadata.is_some())
        {
            return Err(protocol("ambiguous Shopify checkout identity"));
        }
        if let Some(proof) = metadata
            && let Some(reference) = proof.reference
        {
            index
                .metadata
                .entry((proof.gateway, reference.to_owned()))
                .or_default()
                .push(position);
        }
        if correlation(&order.tags, "co:") && correlation(&order.tags, "cv:") {
            for tag in &order.tags {
                if tag.starts_with("payment:") {
                    index
                        .external
                        .entry(tag.clone())
                        .or_default()
                        .push(position);
                }
            }
        }
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
                if let Some(previous) = index.native.insert(key, position)
                    && previous != position
                {
                    return Err(protocol("ambiguous Shopify accounting reference"));
                }
            }
        }
    }
    Ok(index)
}

fn matching_position(
    orders: &[Order],
    index: &OrderIndex,
    payment: &Payment,
) -> Result<Option<usize>, AppError> {
    let tx = &payment.transaction;
    let key = (
        gateway_key(tx.gateway),
        tx.kind == AccountingKind::Refund,
        tx.payment_reference.clone(),
    );
    let mut found = index.native.get(&key).copied();
    if tx.kind == AccountingKind::Sale && payment.source_money.is_some() {
        let Ok(tag) = external_reference(payment) else {
            return Ok(found);
        };
        let metadata_key = (gateway_key(tx.gateway), tx.payment_reference.clone());
        for &position in index
            .external
            .get(&tag)
            .into_iter()
            .flatten()
            .chain(index.metadata.get(&metadata_key).into_iter().flatten())
        {
            if external_sale_transaction(&orders[position], payment)?.is_some() {
                if found.is_some_and(|previous| previous != position) {
                    return Err(protocol("ambiguous Shopify accounting reference"));
                }
                found = Some(position);
            }
        }
    }
    Ok(found)
}

fn matching_positions(
    orders: &[Order],
    payments: &[Payment],
) -> Result<Vec<Option<usize>>, AppError> {
    let index = native_index(orders)?;
    let mut claims: HashMap<(usize, usize), HashSet<(u8, &str)>> = HashMap::new();
    let mut external_claims = HashSet::new();
    let mut positions = Vec::with_capacity(payments.len());
    for payment in payments {
        let position = matching_position(orders, &index, payment)?;
        if let Some(position) = position
            && payment.transaction.kind == AccountingKind::Sale
        {
            let order = &orders[position];
            let external = external_sale_transaction(order, payment)?;
            for (transaction, tx) in order.transactions.iter().enumerate() {
                if external != Some(transaction) && !matches(tx, payment)? {
                    continue;
                }
                let key = (position, transaction);
                let references = claims.entry(key).or_default();
                references.insert((
                    gateway_key(payment.transaction.gateway),
                    &payment.transaction.payment_reference,
                ));
                if external == Some(transaction) {
                    external_claims.insert(key);
                }
                if references.len() > 1 && external_claims.contains(&key) {
                    return Err(protocol("ambiguous Shopify accounting reference"));
                }
            }
        }
        positions.push(position);
    }
    Ok(positions)
}

fn external_reference(payment: &Payment) -> Result<String, AppError> {
    let kind = match payment.transaction.gateway {
        AccountingGateway::Stripe => "stripe:charge",
        AccountingGateway::Paypal => "paypal:capture",
    };
    crate::services::checkout_completion::reference(kind, &payment.transaction.payment_reference)
}

fn external_sale_transaction(order: &Order, payment: &Payment) -> Result<Option<usize>, AppError> {
    if payment.transaction.kind != AccountingKind::Sale {
        return Ok(None);
    }
    let Some(source) = payment.source_money.as_ref() else {
        return Ok(None);
    };
    if !matches!(
        order.display_financial_status,
        Some(CompletionFinancialStatus::Paid)
    ) || !matches!(order.cancelled_at, Some(None))
        || !order.refunds.is_empty()
    {
        return Ok(None);
    }
    let metadata = metadata(order)?;
    let gateway = if let Some(proof) = metadata.as_ref() {
        if proof.gateway != gateway_key(payment.transaction.gateway)
            || proof.reference != Some(payment.transaction.payment_reference.as_str())
            || order.transactions.len() >= 100
            || order
                .transactions
                .iter()
                .any(|tx| !tx.kind.eq_ignore_ascii_case("sale"))
        {
            return Ok(None);
        }
        "manual"
    } else {
        if !correlation(&order.tags, "co:") || !correlation(&order.tags, "cv:") {
            return Ok(None);
        }
        let Ok(tag) = external_reference(payment) else {
            return Ok(None);
        };
        if order.tags.iter().filter(|value| **value == tag).count() != 1 {
            return Ok(None);
        }
        match payment.transaction.gateway {
            AccountingGateway::Stripe => "Stripe Card Payments",
            AccountingGateway::Paypal => "paypal",
        }
    };
    let Some(total) = order.current_total_price_set.as_ref() else {
        return Ok(None);
    };
    let Some(outstanding) = order.total_outstanding_set.as_ref() else {
        return Ok(None);
    };
    let Some(total) = total.presentment_money.as_ref() else {
        return Ok(None);
    };
    let Some(outstanding) = outstanding.presentment_money.as_ref() else {
        return Ok(None);
    };
    if !exact_amount(total, source, source.minor) || !exact_amount(outstanding, source, 0) {
        return Ok(None);
    }
    let mut successful = order
        .transactions
        .iter()
        .enumerate()
        .filter(|(_, tx)| tx.status.eq_ignore_ascii_case("success"));
    let Some((position, tx)) = successful.next() else {
        return Ok(None);
    };
    if successful.next().is_some()
        || !tx.kind.eq_ignore_ascii_case("sale")
        || !tx
            .gateway
            .as_deref()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(gateway))
        || !tx
            .amount_set
            .presentment_money
            .as_ref()
            .is_some_and(|amount| exact_amount(amount, source, source.minor))
        || payment.transaction.gateway == AccountingGateway::Paypal
            && reference(tx)?.is_some_and(|(gateway, reference)| {
                gateway != gateway_key(payment.transaction.gateway)
                    || reference != payment.transaction.payment_reference
            })
    {
        return Ok(None);
    }
    Ok(Some(position))
}

struct Metadata<'a> {
    identity: checkout_metadata::Identity,
    gateway: u8,
    reference: Option<&'a str>,
}

fn metadata(order: &Order) -> Result<Option<Metadata<'_>>, AppError> {
    let Some(identity) =
        checkout_metadata::parse_identity(&order.custom_attributes).map_err(|error| {
            protocol("invalid Shopify checkout accounting metadata").with_source(error)
        })?
    else {
        return Ok(None);
    };
    let (gateway, keys): (u8, &[(&str, &str)]) = if identity.provider == "stripe" {
        (
            0,
            &[
                (checkout_metadata::STRIPE_SESSION_KEY, "stripe:session"),
                (checkout_metadata::STRIPE_INTENT_KEY, "stripe:intent"),
                (checkout_metadata::STRIPE_CHARGE_KEY, "stripe:charge"),
            ],
        )
    } else {
        (
            1,
            &[
                (checkout_metadata::PAYPAL_ORDER_KEY, "paypal:order"),
                (checkout_metadata::PAYPAL_CAPTURE_KEY, "paypal:capture"),
            ],
        )
    };
    let mut references = Vec::new();
    let mut tags = HashSet::new();
    for (key, kind) in keys {
        if let Some(value) = checkout_metadata::unique_value(&order.custom_attributes, key)
            .map_err(|error| {
                protocol("invalid Shopify checkout accounting metadata").with_source(error)
            })?
        {
            tags.insert(
                crate::services::checkout_completion::reference(kind, value).map_err(|error| {
                    protocol("invalid Shopify checkout accounting reference").with_source(error)
                })?,
            );
            references.push(value);
        }
    }
    checkout_metadata::validate_legacy_identity(&identity, &order.tags).map_err(|error| {
        protocol("conflicting Shopify checkout accounting identity").with_source(error)
    })?;
    let mut seen = HashSet::new();
    for tag in order.tags.iter().filter(|tag| tag.starts_with("payment:")) {
        if !tags.contains(tag) || !seen.insert(tag) {
            return Err(protocol(
                "conflicting Shopify checkout accounting reference",
            ));
        }
    }
    let reference = if references.len() == keys.len() {
        references.last().copied()
    } else {
        None
    };
    Ok(Some(Metadata {
        identity,
        gateway,
        reference,
    }))
}

fn legacy_checkout_id(tags: &[String]) -> Option<uuid::Uuid> {
    if !correlation(tags, "co:") || !correlation(tags, "cv:") {
        return None;
    }
    tags.iter().find_map(|tag| {
        tag.strip_prefix("co:")
            .and_then(|id| uuid::Uuid::parse_str(id).ok())
    })
}

fn correlation(tags: &[String], prefix: &str) -> bool {
    let mut values = tags.iter().filter_map(|tag| tag.strip_prefix(prefix));
    let Some(value) = values.next() else {
        return false;
    };
    values.next().is_none() && uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}

fn exact_amount(
    actual: &Money,
    source: &crate::models::checkout_completion::ExactMoney,
    expected: i64,
) -> bool {
    actual.currency_code.eq_ignore_ascii_case(&source.currency)
        && source
            .parse_amount(&actual.amount)
            .is_ok_and(|amount| amount == expected)
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
    let mut shipping_lines = Vec::new();
    let mut has_shipping_allocation = false;
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
            has_shipping_allocation = true;
            shipping_lines.extend(
                refund
                    .refund_shipping_lines
                    .nodes
                    .iter()
                    .map(|line| &line.subtotal_amount_set.shop_money),
            );
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
        let allocation = sale_details(order, payment)?;
        adjusted = allocation.order_adjusted;
        include_items = allocation.include_items;
        if allocation.include_shipping {
            has_shipping_allocation = true;
            shipping_lines.extend(
                order
                    .shipping_lines
                    .nodes
                    .iter()
                    .map(|line| &line.price_set.shop_money),
            );
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
    let shipping = if payment.transaction.gateway == AccountingGateway::Paypal
        && payment.transaction.shipping_fee.is_some()
    {
        payment.transaction.shipping_fee
    } else if has_shipping_allocation {
        shipping_total(&shipping_lines, payment)?
    } else if payment.transaction.gateway == AccountingGateway::Stripe {
        Some(0.0)
    } else {
        None
    };
    let tx = &mut payment.transaction;
    tx.shopify_order_id = id;
    tx.shopify_order_number = order.number;
    tx.shopify_order_tags.clone_from(&order.tags);
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

struct SaleAllocation {
    include_shipping: bool,
    order_adjusted: bool,
    include_items: bool,
}

fn sale_details(order: &Order, payment: &Payment) -> Result<SaleAllocation, AppError> {
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
    let external =
        external_sale_transaction(order, payment)?.map(|position| &order.transactions[position]);
    let mut position = None;
    let mut total = 0.0;
    for (index, tx) in captured.iter().enumerate() {
        total += money(&tx.amount_set.shop_money.amount)?;
        if matches(tx, payment)? || external.is_some_and(|candidate| std::ptr::eq(*tx, candidate)) {
            position = Some(index);
        }
    }
    let Some(index) = position else {
        return Ok(SaleAllocation {
            include_shipping: false,
            order_adjusted: false,
            include_items: false,
        });
    };
    let amount = money(&captured[index].amount_set.shop_money.amount)?;
    if (amount - total_items - shipping).abs() < 0.005 {
        return Ok(SaleAllocation {
            include_shipping: true,
            order_adjusted: false,
            include_items: true,
        });
    }
    if index == 0 && total > 0.0 && total <= total_items + shipping {
        Ok(SaleAllocation {
            include_shipping: true,
            order_adjusted: true,
            include_items: true,
        })
    } else {
        Ok(SaleAllocation {
            include_shipping: false,
            order_adjusted: false,
            include_items: false,
        })
    }
}

fn shipping_total(lines: &[&Money], payment: &Payment) -> Result<Option<f64>, AppError> {
    let mut total = 0.0;
    for line in lines {
        let value = if payment.transaction.gateway == AccountingGateway::Paypal {
            if !line
                .currency_code
                .eq_ignore_ascii_case(&payment.transaction.currency)
            {
                return Ok(None);
            }
            money(&line.amount)?
        } else {
            thb(line, payment)?
        };
        total += if payment.transaction.kind == AccountingKind::Refund {
            -value.abs()
        } else {
            value
        };
    }
    if !total.is_finite() {
        return Err(protocol("invalid shipping amount"));
    }
    Ok(Some(total))
}

fn thb(amount: &Money, payment: &Payment) -> Result<f64, AppError> {
    let value = money(&amount.amount)?;
    if amount.currency_code.eq_ignore_ascii_case("THB") {
        return Ok(value);
    }
    let rate = payment
        .transaction
        .exchange_rate
        .ok_or_else(|| protocol("Stripe shipping exchange rate missing"))?;
    if !amount.currency_code.eq_ignore_ascii_case(&payment.currency)
        || payment.currency.eq_ignore_ascii_case("THB")
        || !rate.is_finite()
        || rate <= 0.0
    {
        return Err(protocol("incompatible Shopify shipping currency"));
    }
    let result = value * rate;
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
    let sku = line.sku.clone().map(Sku::into_string).unwrap_or_default();
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
        category: SkuCategory::from_sku(&sku).as_str().to_owned(),
        sku,
        kind,
        transaction_date: date.to_owned(),
        shopify_order_id: order_id,
        payment_reference: reference.to_owned(),
    }
}

fn address(order: &Order) -> String {
    let Some(address) = &order.shipping_address else {
        return String::new();
    };
    format_address(&[
        address.address1.as_deref(),
        address.address2.as_deref(),
        address.city.as_deref(),
        address.province_code.as_deref(),
        address.country.as_deref(),
        address.zip.as_deref(),
    ])
}

fn phone(order: &Order) -> u64 {
    phone_number(
        order
            .shipping_address
            .as_ref()
            .and_then(|a| a.phone.as_deref())
            .unwrap_or_default(),
    )
}
#[cfg(test)]
#[path = "../../tests/services/accounting/shopify.rs"]
mod tests;
