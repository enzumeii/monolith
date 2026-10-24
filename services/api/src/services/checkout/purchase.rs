use super::{CheckoutClients, conflict, provider_name, unavailable};
use crate::{
    config::CheckoutSettings,
    error::{AppError, upstream},
    models::{
        checkout::{Checkout, CompletionState, DraftState, PaymentState},
        checkout_completion::ExactMoney,
        shopify::{
            CompletionDraft, CompletionDraftStatus, CreateDraftOrderInput, DraftOrderLineItemInput,
            Money, PageRequest, ShopifyId,
        },
    },
    repositories::{checkout as repo, conventions},
};
use chrono::{Duration, Utc};
use domain::dto::{
    CatalogueInventoryPolicy,
    checkout::{CheckoutMoney, CheckoutProvider, CreateCheckoutRequest},
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub(super) struct PreparedPurchase {
    email: String,
    pub money: CheckoutMoney,
    items: Vec<PreparedItem>,
    tags: Vec<String>,
}

struct PreparedItem {
    variant_id: Uuid,
    shopify_product_id: String,
    shopify_variant_id: String,
    quantity: u32,
    unit_price_minor: i64,
}

pub(super) async fn prepare(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    request: &CreateCheckoutRequest,
    guest_hash: &[u8],
    fingerprint: Vec<u8>,
) -> Result<(Checkout, PreparedPurchase), AppError> {
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
    let account_id = provider_account(settings, clients, request.provider)?;
    let selected = conventions::variants(pool, convention_id).await?;
    let (items, total) = prepare_items(request, &selected, &convention.currency_code, provider)?;
    let purchase = PreparedPurchase {
        email: request.email.clone(),
        money: CheckoutMoney {
            minor: total,
            currency: convention.currency_code,
        },
        items,
        tags: vec![
            format!("convention:{convention_id}"),
            convention.name,
            format!("checkout:{id}"),
        ],
    };
    let lines = purchase
        .items
        .iter()
        .map(|item| (item.shopify_variant_id.clone(), i64::from(item.quantity)))
        .collect();
    let now = Utc::now();
    let row = Checkout {
        id,
        guest_secret_hash: guest_hash.to_vec(),
        owner_expires_at: now
            + Duration::seconds(
                i64::try_from(settings.guest_recovery_secs)
                    .map_err(|_| conflict("Invalid guest recovery duration"))?,
            ),
        request_fingerprint: fingerprint,
        convention_id,
        provider: provider.into(),
        provider_account_id: account_id,
        provider_environment: if provider == "stripe" {
            settings.stripe_livemode.to_string()
        } else {
            settings.paypal_api_origin.clone()
        },
        evidence_hash: evidence_hash(&purchase.email, &purchase.money, &lines)?,
        required_tag_hashes: purchase
            .tags
            .iter()
            .map(|tag| Sha256::digest(tag.as_bytes()).to_vec())
            .collect(),
        success_url: Some(return_url(
            settings,
            &convention.slug,
            id,
            &settings.return_path_template,
        )),
        cancel_url: Some(return_url(
            settings,
            &convention.slug,
            id,
            &settings.cancel_path_template,
        )),
        gateway_request_fingerprint: None,
        payment_state: PaymentState::NotStarted,
        draft_state: DraftState::NotStarted,
        completion_state: CompletionState::NotStarted,
        draft_id: None,
        provider_resource_id: None,
        order_id: None,
        reservation_expires_at: None,
        created_at: now,
    };
    Ok((row, purchase))
}

pub(super) async fn reserve(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    purchase: &PreparedPurchase,
    token: Uuid,
) -> Result<(), AppError> {
    validate_stock(settings, clients, purchase).await?;
    let deadline = Utc::now()
        + Duration::seconds(
            i64::try_from(settings.reservation_secs)
                .map_err(|_| conflict("Invalid reservation duration"))?,
        );
    let input = CreateDraftOrderInput {
        line_items: purchase
            .items
            .iter()
            .map(|item| {
                Ok(DraftOrderLineItemInput {
                    variant_id: ShopifyId::new(&item.shopify_variant_id)
                        .map_err(|error| upstream(error))?,
                    quantity: i32::try_from(item.quantity)
                        .map_err(|_| conflict("Invalid quantity"))?,
                    price_override: Some(Money {
                        amount: amount(item.unit_price_minor, &purchase.money.currency)?,
                        currency_code: purchase.money.currency.clone(),
                    }),
                })
            })
            .collect::<Result<_, AppError>>()?,
        tags: purchase.tags.clone(),
        email: Some(purchase.email.clone()),
        presentment_currency_code: Some(purchase.money.currency.clone()),
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
    checkout.draft_state = DraftState::Uncertain;
    checkout.draft_id = created.map(|draft| draft.id.as_ref().to_owned());
    repo::save(pool, checkout, token).await?;
    let draft = read(clients, checkout).await?;
    validate_reservation(checkout, &draft)?;
    validate_payable(&draft)?;
    if draft
        .reserve_inventory_until
        .is_none_or(|expiry| expiry < Utc::now() + Duration::minutes(30))
    {
        return Err(conflict(
            "Shopify reservation does not match the accepted purchase",
        ));
    }
    checkout.reservation_expires_at = draft.reserve_inventory_until;
    checkout.draft_state = DraftState::Reserved;
    repo::save(pool, checkout, token).await
}

pub(super) async fn recover(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
) -> Result<(), AppError> {
    if checkout.draft_id.is_none() {
        let query = format!("tag:'checkout:{}'", checkout.id);
        let expected_tag = format!("checkout:{}", checkout.id);
        let mut found = Vec::new();
        super::super::pagination::for_each_page(
            None,
            |after| {
                let query = &query;
                async move {
                    clients
                        .shopify
                        .draft_orders(PageRequest { first: 250, after }, Some(query))
                        .await
                        .map_err(|error| upstream(error))
                }
            },
            |drafts| {
                for draft in drafts {
                    if draft.tags.contains(&expected_tag) {
                        validate_purchase(checkout, &draft)?;
                        validate_reservation(checkout, &draft)?;
                        found.push(draft.id.as_ref().to_owned());
                    }
                }
                Ok(())
            },
            |error| upstream(error),
        )
        .await?;
        if found.len() != 1 {
            return Err(conflict("Shopify draft creation requires reconciliation"));
        }
        checkout.draft_id = found.pop();
        repo::save(pool, checkout, token).await?;
    }
    let draft = read(clients, checkout).await?;
    if checkout.payment_state == PaymentState::NotStarted {
        validate_reservation(checkout, &draft)?;
        validate_payable(&draft)?;
        checkout.reservation_expires_at = draft.reserve_inventory_until;
    }
    checkout.draft_state = DraftState::Reserved;
    repo::save(pool, checkout, token).await
}

pub(super) async fn read(
    clients: &CheckoutClients<'_>,
    checkout: &Checkout,
) -> Result<CompletionDraft, AppError> {
    let id = ShopifyId::new(checkout.draft_id.as_deref().ok_or_else(|| {
        conflict("Checkout draft is not available; original purchase or reconciliation is required")
    })?)
    .map_err(|error| upstream(error))?;
    let draft = clients
        .shopify
        .checkout_draft(&id)
        .await
        .map_err(|error| upstream(error))?
        .ok_or_else(|| conflict("Reserved Shopify draft is missing"))?;
    validate_purchase(checkout, &draft)?;
    Ok(draft)
}

pub(super) fn validate_purchase(
    checkout: &Checkout,
    draft: &CompletionDraft,
) -> Result<CheckoutMoney, AppError> {
    let money = draft_money(draft)?;
    let lines = draft
        .line_items
        .as_ref()
        .ok_or_else(|| conflict("Shopify reservation lines are missing"))?;
    if lines.page_info.has_next_page {
        return Err(conflict("Shopify reservation lines are incomplete"));
    }
    let mut actual = BTreeMap::new();
    for line in &lines.nodes {
        let id = line
            .variant
            .as_ref()
            .ok_or_else(|| conflict("Shopify reservation variant is missing"))?
            .id
            .as_ref()
            .to_owned();
        if line.quantity <= 0 || actual.insert(id, line.quantity).is_some() {
            return Err(conflict("Shopify reservation has invalid variants"));
        }
    }
    let email = draft
        .email
        .as_deref()
        .ok_or_else(|| conflict("Shopify reservation email is missing"))?;
    if actual.is_empty()
        || evidence_hash(email, &money, &actual)? != checkout.evidence_hash
        || !tags_match(checkout, &draft.tags)
    {
        return Err(conflict(
            "Shopify draft no longer matches the accepted purchase",
        ));
    }
    Ok(money)
}

pub(super) fn tags_match(checkout: &Checkout, tags: &[String]) -> bool {
    checkout.required_tag_hashes.iter().all(|expected| {
        tags.iter()
            .any(|tag| Sha256::digest(tag.as_bytes()).as_slice() == expected.as_slice())
    })
}

pub(super) fn evidence_hash(
    email: &str,
    money: &CheckoutMoney,
    lines: &BTreeMap<String, i64>,
) -> Result<Vec<u8>, AppError> {
    Ok(Sha256::digest(
        serde_json::to_vec(&(email, money, lines))
            .map_err(|error| AppError::from(anyhow::Error::new(error)))?,
    )
    .to_vec())
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

pub(super) fn validate_payable(draft: &CompletionDraft) -> Result<(), AppError> {
    if !draft.ready
        || !matches!(draft.status, CompletionDraftStatus::Open)
        || draft.order.is_some()
        || draft.payment_terms.is_some()
    {
        return Err(conflict("Shopify draft is not eligible for payment"));
    }
    Ok(())
}

pub(super) fn validate_reservation(
    checkout: &Checkout,
    draft: &CompletionDraft,
) -> Result<(), AppError> {
    if checkout.reservation_expires_at.is_none_or(|requested| {
        draft
            .reserve_inventory_until
            .is_none_or(|actual| actual < requested - Duration::seconds(1))
    }) {
        return Err(conflict(
            "Shopify reservation does not match the accepted purchase",
        ));
    }
    Ok(())
}

fn provider_account(
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    provider: CheckoutProvider,
) -> Result<String, AppError> {
    Ok(match provider {
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
    })
}

fn draft_money(draft: &CompletionDraft) -> Result<CheckoutMoney, AppError> {
    let value = &draft.total_price_set.presentment_money;
    let minor = exact(0, &value.currency_code)?.parse_amount(&value.amount)?;
    if minor <= 0 {
        return Err(conflict("Shopify draft amount is invalid"));
    }
    Ok(CheckoutMoney {
        minor,
        currency: value.currency_code.clone(),
    })
}

fn return_url(settings: &CheckoutSettings, slug: &str, id: Uuid, template: &str) -> String {
    format!(
        "{}{}",
        settings.frontend_origin.trim_end_matches('/'),
        template
            .replace("{convention_slug}", slug)
            .replace("{checkout_id}", &id.to_string())
    )
}

fn prepare_items(
    request: &CreateCheckoutRequest,
    selected: &[crate::models::conventions::ConventionVariant],
    currency: &str,
    provider: &str,
) -> Result<(Vec<PreparedItem>, i64), AppError> {
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total = 0i64;
    for input in &request.items {
        let internal_id = Uuid::parse_str(&input.variant_id).ok();
        let variant = selected
            .iter()
            .find(|variant| {
                internal_id == Some(variant.id) || input.variant_id == variant.shopify_variant_id
            })
            .ok_or_else(|| conflict("Variant is not selected for this convention"))?;
        let variant_id = variant.id;
        if !seen.insert(variant_id) || !(1..=999).contains(&input.quantity) {
            return Err(conflict("Invalid checkout item selection"));
        }
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
        items.push(PreparedItem {
            variant_id,
            shopify_product_id: variant.shopify_product_id.clone(),
            shopify_variant_id: variant.shopify_variant_id.clone(),
            quantity: input.quantity,
            unit_price_minor,
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
    purchase: &PreparedPurchase,
) -> Result<(), AppError> {
    let location =
        ShopifyId::new(&settings.shopify_location_id).map_err(|error| upstream(error))?;
    let mut ids = BTreeSet::new();
    for item in &purchase.items {
        ids.insert(
            ShopifyId::new(&item.shopify_product_id)
                .map_err(|error| upstream(error))?
                .number("Product")
                .map_err(|error| upstream(error))?,
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
                    if let Some(item) = purchase
                        .items
                        .iter()
                        .find(|item| item.shopify_variant_id == variant.id)
                    {
                        if product.id != item.shopify_product_id
                            || !variant.eligible
                            || !variant.tracked
                            || variant.inventory_policy != CatalogueInventoryPolicy::Deny
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
    if found.len() != purchase.items.len() {
        return Err(conflict("Selected inventory is unavailable"));
    }
    Ok(())
}
