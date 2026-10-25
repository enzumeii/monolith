use super::{AdvanceError, CheckoutClients, conflict, correlation, provider_name, unavailable};
use crate::{
    config::CheckoutSettings,
    error::{AppError, upstream},
    models::{
        checkout::{Checkout, CompletionState, DraftState, PaymentRecordingState, PaymentState},
        checkout_completion::{ExactMoney, evidence_conflict},
        shopify::{
            CompletionDraft, CompletionDraftStatus, CreateDraftOrderInput, CustomAttribute,
            DraftOrderLineItemInput, Money, PageRequest, ShopifyId,
        },
    },
    repositories, services,
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

const MAX_RECOVERY_PAGES: usize = 20;
const MAX_RECOVERY_DRAFTS: usize = 5_000;

pub(super) struct PreparedPurchase {
    email: String,
    note: Option<String>,
    pub money: CheckoutMoney,
    items: Vec<PreparedItem>,
    tags: Vec<String>,
    slug: Option<String>,
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
    existing: Option<&Checkout>,
) -> Result<(Checkout, PreparedPurchase), AppError> {
    let completion_version = existing.map_or(2, |checkout| checkout.completion_version);
    let correlation_version = existing.map_or(correlation::INDEPENDENT_VERSION, |checkout| {
        checkout.correlation_version
    });
    let id = Uuid::parse_str(&request.operation_id)
        .map_err(|_| conflict("Invalid operation identity"))?;
    let convention_id = Uuid::parse_str(&request.convention_id)
        .map_err(|_| conflict("Invalid convention identity"))?;
    let convention = repositories::conventions::find(pool, convention_id)
        .await?
        .ok_or_else(|| conflict("Convention is unavailable"))?;
    let provider = provider_name(request.provider);
    validate_convention_purchase(&convention, request)?;
    let (account_id, environment) =
        provider_identity(settings, clients, request.provider, existing).await?;
    let selected = repositories::conventions::variants(pool, convention_id).await?;
    let (items, total) = prepare_items(request, &selected, &convention.currency_code, provider)?;
    let slug = (completion_version == 2
        || matches!(
            correlation_version,
            correlation::METADATA_VERSION | correlation::INDEPENDENT_VERSION
        ))
    .then(|| convention.slug.clone());
    let purchase = PreparedPurchase {
        email: request.email.clone(),
        note: request.note.clone(),
        money: CheckoutMoney {
            minor: total,
            currency: convention.currency_code,
        },
        items,
        tags: if matches!(
            correlation_version,
            correlation::METADATA_VERSION | correlation::INDEPENDENT_VERSION
        ) {
            purchase_tags(convention_id, &convention.name, id, slug.as_deref())
        } else {
            legacy_purchase_tags(convention_id, &convention.name, id, slug.as_deref())
        },
        slug,
    };
    let identity = PurchaseIdentity {
        id,
        convention_id,
        completion_version,
        correlation_version,
        provider: provider.to_owned(),
        account_id,
        environment,
        convention_slug: convention.slug,
    };
    let row = purchase_record(
        settings,
        &purchase,
        identity,
        guest_hash,
        fingerprint,
        existing,
    )?;
    Ok((row, purchase))
}

pub(super) async fn reserve(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    purchase: &PreparedPurchase,
    token: Uuid,
) -> Result<(), AdvanceError> {
    validate_stock(settings, clients, purchase).await?;
    let deadline = Utc::now()
        + Duration::seconds(
            i64::try_from(settings.reservation_secs)
                .map_err(|_| conflict("Invalid reservation duration"))?,
        );
    let input = CreateDraftOrderInput {
        note: purchase.note.clone(),
        line_items: purchase
            .items
            .iter()
            .map(|item| {
                Ok(DraftOrderLineItemInput {
                    variant_id: ShopifyId::new(&item.shopify_variant_id)
                        .map_err(|error| upstream(error))?,
                    quantity: i32::try_from(item.quantity)
                        .map_err(|_| conflict("Invalid quantity"))?,
                    price_override: (checkout.correlation_version
                        != correlation::INDEPENDENT_VERSION)
                        .then(|| {
                            Ok::<_, AppError>(Money {
                                amount: amount(item.unit_price_minor, &purchase.money.currency)?,
                                currency_code: purchase.money.currency.clone(),
                            })
                        })
                        .transpose()?,
                })
            })
            .collect::<Result<_, AppError>>()?,
        tags: purchase.tags.clone(),
        custom_attributes: if matches!(
            checkout.correlation_version,
            correlation::METADATA_VERSION | correlation::INDEPENDENT_VERSION
        ) {
            correlation::identity_attributes(checkout)?
        } else {
            purchase
                .slug
                .iter()
                .map(|slug| CustomAttribute {
                    key: correlation::SLUG_KEY.into(),
                    value: slug.clone(),
                })
                .collect()
        },
        email: Some(purchase.email.clone()),
        presentment_currency_code: (checkout.correlation_version
            != correlation::INDEPENDENT_VERSION)
            .then(|| purchase.money.currency.clone()),
        reserve_inventory_until: Some(deadline),
        session_token: Some(checkout.id.to_string()),
        ..Default::default()
    };
    checkout.draft_state = DraftState::Creating;
    checkout.reservation_expires_at = Some(deadline);
    super::save(pool, checkout, token).await?;
    let created = match clients.shopify.create_draft_order(&input).await {
        Ok(payload) if payload.user_errors.is_empty() => payload.draft_order,
        Ok(payload) => {
            checkout.draft_state = DraftState::Uncertain;
            super::save(pool, checkout, token).await?;
            return Err(upstream(crate::client::ClientError::new(
                crate::client::ClientErrorKind::Mutation {
                    errors: payload.user_errors,
                },
            ))
            .into());
        }
        Err(error) => {
            checkout.draft_state = DraftState::Uncertain;
            super::save(pool, checkout, token).await?;
            return Err(upstream(error).into());
        }
    };
    checkout.draft_state = DraftState::Uncertain;
    checkout.draft_id = created.map(|draft| draft.id.as_ref().to_owned());
    super::save(pool, checkout, token).await?;
    let draft = read(clients, checkout).await?;
    validate_reservation(checkout, &draft)?;
    validate_payable(&draft)?;
    if draft
        .reserve_inventory_until
        .is_none_or(|expiry| expiry < Utc::now() + Duration::minutes(30))
    {
        return Err(conflict("Shopify reservation does not match the accepted purchase").into());
    }
    pin_shopify_money(checkout, &draft)?;
    checkout.reservation_expires_at = draft.reserve_inventory_until;
    checkout.draft_state = DraftState::Reserved;
    super::save(pool, checkout, token).await
}

pub(super) async fn recover(
    pool: &PgPool,
    clients: &CheckoutClients<'_>,
    checkout: &mut Checkout,
    token: Uuid,
) -> Result<(), AdvanceError> {
    if checkout.draft_id.is_none() {
        checkout.draft_id = Some(find_reserved_draft(clients, checkout).await?);
        super::save(pool, checkout, token).await?;
    }
    let draft = read(clients, checkout).await?;
    if checkout.payment_state == PaymentState::NotStarted {
        validate_reservation(checkout, &draft)?;
        validate_payable(&draft)?;
        pin_shopify_money(checkout, &draft)?;
        checkout.reservation_expires_at = draft.reserve_inventory_until;
    }
    checkout.draft_state = DraftState::Reserved;
    super::save(pool, checkout, token).await
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
    let evidence_money = purchase_evidence_money(checkout, draft, &money)?;
    let actual = reservation_lines(draft)?;
    let email = draft
        .email
        .as_deref()
        .ok_or_else(|| conflict("Shopify reservation email is missing"))?;
    if actual.is_empty()
        || bound_evidence_hash(
            email,
            &evidence_money,
            &actual,
            if checkout.completion_version == 2 {
                checkout.convention_slug.as_deref()
            } else {
                None
            },
        )? != checkout.evidence_hash
        || !tags_match(checkout, &draft.tags)
        || (matches!(
            checkout.correlation_version,
            correlation::METADATA_VERSION | correlation::INDEPENDENT_VERSION
        ) && !correlation::identity_matches(checkout, &draft.custom_attributes))
        || (checkout.correlation_version == correlation::LEGACY_VERSION
            && checkout.completion_version == 2
            && !slug_matches(checkout, &draft.custom_attributes))
    {
        return Err(evidence_conflict(
            "Shopify draft no longer matches the accepted purchase",
        ));
    }
    Ok(money)
}

pub(super) fn payment_money(
    checkout: &Checkout,
    legacy: &CheckoutMoney,
) -> Result<CheckoutMoney, AppError> {
    if checkout.correlation_version == correlation::INDEPENDENT_VERSION {
        correlation::shopify_money(checkout)?
            .ok_or_else(|| evidence_conflict("Checkout Shopify snapshot is missing"))?;
        correlation::payment_money(checkout)?
            .ok_or_else(|| evidence_conflict("Checkout payment snapshot is missing"))
    } else {
        Ok(legacy.clone())
    }
}

pub(super) fn tags_match(checkout: &Checkout, tags: &[String]) -> bool {
    if matches!(
        checkout.correlation_version,
        correlation::METADATA_VERSION | correlation::INDEPENDENT_VERSION
    ) {
        if correlation::Identity::from_checkout(checkout)
            .and_then(|identity| correlation::validate_legacy_identity(&identity, tags))
            .is_err()
        {
            return false;
        }
        return checkout.convention_slug.as_ref().is_some_and(|slug| {
            let expected: String = slug.chars().take(40).collect();
            tags.iter().filter(|tag| **tag == expected).count() == 1
        });
    }
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

pub(super) fn slug_matches(checkout: &Checkout, attributes: &[CustomAttribute]) -> bool {
    let matches: Vec<_> = attributes
        .iter()
        .filter(|attribute| attribute.key == "convention_slug")
        .collect();
    matches.len() == 1 && checkout.convention_slug.as_deref() == Some(matches[0].value.as_str())
}

pub(super) fn exact(minor: i64, currency: &str) -> Result<ExactMoney, AppError> {
    let precision = services::conventions::money::precision(currency)
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

impl PreparedPurchase {
    fn evidence_hash(&self, completion_version: i16) -> Result<Vec<u8>, AppError> {
        let lines = self
            .items
            .iter()
            .map(|item| (item.shopify_variant_id.clone(), i64::from(item.quantity)))
            .collect();
        bound_evidence_hash(
            &self.email,
            &self.money,
            &lines,
            if completion_version == 2 {
                self.slug.as_deref()
            } else {
                None
            },
        )
    }
}

struct PurchaseIdentity {
    id: Uuid,
    convention_id: Uuid,
    completion_version: i16,
    correlation_version: i16,
    provider: String,
    account_id: String,
    environment: String,
    convention_slug: String,
}

fn purchase_record(
    settings: &CheckoutSettings,
    purchase: &PreparedPurchase,
    identity: PurchaseIdentity,
    guest_hash: &[u8],
    fingerprint: Vec<u8>,
    existing: Option<&Checkout>,
) -> Result<Checkout, AppError> {
    let now = Utc::now();
    Ok(Checkout {
        id: identity.id,
        completion_version: identity.completion_version,
        correlation_version: identity.correlation_version,
        convention_slug: purchase.slug.clone(),
        payment_amount_minor: (identity.correlation_version == correlation::INDEPENDENT_VERSION)
            .then_some(purchase.money.minor),
        payment_currency: (identity.correlation_version == correlation::INDEPENDENT_VERSION)
            .then(|| purchase.money.currency.clone()),
        shopify_amount_minor: None,
        shopify_currency: None,
        payment_recording_state: PaymentRecordingState::NotStarted,
        guest_secret_hash: guest_hash.to_vec(),
        owner_expires_at: now
            + Duration::seconds(
                i64::try_from(settings.guest_recovery_secs)
                    .map_err(|_| conflict("Invalid guest recovery duration"))?,
            ),
        request_fingerprint: fingerprint,
        convention_id: identity.convention_id,
        provider: identity.provider,
        provider_account_id: identity.account_id,
        provider_environment: identity.environment,
        evidence_hash: purchase.evidence_hash(identity.completion_version)?,
        required_tag_hashes: purchase
            .tags
            .iter()
            .map(|tag| Sha256::digest(tag.as_bytes()).to_vec())
            .collect(),
        success_url: Some(return_url(
            settings,
            &identity.convention_slug,
            identity.id,
            &settings.return_path_template,
        )),
        cancel_url: Some(return_url(
            settings,
            &identity.convention_slug,
            identity.id,
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
        created_at: existing.map_or(now, |checkout| checkout.created_at),
    })
}

fn reservation_lines(draft: &CompletionDraft) -> Result<BTreeMap<String, i64>, AppError> {
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
    Ok(actual)
}

fn bound_evidence_hash(
    email: &str,
    money: &CheckoutMoney,
    lines: &BTreeMap<String, i64>,
    slug: Option<&str>,
) -> Result<Vec<u8>, AppError> {
    match slug {
        None => evidence_hash(email, money, lines),
        Some(slug) => Ok(Sha256::digest(
            serde_json::to_vec(&(email, money, lines, slug))
                .map_err(|error| AppError::from(anyhow::Error::new(error)))?,
        )
        .to_vec()),
    }
}

fn validate_convention_purchase(
    convention: &crate::models::conventions::Convention,
    request: &CreateCheckoutRequest,
) -> Result<(), AppError> {
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
    Ok(())
}

fn purchase_evidence_money(
    checkout: &Checkout,
    draft: &CompletionDraft,
    money: &CheckoutMoney,
) -> Result<CheckoutMoney, AppError> {
    if checkout.correlation_version == correlation::INDEPENDENT_VERSION {
        let payment = correlation::payment_money(checkout)?
            .ok_or_else(|| evidence_conflict("Checkout payment snapshot is missing"))?;
        if let Some(expected) = correlation::shopify_money(checkout)? {
            if expected != *money {
                return Err(evidence_conflict("Shopify purchase money changed"));
            }
        } else if checkout.payment_state != PaymentState::NotStarted
            || !matches!(
                checkout.draft_state,
                DraftState::Creating | DraftState::Uncertain
            )
        {
            return Err(evidence_conflict("Checkout Shopify snapshot is missing"));
        }
        if draft.custom_attributes.iter().any(|attr| {
            matches!(
                attr.key.as_str(),
                correlation::SHOPIFY_AMOUNT_KEY | correlation::SHOPIFY_CURRENCY_KEY
            )
        }) && correlation::parse_money_snapshots(&draft.custom_attributes)?
            .is_none_or(|snapshots| snapshots.shopify != *money)
        {
            return Err(evidence_conflict(
                "Shopify draft monetary metadata conflicts",
            ));
        }
        Ok(payment)
    } else {
        Ok(money.clone())
    }
}

fn pin_shopify_money(checkout: &mut Checkout, draft: &CompletionDraft) -> Result<(), AppError> {
    if checkout.correlation_version == correlation::INDEPENDENT_VERSION {
        let observed = validate_purchase(checkout, draft)?;
        checkout.shopify_amount_minor = Some(observed.minor);
        checkout.shopify_currency = Some(observed.currency);
    }
    Ok(())
}

async fn find_reserved_draft(
    clients: &CheckoutClients<'_>,
    checkout: &Checkout,
) -> Result<String, AppError> {
    let metadata = matches!(
        checkout.correlation_version,
        correlation::METADATA_VERSION | correlation::INDEPENDENT_VERSION
    );
    let expected_tag = if metadata {
        None
    } else {
        Some(recovery_tag(checkout).ok_or_else(draft_ambiguous)?)
    };
    let query = if metadata {
        draft_recovery_query(checkout)?
    } else {
        format!(
            "tag:'{}'",
            expected_tag.as_deref().ok_or_else(draft_ambiguous)?
        )
    };
    let mut found = Vec::new();
    let mut requested_pages = 0;
    let mut scanned = 0;
    services::pagination::for_each_page(
        None,
        |after| {
            requested_pages += 1;
            let allowed = !metadata || requested_pages <= MAX_RECOVERY_PAGES;
            let query = &query;
            async move {
                if !allowed {
                    return Err(draft_ambiguous());
                }
                clients
                    .shopify
                    .draft_orders(
                        PageRequest { first: 250, after },
                        Some(query),
                        metadata.then_some(crate::models::shopify::DraftOrderSortKey::CreatedAt),
                    )
                    .await
                    .map_err(|error| upstream(error))
            }
        },
        |drafts| {
            scanned += drafts.len();
            if metadata && scanned > MAX_RECOVERY_DRAFTS {
                return Err(draft_ambiguous());
            }
            for draft in drafts {
                let candidate = if metadata {
                    draft.custom_attributes.iter().any(|attribute| {
                        attribute.key == correlation::CHECKOUT_KEY
                            && attribute.value == checkout.id.to_string()
                    })
                } else {
                    expected_tag
                        .as_ref()
                        .is_some_and(|tag| draft.tags.contains(tag))
                };
                if !candidate {
                    continue;
                }
                validate_recovered_draft(checkout, &draft, metadata)?;
                found.push(draft.id.as_ref().to_owned());
                if found.len() > 1 {
                    return Err(draft_ambiguous());
                }
            }
            Ok(())
        },
        |error| upstream(error),
    )
    .await?;
    if found.len() != 1 {
        return Err(draft_ambiguous());
    }
    found.pop().ok_or_else(draft_ambiguous)
}

fn validate_recovered_draft(
    checkout: &Checkout,
    draft: &CompletionDraft,
    metadata: bool,
) -> Result<(), AppError> {
    if metadata {
        let (from, through) = draft_recovery_window(checkout)?;
        if draft
            .created_at
            .is_none_or(|created| created < from || created > through)
        {
            return Err(draft_ambiguous());
        }
    }
    validate_purchase(checkout, draft).map_err(|error| {
        super::recovery::manual(error, crate::models::checkout::RecoveryCode::DraftAmbiguous)
    })?;
    validate_reservation(checkout, draft).map_err(|error| {
        super::recovery::manual(error, crate::models::checkout::RecoveryCode::DraftAmbiguous)
    })
}

fn draft_recovery_window(
    checkout: &Checkout,
) -> Result<(chrono::DateTime<Utc>, chrono::DateTime<Utc>), AppError> {
    let from = checkout
        .created_at
        .checked_sub_signed(Duration::minutes(5))
        .ok_or_else(draft_ambiguous)?;
    let through = checkout
        .created_at
        .checked_add_signed(Duration::minutes(5))
        .ok_or_else(draft_ambiguous)?;
    Ok((from, through))
}

fn draft_recovery_query(checkout: &Checkout) -> Result<String, AppError> {
    let (from, through) = draft_recovery_window(checkout)?;
    Ok(format!(
        "created_at:>='{}' created_at:<='{}'",
        from.to_rfc3339(),
        through.to_rfc3339()
    ))
}

fn draft_ambiguous() -> AppError {
    super::recovery::manual(
        conflict("Shopify draft creation requires reconciliation"),
        crate::models::checkout::RecoveryCode::DraftAmbiguous,
    )
}

fn purchase_tags(
    convention_id: Uuid,
    convention_name: &str,
    checkout_id: Uuid,
    slug: Option<&str>,
) -> Vec<String> {
    match slug {
        Some(slug) => vec![slug.chars().take(40).collect()],
        None => legacy_purchase_tags(convention_id, convention_name, checkout_id, None),
    }
}

fn legacy_purchase_tags(
    convention_id: Uuid,
    convention_name: &str,
    checkout_id: Uuid,
    slug: Option<&str>,
) -> Vec<String> {
    let mut tags = vec![
        format!("cv:{convention_id}"),
        if slug.is_some() {
            format!(
                "name:{}",
                convention_name.chars().take(35).collect::<String>()
            )
        } else {
            convention_name.chars().take(40).collect()
        },
        format!("co:{checkout_id}"),
    ];
    if let Some(slug) = slug {
        tags.push(if slug.chars().count() <= 35 {
            format!("slug:{slug}")
        } else {
            use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
            format!(
                "slug:{}",
                URL_SAFE_NO_PAD.encode(&Sha256::digest(slug.as_bytes())[..24])
            )
        });
    }
    tags
}

fn recovery_tag(checkout: &Checkout) -> Option<String> {
    ["co", "checkout"]
        .into_iter()
        .map(|prefix| format!("{prefix}:{}", checkout.id))
        .find(|tag| {
            let hash = Sha256::digest(tag.as_bytes());
            checkout
                .required_tag_hashes
                .iter()
                .any(|expected| hash.as_slice() == expected.as_slice())
        })
}

async fn provider_identity(
    settings: &CheckoutSettings,
    clients: &CheckoutClients<'_>,
    provider: CheckoutProvider,
    existing: Option<&Checkout>,
) -> Result<(String, String), AppError> {
    let identity = match provider {
        CheckoutProvider::Stripe => (
            clients
                .stripe
                .ok_or_else(unavailable)?
                .get_account()
                .await
                .map_err(|error| upstream(error))?
                .id,
            settings.stripe_livemode.to_string(),
        ),
        CheckoutProvider::Paypal => (
            settings
                .paypal_merchant_id
                .clone()
                .filter(|value| !value.is_empty())
                .ok_or_else(unavailable)?,
            clients.paypal.ok_or_else(unavailable)?.api_origin(),
        ),
    };
    if existing.is_some_and(|checkout| {
        checkout.provider_account_id != identity.0 || checkout.provider_environment != identity.1
    }) {
        return Err(conflict("Checkout provider account or environment changed"));
    }
    Ok(identity)
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
        let canonical = services::conventions::money::validate(
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
        crate::validations::inventories::parse_location_id(&settings.shopify_location_id)
            .ok_or_else(|| conflict("Invalid Shopify checkout location"))?;
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
    services::pagination::for_each_page(
        None,
        |after| async move {
            let page = services::catalogue::products(
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

#[cfg(test)]
#[path = "../../tests/services/checkout/purchase.rs"]
mod tests;
