use super::{Payment, date, money, protocol, quantity};
use crate::{
    client::shopify::ShopifyClient,
    error::{AppError, upstream},
    models::{
        Sku, SkuCategory,
        checkout_completion::CompletionState,
        shopify::{AccountingLineItem, Money, Order, OrderTransaction, PageRequest},
    },
    services::pagination::for_each_page,
    utils::contact::{format_address, phone_number},
    validations::accounting::AccountingDateRange,
};
use domain::dto::accounting::AccountingTransactions;
use domain::dto::{AccountingGateway, AccountingItem, AccountingKind, AccountingReportResponse};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(super) async fn enrich(
    pool: &sqlx::PgPool,
    client: &ShopifyClient,
    stripe: &crate::client::stripe::StripeClient,
    paypal: &crate::client::paypal::PaypalClient,
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
    let mappings = super::checkout::mappings(pool, stripe, paypal, &payments).await?;
    let mut orders = discover(client, &payments, range).await?;
    let mut seen = orders
        .iter()
        .map(|order| order.id.clone())
        .collect::<HashSet<_>>();
    for mapping in mappings.values() {
        if let Some(id) = &mapping.order_id
            && !seen.contains(id)
        {
            let order = client
                .accounting_order(id)
                .await
                .map_err(upstream)?
                .ok_or_else(|| protocol("mapped Shopify accounting order missing"))?;
            if order.id != *id {
                return Err(protocol("mapped Shopify accounting order ID mismatch"));
            }
            complete(&order)?;
            seen.insert(id.clone());
            orders.push(order);
        }
    }
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
    let mut allocated_checkouts = HashSet::new();
    for mut payment in payments {
        let tx = &payment.transaction;
        let key = (
            gateway_key(tx.gateway),
            tx.kind == AccountingKind::Refund,
            tx.payment_reference.clone(),
        );
        let native = index.get(&key).map(|position| &orders[*position]);
        let mapping = if tx.kind == AccountingKind::Sale {
            mappings.get(&(key.0, key.2.clone()))
        } else {
            None
        };
        let local = mapping
            .and_then(|mapping| mapping.order_id.as_ref())
            .and_then(|id| orders.iter().find(|order| order.id == *id));
        if let (Some(native), Some(local)) = (native, local)
            && native.id != local.id
        {
            return Err(protocol("native and checkout accounting orders disagree"));
        }
        if let Some(order) = native.or(local) {
            if let (Some(mapping), Some(_)) = (mapping, local) {
                validate_checkout_order(order, &mapping.snapshot, mapping.completion_state)?;
            }
            apply(&mut payment, order, &mut response.items)?;
            if native.is_none()
                && let Some(mapping) = mapping
            {
                apply_checkout_sale(
                    &mut payment,
                    order,
                    &mut response.items,
                    allocated_checkouts.insert(mapping.checkout_id),
                )?;
            }
        }
        match payment.transaction.gateway {
            AccountingGateway::Stripe => response.transactions.stripe.push(payment.transaction),
            AccountingGateway::Paypal => response.transactions.paypal.push(payment.transaction),
        }
    }
    Ok(response)
}

async fn discover(
    client: &ShopifyClient,
    payments: &[Payment],
    range: &AccountingDateRange,
) -> Result<Vec<Order>, AppError> {
    let mut start = range.start;
    let mut end = range.end;
    for payment in payments {
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
    Ok(orders)
}

fn validate_checkout_order(
    order: &Order,
    snapshot: &crate::models::checkout_completion::Snapshot,
    state: CompletionState,
) -> Result<(), AppError> {
    let accepted = &snapshot.accepted;
    let exact = |money: Option<&Money>, expected: i64| -> Result<bool, AppError> {
        let money =
            money.ok_or_else(|| protocol("mapped Shopify accounting presentment missing"))?;
        Ok(money.currency_code == accepted.currency
            && accepted
                .parse_amount(&money.amount)
                .map_err(|_| protocol("invalid mapped Shopify accounting amount"))?
                == expected)
    };
    if !exact(
        order
            .total_price_set
            .as_ref()
            .and_then(|m| m.presentment_money.as_ref()),
        accepted.minor,
    )? || !exact(
        order
            .total_shipping_price_set
            .as_ref()
            .and_then(|m| m.presentment_money.as_ref()),
        0,
    )? || order.line_items.nodes.len() != snapshot.lines.len()
    {
        return Err(protocol("mapped Shopify accounting total mismatch"));
    }
    let mut remaining = snapshot.lines.iter().collect::<Vec<_>>();
    for line in order
        .line_items
        .nodes
        .iter()
        .filter(|line| line.variant.is_some())
    {
        let variant = line
            .variant
            .as_ref()
            .ok_or_else(|| protocol("mapped Shopify accounting variant missing"))?;
        let position = remaining
            .iter()
            .position(|reserved| reserved.variant_id == variant.id)
            .ok_or_else(|| protocol("mapped Shopify accounting variant mismatch"))?;
        let reserved = remaining.remove(position);
        if line.quantity != reserved.quantity
            || !exact(
                line.original_unit_price_set.presentment_money.as_ref(),
                reserved.unit_price.minor,
            )?
        {
            return Err(protocol("mapped Shopify accounting line mismatch"));
        }
    }
    for line in order
        .line_items
        .nodes
        .iter()
        .filter(|line| line.variant.is_none())
    {
        if state != CompletionState::Complete {
            return Err(protocol("mapped Shopify accounting variant missing"));
        }
        let price = line
            .original_unit_price_set
            .presentment_money
            .as_ref()
            .ok_or_else(|| protocol("mapped Shopify accounting presentment missing"))?;
        let minor = accepted
            .parse_amount(&price.amount)
            .map_err(|_| protocol("invalid mapped Shopify accounting historical amount"))?;
        let position = remaining
            .iter()
            .position(|reserved| {
                reserved.quantity == line.quantity
                    && reserved.unit_price.minor == minor
                    && reserved.unit_price.currency == price.currency_code
            })
            .ok_or_else(|| protocol("mapped Shopify accounting historical line mismatch"))?;
        remaining.remove(position);
    }
    Ok(())
}

fn apply_checkout_sale(
    payment: &mut Payment,
    order: &Order,
    items: &mut Vec<AccountingItem>,
    allocate_items: bool,
) -> Result<(), AppError> {
    if payment
        .transaction
        .shipping_fee
        .is_some_and(|amount| amount != 0.0)
    {
        return Err(protocol("mapped checkout provider shipping mismatch"));
    }
    let id = order.id.number("Order").map_err(upstream)?;
    if allocate_items {
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
    payment.transaction.shipping_fee = Some(0.0);
    Ok(())
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
    let mut position = None;
    let mut total = 0.0;
    for (index, tx) in captured.iter().enumerate() {
        total += money(&tx.amount_set.shop_money.amount)?;
        if matches(tx, payment)? {
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
