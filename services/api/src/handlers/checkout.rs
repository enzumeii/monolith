use crate::{
    error::AppError,
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
    http::{HeaderMap, Uri},
};
use domain::dto::checkout::{CheckoutProvider, CheckoutResponse, CreateCheckoutRequest};
use serde_json::{Value, json};

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
    let clients = super::dependencies::checkout_clients(&state)?;
    Ok(Json(
        service::create(
            &state.pool,
            super::dependencies::checkout_settings(&state)?,
            &clients,
            &request,
            &guest_hash,
        )
        .await?,
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
    Ok(Json(
        service::verify(
            &state.pool,
            super::dependencies::checkout_settings(&state)?,
            &super::dependencies::checkout_clients(&state)?,
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
    raw: Result<Bytes, BytesRejection>,
) -> Result<Json<Value>, AppError> {
    validate::no_query(uri.query())?;
    let raw = validate::bytes(raw)?;
    let config = super::dependencies::checkout_settings(&state)?;
    let notification = match provider.as_str() {
        "stripe" => notification::stripe(
            config,
            validate::stripe_authentication(config, &headers)?,
            &raw,
        )?,
        "paypal" => {
            let request = validate::paypal_authentication(config, &headers, &raw)?;
            notification::paypal(super::dependencies::paypal_client(&state)?, &request).await?
        }
        _ => return Err(validate::invalid("Unsupported webhook provider")),
    };
    let Some(notification) = notification else {
        return Ok(Json(json!({"received": true})));
    };
    service::webhook(
        &state.pool,
        config,
        &super::dependencies::checkout_clients(&state)?,
        notification.provider,
        notification.resource_id.as_deref(),
        notification.correlation_id,
    )
    .await?;
    Ok(Json(json!({"received":true})))
}
