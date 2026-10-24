use crate::{
    client::{paypal, stripe},
    error::AppError,
    models::paypal::webhook::WebhookError as PaypalWebhookError,
    services::checkout::{self as service, notification},
    state::AppState,
    validations::checkout as validate,
};
use axum::{
    Json,
    body::Bytes,
    extract::{
        Path, State,
        rejection::{BytesRejection, JsonRejection},
    },
    http::{HeaderMap, StatusCode, Uri},
};
use domain::dto::checkout::{CheckoutProvider, CheckoutResponse, CreateCheckoutRequest};
use serde_json::{Value, json};
use std::time::Duration;

pub(crate) async fn create(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Result<Json<CreateCheckoutRequest>, JsonRejection>,
) -> Result<Json<CheckoutResponse>, AppError> {
    validate::no_query(uri.query())?;
    let guest_hash = validate::credential(&headers)?;
    let Json(request) = body.map_err(|error| validate::body_error(error.status()))?;
    validate::create(&request)?;
    match request.provider {
        CheckoutProvider::Stripe => {
            super::dependencies::stripe_client(&state)?;
        }
        CheckoutProvider::Paypal => {
            super::dependencies::paypal_client(&state)?;
        }
    }
    let clients = service::CheckoutClients {
        shopify: super::dependencies::shopify_client(&state)?,
        stripe: state
            .stripe
            .as_ref()
            .ok()
            .and_then(|client| client.as_deref()),
        paypal: state
            .paypal
            .as_ref()
            .ok()
            .and_then(|client| client.as_deref()),
    };
    let config = state.settings.checkout.as_ref().ok_or_else(|| {
        AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Checkout configuration unavailable",
        )
    })?;
    Ok(Json(
        service::create(&state.pool, config, &clients, &request, &guest_hash).await?,
    ))
}

pub(crate) async fn verify(
    State(state): State<AppState>,
    Path(checkout_id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<CheckoutResponse>, AppError> {
    validate::no_query(uri.query())?;
    let credential = validate::credential(&headers)?;
    let body = validate::bytes(body)?;
    if !body.is_empty() {
        return Err(validate::invalid(
            "Checkout verification does not accept a request body",
        ));
    }
    let id = validate::id(&checkout_id)?;
    let config = state.settings.checkout.as_ref().ok_or_else(|| {
        AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Checkout configuration unavailable",
        )
    })?;
    let clients = service::CheckoutClients {
        shopify: super::dependencies::shopify_client(&state)?,
        stripe: state
            .stripe
            .as_ref()
            .ok()
            .and_then(|client| client.as_deref()),
        paypal: state
            .paypal
            .as_ref()
            .ok()
            .and_then(|client| client.as_deref()),
    };
    Ok(Json(
        service::verify(&state.pool, config, &clients, id, &credential).await?,
    ))
}

pub(crate) async fn webhook(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    raw: Result<Bytes, BytesRejection>,
) -> Result<Json<Value>, AppError> {
    validate::no_query(uri.query())?;
    let raw = validate::bytes(raw)?;
    let config = state.settings.checkout.as_ref().ok_or_else(|| {
        AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Checkout configuration unavailable",
        )
    })?;
    let notification = match provider.as_str() {
        "stripe" => {
            let authentication = validate::stripe_authentication(config, &headers)?;
            let event = stripe::webhook::verify_event(
                &raw,
                authentication.signature,
                authentication.secret,
                chrono::Utc::now().timestamp(),
                Duration::from_secs(300),
            )
            .map_err(|_| validate::invalid("Invalid Stripe webhook authentication or event"))?;
            notification::stripe(config, event)?
        }
        "paypal" => {
            let request = validate::paypal_authentication(config, &headers, &raw)?;
            let event = paypal::webhook::verify_event(
                super::dependencies::paypal_client(&state)?,
                &request,
            )
            .await
            .map_err(|error| match error {
                PaypalWebhookError::Upstream(error) => crate::error::upstream(error),
                _ => validate::invalid("Invalid PayPal webhook authentication or event"),
            })?;
            notification::paypal(event)
        }
        _ => return Err(validate::invalid("Unsupported webhook provider")),
    };
    let Some(notification) = notification else {
        return Ok(Json(json!({"received": true})));
    };
    let clients = service::CheckoutClients {
        shopify: super::dependencies::shopify_client(&state)?,
        stripe: state
            .stripe
            .as_ref()
            .ok()
            .and_then(|client| client.as_deref()),
        paypal: state
            .paypal
            .as_ref()
            .ok()
            .and_then(|client| client.as_deref()),
    };
    service::webhook(
        &state.pool,
        config,
        &clients,
        notification.provider,
        notification.resource_id.as_deref(),
        notification.correlation_id,
    )
    .await?;
    Ok(Json(json!({"received":true})))
}
