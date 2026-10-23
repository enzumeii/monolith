use crate::{
    config::CheckoutSettings,
    error::AppError,
    services::checkout::{self as service, CheckoutClients},
    state::AppState,
    validations::checkout as validate,
};
use axum::{
    Json,
    body::Bytes,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, Uri},
};
use domain::dto::checkout::{CheckoutProvider, CheckoutResponse, CreateCheckoutRequest};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, time::Duration};

pub(crate) async fn create(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    uri: Uri,
    headers: HeaderMap,
    body: Result<Json<CreateCheckoutRequest>, JsonRejection>,
) -> Result<Json<CheckoutResponse>, AppError> {
    validate::no_query(uri.query())?;
    let guest_hash = validate::credential(&headers)?;
    let Json(request) = body.map_err(|_| validate::invalid("Invalid checkout request body"))?;
    validate::create(&request)?;
    match request.provider {
        CheckoutProvider::Stripe => {
            super::dependencies::stripe_client(&state)?;
        }
        CheckoutProvider::Paypal => {
            super::dependencies::paypal_client(&state)?;
        }
    }
    let clients = clients(&state)?;
    let source_hash = Sha256::digest(peer.ip().to_string().as_bytes());
    Ok(Json(
        service::create(
            &state.pool,
            settings(&state)?,
            &clients,
            &request,
            &guest_hash,
            &source_hash,
        )
        .await?,
    ))
}

pub(crate) async fn verify(
    State(state): State<AppState>,
    Path(checkout_id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<CheckoutResponse>, AppError> {
    validate::no_query(uri.query())?;
    let credential = validate::credential(&headers)?;
    if !body.is_empty() {
        return Err(validate::invalid(
            "Checkout verification does not accept a request body",
        ));
    }
    let id = validate::id(&checkout_id)?;
    Ok(Json(
        service::verify(
            &state.pool,
            settings(&state)?,
            &clients(&state)?,
            id,
            &credential,
        )
        .await?,
    ))
}

pub(crate) async fn webhook(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    raw: Bytes,
) -> Result<Json<Value>, AppError> {
    validate::no_query(uri.query())?;
    let config = settings(&state)?;
    let notification = match provider.as_str() {
        "stripe" => stripe_event(config, &headers, &raw)?,
        "paypal" => paypal_event(&state, config, &headers, &raw).await?,
        _ => return Err(validate::invalid("Unsupported webhook provider")),
    };
    let Some((provider, event_id, resource, correlation)) = notification else {
        return Ok(Json(json!({"received": true})));
    };
    service::webhook(
        &state.pool,
        config,
        &clients(&state)?,
        provider,
        &event_id,
        resource.as_deref(),
        correlation,
    )
    .await?;
    Ok(Json(json!({"received":true})))
}

type Notification = (CheckoutProvider, String, Option<String>, Option<uuid::Uuid>);

fn stripe_event(
    config: &CheckoutSettings,
    headers: &HeaderMap,
    raw: &[u8],
) -> Result<Option<Notification>, AppError> {
    use crate::client::stripe;

    if headers.contains_key("paypal-transmission-id") {
        return Err(validate::invalid("Ambiguous webhook authentication"));
    }
    let secret = config
        .stripe_webhook_secret
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| unavailable("Stripe webhook configuration unavailable"))?;
    let event = stripe::webhook::verify_event(
        raw,
        header(headers, "stripe-signature")?,
        secret,
        chrono::Utc::now().timestamp(),
        Duration::from_secs(300),
    )
    .map_err(|_| validate::invalid("Invalid Stripe webhook authentication or event"))?;
    if event.livemode != config.stripe_livemode
        || event
            .account
            .as_deref()
            .is_some_and(|account| account != config.stripe_expected_account)
    {
        return Err(validate::invalid(
            "Stripe webhook account or environment mismatch",
        ));
    }
    if matches!(event.event_type, stripe::webhook::EventType::Other(_)) {
        return Ok(None);
    }
    let value: Value = serde_json::from_slice(raw)
        .map_err(|_| validate::invalid("Invalid Stripe webhook event"))?;
    let correlation = value
        .pointer("/data/object/metadata/checkout_id")
        .and_then(Value::as_str)
        .and_then(|value| uuid::Uuid::parse_str(value).ok());
    Ok(Some((
        CheckoutProvider::Stripe,
        event.id,
        event
            .object
            .id
            .starts_with("cs_")
            .then_some(event.object.id),
        correlation,
    )))
}

async fn paypal_event(
    state: &AppState,
    config: &CheckoutSettings,
    headers: &HeaderMap,
    raw: &[u8],
) -> Result<Option<Notification>, AppError> {
    use crate::client::paypal;

    if headers.contains_key("stripe-signature") {
        return Err(validate::invalid("Ambiguous webhook authentication"));
    }
    if raw.len() > 262_144 {
        return Err(AppError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "PayPal webhook payload exceeds supported limit",
        ));
    }
    let webhook_id = config
        .paypal_webhook_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| unavailable("PayPal webhook configuration unavailable"))?;
    let value: Value = serde_json::from_slice(raw)
        .map_err(|_| validate::invalid("Invalid PayPal webhook event"))?;
    let request = crate::models::paypal::VerifyWebhookSignatureRequest {
        auth_algo: header(headers, "paypal-auth-algo")?.into(),
        cert_url: header(headers, "paypal-cert-url")?.into(),
        transmission_id: header(headers, "paypal-transmission-id")?.into(),
        transmission_sig: header(headers, "paypal-transmission-sig")?.into(),
        transmission_time: header(headers, "paypal-transmission-time")?.into(),
        webhook_id: webhook_id.into(),
        webhook_event: value.clone(),
    };
    let event = paypal::webhook::verify_event(super::dependencies::paypal_client(state)?, &request)
        .await
        .map_err(|error| match error {
            paypal::webhook::WebhookError::Upstream(error) => crate::error::upstream(error),
            _ => validate::invalid("Invalid PayPal webhook authentication or event"),
        })?;
    if matches!(
        event.event_type,
        crate::models::paypal::WebhookEventType::Other(_)
    ) {
        return Ok(None);
    }
    let correlation = value
        .pointer("/resource/custom_id")
        .or_else(|| value.pointer("/resource/purchase_units/0/custom_id"))
        .and_then(Value::as_str)
        .and_then(|value| uuid::Uuid::parse_str(value).ok());
    Ok(Some((
        CheckoutProvider::Paypal,
        event.id,
        event.order_id,
        correlation,
    )))
}

fn settings(state: &AppState) -> Result<&CheckoutSettings, AppError> {
    state
        .settings
        .checkout
        .as_ref()
        .ok_or_else(|| unavailable("Checkout configuration unavailable"))
}

fn clients(state: &AppState) -> Result<CheckoutClients<'_>, AppError> {
    Ok(CheckoutClients {
        shopify: super::dependencies::shopify_client(state)?,
        stripe: state
            .stripe
            .as_ref()
            .ok()
            .and_then(|value| value.as_deref()),
        paypal: state
            .paypal
            .as_ref()
            .ok()
            .and_then(|value| value.as_deref()),
    })
}

fn header<'a>(headers: &'a HeaderMap, name: &'static str) -> Result<&'a str, AppError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| validate::invalid("Missing or invalid webhook authentication header"))
}

fn unavailable(message: &'static str) -> AppError {
    AppError::new(StatusCode::SERVICE_UNAVAILABLE, message)
}
