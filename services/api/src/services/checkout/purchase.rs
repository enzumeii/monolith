use super::{CheckoutClients, conflict, provider_name, unavailable};
use crate::{
    config::CheckoutSettings,
    error::AppError,
    models::checkout::{Checkout, CompletionState, DraftState, PaymentState},
    repositories::checkout as repo,
};
use crate::{
    error::upstream,
    models::{
        checkout::{CheckoutItemSnapshot, CheckoutProviderContext, CheckoutSnapshot},
        checkout_completion::ExactMoney,
        shopify::{CreateDraftOrderInput, DraftOrderLineItemInput, Money, PageRequest, ShopifyId},
    },
    repositories::conventions,
};
use chrono::{Duration, Utc};
use domain::dto::checkout::{CheckoutMoney, CheckoutProvider, CreateCheckoutRequest};
use sqlx::PgPool;
use sqlx::types::Json;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub(super) async fn prepare(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    request: &CreateCheckoutRequest,
    guest_hash: &[u8],
    fingerprint: Vec<u8>,
) -> Result<Checkout, AppError> {
    let id = Uuid::parse_str(&request.operation_id)
        .map_err(|_| conflict("Invalid operation identity"))?;
    let convention_id = Uuid::parse_str(&request.convention_id)
        .map_err(|_| conflict("Invalid convention identity"))?;
    let convention = conventions::find(pool, convention_id)
        .await?
        .ok_or_else(|| conflict("Convention is unavailable"))?;
    let provider = provider_name(request.provider);
    if !convention.preorder_enabled
        || !convention
            .payment_methods
            .iter()
            .any(|method| method == provider)
    {
        return Err(conflict("Convention or payment method is unavailable"));
    }
    if request.expected_currency != convention.currency_code {
        return Err(conflict("Convention currency changed; refresh the cart"));
    }
    let account_id = match request.provider {
        CheckoutProvider::Stripe => {
            clients.stripe.ok_or_else(unavailable)?;
            settings.stripe_expected_account.clone()
        }
        CheckoutProvider::Paypal => {
            clients.paypal.ok_or_else(unavailable)?;
            settings
                .paypal_merchant_id
                .clone()
                .filter(|v| !v.is_empty())
                .ok_or_else(unavailable)?
        }
    };
    let selected = conventions::variants(pool, convention_id).await?;
    let (items, total) = prepare_items(request, &selected, &convention.currency_code, provider)?;
    let now = Utc::now();
    Ok(Checkout {
        id,
        guest_secret_hash: guest_hash.to_vec(),
        owner_expires_at: now
            + Duration::seconds(
                i64::try_from(settings.guest_recovery_secs)
                    .map_err(|_| conflict("Invalid guest recovery duration"))?,
            ),
        request_fingerprint: fingerprint,
        provider: provider.into(),
        snapshot: Json(CheckoutSnapshot {
            convention_id,
            convention_slug: convention.slug,
            convention_name: convention.name,
            email: request.email.clone(),
            money: CheckoutMoney {
                minor: total,
                currency: convention.currency_code,
            },
            items,
        }),
        provider_context: Json(CheckoutProviderContext {
            account_id,
            environment: if provider == "stripe" {
                settings.stripe_livemode.to_string()
            } else {
                settings.paypal_api_origin.clone()
            },
            correlation_id: id.to_string(),
        }),
        payment_state: PaymentState::NotStarted,
        draft_state: DraftState::NotStarted,
        completion_state: CompletionState::NotStarted,
        draft_id: None,
        provider_resource_id: None,
        payment_reference: None,
        order_id: None,
        order_name: None,
        redirect_url: None,
        reservation_expires_at: None,
        create_request_id: format!("checkout-{id}"),
        capture_request_id: format!("capture-{id}"),
        created_at: now,
    })
}

pub(super) async fn reserve(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
) -> Result<(), AppError> {
    validate_stock(settings, clients, checkout).await?;
    let deadline = Utc::now()
        + Duration::seconds(
            i64::try_from(settings.reservation_secs)
                .map_err(|_| conflict("Invalid reservation duration"))?,
        );
    let input = CreateDraftOrderInput {
        line_items: checkout
            .snapshot
            .items
            .iter()
            .map(|item| {
                Ok(DraftOrderLineItemInput {
                    variant_id: ShopifyId::new(&item.shopify_variant_id).map_err(upstream)?,
                    quantity: i32::try_from(item.quantity)
                        .map_err(|_| conflict("Invalid quantity"))?,
                    price_override: Some(Money {
                        amount: amount(item.unit_price_minor, &checkout.snapshot.money.currency)?,
                        currency_code: checkout.snapshot.money.currency.clone(),
                    }),
                })
            })
            .collect::<Result<_, AppError>>()?,
        tags: convention_tags(checkout),
        email: Some(checkout.snapshot.email.clone()),
        presentment_currency_code: Some(checkout.snapshot.money.currency.clone()),
        reserve_inventory_until: Some(deadline),
        session_token: Some(checkout.id.to_string()),
        ..Default::default()
    };
    checkout.draft_state = DraftState::Creating;
    checkout.reservation_expires_at = Some(deadline);
    repo::save(pool, checkout, token).await?;
    let created = match clients.shopify.create_draft_order(&input).await {
        Ok(payload) if payload.user_errors.is_empty() => payload.draft_order,
        Ok(_) => {
            checkout.draft_state = DraftState::Uncertain;
            repo::save(pool, checkout, token).await?;
            return Err(conflict("Shopify could not reserve this purchase"));
        }
        Err(error) => {
            checkout.draft_state = DraftState::Uncertain;
            repo::save(pool, checkout, token).await?;
            return Err(upstream(error));
        }
    };
    let Some(created) = created else {
        checkout.draft_state = DraftState::Uncertain;
        repo::save(pool, checkout, token).await?;
        return Err(conflict("Shopify draft creation is uncertain"));
    };
    checkout.draft_id = Some(created.id.as_ref().to_owned());
    checkout.draft_state = DraftState::Uncertain;
    repo::save(pool, checkout, token).await?;
    let actual = clients
        .shopify
        .checkout_draft(&created.id)
        .await
        .map_err(upstream)?
        .ok_or_else(|| conflict("Reserved Shopify draft is missing"))?;
    validate_draft(checkout, &actual, deadline)?;
    checkout.reservation_expires_at = actual.reserve_inventory_until;
    checkout.draft_state = DraftState::Reserved;
    repo::save(pool, checkout, token).await
}

pub(super) fn validate_draft(
    checkout: &Checkout,
    draft: &crate::models::shopify::CompletionDraft,
    requested: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    use crate::models::shopify::CompletionDraftStatus;
    validate_purchase(checkout, draft)?;
    if !draft.ready
        || !matches!(draft.status, CompletionDraftStatus::Open)
        || draft.order.is_some()
        || draft.payment_terms.is_some()
        || draft.reserve_inventory_until.is_none_or(|expiry| {
            expiry < requested - Duration::seconds(1) || expiry < Utc::now() + Duration::minutes(30)
        })
    {
        return Err(conflict(
            "Shopify reservation does not match the accepted purchase",
        ));
    }
    Ok(())
}

pub(super) fn validate_purchase(
    checkout: &Checkout,
    draft: &crate::models::shopify::CompletionDraft,
) -> Result<(), AppError> {
    let expected = exact(
        checkout.snapshot.money.minor,
        &checkout.snapshot.money.currency,
    )?;
    let lines = draft
        .line_items
        .as_ref()
        .ok_or_else(|| conflict("Shopify reservation lines are missing"))?;
    let mut actual = BTreeMap::new();
    for line in &lines.nodes {
        let id = line
            .variant
            .as_ref()
            .ok_or_else(|| conflict("Shopify reservation variant is missing"))?
            .id
            .as_ref()
            .to_owned();
        if actual.insert(id, line.quantity).is_some() {
            return Err(conflict("Shopify reservation has duplicate variants"));
        }
    }
    let wanted: BTreeMap<_, _> = checkout
        .snapshot
        .items
        .iter()
        .map(|item| (item.shopify_variant_id.clone(), i64::from(item.quantity)))
        .collect();
    if draft.email.as_deref() != Some(checkout.snapshot.email.as_str())
        || lines.page_info.has_next_page
        || actual != wanted
        || draft.total_price_set.presentment_money.currency_code != expected.currency
        || expected.parse_amount(&draft.total_price_set.presentment_money.amount)? != expected.minor
        || convention_tags(checkout)
            .iter()
            .any(|tag| !draft.tags.contains(tag))
    {
        return Err(conflict(
            "Shopify draft no longer matches the accepted purchase",
        ));
    }
    Ok(())
}

pub(super) fn convention_tags(checkout: &Checkout) -> Vec<String> {
    vec![
        format!("convention:{}", checkout.snapshot.convention_id),
        checkout.snapshot.convention_name.clone(),
        format!("checkout:{}", checkout.id),
    ]
}
pub(super) fn exact(minor: i64, currency: &str) -> Result<ExactMoney, AppError> {
    let precision = super::super::conventions::money::precision(currency)
        .map_err(|_| conflict("Unsupported currency"))?;
    ExactMoney::new(
        minor,
        currency,
        u8::try_from(precision).map_err(|_| conflict("Invalid currency precision"))?,
    )
}
pub(super) fn amount(minor: i64, currency: &str) -> Result<String, AppError> {
    Ok(format_minor(minor, exact(minor, currency)?.precision))
}

pub(super) fn format_minor(minor: i64, precision: u8) -> String {
    let scale = 10i64.pow(u32::from(precision));
    if precision == 0 {
        minor.to_string()
    } else {
        format!(
            "{}.{:0width$}",
            minor / scale,
            minor % scale,
            width = usize::from(precision)
        )
    }
}

fn prepare_items(
    request: &CreateCheckoutRequest,
    selected: &[crate::models::conventions::ConventionVariant],
    currency: &str,
    provider: &str,
) -> Result<(Vec<CheckoutItemSnapshot>, i64), AppError> {
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total = 0i64;
    for input in &request.items {
        let variant_id = Uuid::parse_str(&input.variant_id)
            .map_err(|_| conflict("Invalid convention variant"))?;
        if !seen.insert(variant_id) || !(1..=999).contains(&input.quantity) {
            return Err(conflict("Invalid checkout item selection"));
        }
        let variant = selected
            .iter()
            .find(|variant| variant.id == variant_id)
            .ok_or_else(|| conflict("Variant is not selected for this convention"))?;
        let money = exact(0, currency)?;
        let canonical = super::super::conventions::money::validate(
            &variant.final_price,
            currency,
            &[match request.provider {
                CheckoutProvider::Stripe => domain::dto::ConventionPaymentMethod::Stripe,
                CheckoutProvider::Paypal => domain::dto::ConventionPaymentMethod::Paypal,
            }],
        )
        .map_err(|_| conflict("Invalid convention price"))?;
        let unit_price_minor = money.parse_amount(&canonical)?;
        if input.expected_unit_price != unit_price_minor {
            return Err(conflict("Convention price changed; refresh the cart"));
        }
        let line_total = unit_price_minor
            .checked_mul(i64::from(input.quantity))
            .ok_or_else(|| conflict("Checkout amount overflow"))?;
        total = total
            .checked_add(line_total)
            .ok_or_else(|| conflict("Checkout amount overflow"))?;
        items.push(CheckoutItemSnapshot {
            variant_id,
            shopify_product_id: variant.shopify_product_id.clone(),
            shopify_variant_id: variant.shopify_variant_id.clone(),
            product_title: variant.product_title.clone(),
            variant_title: variant.variant_title.clone(),
            sku: variant.sku.clone(),
            quantity: input.quantity,
            unit_price_minor,
            total_minor: line_total,
        });
    }
    if items.is_empty()
        || items.len() > 100
        || total <= 0
        || (provider == "stripe" && total > 99_999_999)
    {
        return Err(conflict("Invalid checkout total or item count"));
    }
    Ok((items, total))
}

async fn validate_stock(
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    checkout: &Checkout,
) -> Result<(), AppError> {
    let location = ShopifyId::new(&settings.shopify_location_id).map_err(upstream)?;
    let mut ids = BTreeSet::new();
    for item in &checkout.snapshot.items {
        ids.insert(
            ShopifyId::new(&item.shopify_product_id)
                .map_err(upstream)?
                .number("Product")
                .map_err(upstream)?,
        );
    }
    let search = ids
        .into_iter()
        .map(|id| format!("id:{id}"))
        .collect::<Vec<_>>()
        .join(" OR ");
    let mut found = BTreeSet::new();
    let search = &search;
    let location = &location;
    super::super::pagination::for_each_page(
        None,
        |after| async move {
            let page = super::super::catalogue::products(
                clients.shopify,
                location.clone(),
                PageRequest { first: 100, after },
                Some(search),
            )
            .await?;
            Ok(crate::models::shopify::Connection {
                nodes: page.products,
                page_info: page.page_info,
            })
        },
        |products| {
            for product in products {
                for variant in product.variants {
                    if let Some(item) = checkout
                        .snapshot
                        .items
                        .iter()
                        .find(|item| item.shopify_variant_id == variant.id)
                    {
                        if product.id != item.shopify_product_id
                            || !variant.eligible
                            || !variant.tracked
                            || variant
                                .available_quantity
                                .is_none_or(|available| available < i64::from(item.quantity))
                        {
                            return Err(conflict("Selected inventory is unavailable"));
                        }
                        found.insert(item.variant_id);
                    }
                }
            }
            Ok(())
        },
        |error| upstream(error),
    )
    .await?;
    if found.len() != checkout.snapshot.items.len() {
        return Err(conflict("Selected inventory is unavailable"));
    }
    Ok(())
}
