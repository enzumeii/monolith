use crate::{
    error::AppError, models::checkout::WebhookInput, services, state::AppState, transformers,
    validations,
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
use domain::dto::checkout::{CheckoutResponse, CreateCheckoutRequest};
use serde_json::{Value, json};

pub(crate) async fn create(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    body: Result<Json<CreateCheckoutRequest>, JsonRejection>,
) -> Result<Json<CheckoutResponse>, AppError> {
    validations::checkout::no_query(uri.query())?;
    let guest_hash = validations::checkout::credential(&headers)?;
    let Json(request) = body.map_err(|error| validations::checkout::body_error(error.status()))?;
    validations::checkout::create(&request)?;
    super::dependencies::checkout_provider(&state, request.provider)?;
    let clients = super::dependencies::checkout_clients(&state)?;
    let config = super::dependencies::checkout_settings(&state)?;
    Ok(Json(
        services::checkout::create(&state.pool, config, &clients, &request, &guest_hash).await?,
    ))
}

pub(crate) async fn verify(
    State(state): State<AppState>,
    Path(checkout_id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<CheckoutResponse>, AppError> {
    validations::checkout::no_query(uri.query())?;
    let credential = validations::checkout::credential(&headers)?;
    let body = validations::checkout::bytes(body)?;
    if !body.is_empty() {
        return Err(validations::checkout::invalid(
            "Checkout verification does not accept a request body",
        ));
    }
    let id = validations::checkout::id(&checkout_id)?;
    let config = super::dependencies::checkout_settings(&state)?;
    let clients = super::dependencies::checkout_clients(&state)?;
    Ok(Json(
        services::checkout::verify(&state.pool, config, &clients, id, &credential).await?,
    ))
}

pub(crate) async fn webhook(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    raw: Result<Bytes, BytesRejection>,
) -> Result<Json<Value>, AppError> {
    validations::checkout::no_query(uri.query())?;
    let raw = validations::checkout::bytes(raw)?;
    let config = super::dependencies::checkout_settings(&state)?;
    let input = match provider.as_str() {
        "stripe" => WebhookInput::Stripe {
            raw: &raw,
            authentication: validations::checkout::stripe_authentication(config, &headers)?,
        },
        "paypal" => transformers::checkout::paypal_webhook(
            validations::checkout::paypal_authentication(config, &headers, &raw)?,
        ),
        _ => {
            return Err(validations::checkout::invalid(
                "Unsupported webhook provider",
            ));
        }
    };
    services::checkout::webhooks::receive(
        &state.pool,
        config,
        super::dependencies::checkout_webhook_clients(&state),
        input,
    )
    .await?;
    Ok(Json(json!({"received":true})))
}
