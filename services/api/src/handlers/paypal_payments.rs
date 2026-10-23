use crate::{
    error::AppError,
    models::paypal::VerifyWebhookSignatureRequest,
    services::paypal_payments::{self, Gateway},
    state::{AppState, required_client},
    validations::paypal_payments as validate,
};
use axum::{
    Json,
    body::Bytes,
    extract::{
        Path, State,
        rejection::{BytesRejection, JsonRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
};
use domain::dto::{CreatePaypalPaymentRequest, PaypalPaymentResponse};
use uuid::Uuid;

pub(crate) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<CreatePaypalPaymentRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<PaypalPaymentResponse>), AppError> {
    let digest = capability(&headers, true)?;
    let gateway = gateway(&state)?;
    let Json(request) = payload.map_err(|_| bad_request("Invalid PayPal payment request"))?;
    let input = validate::validate_create(&request, &gateway.policy.currencies)?;
    let (status, payment) = paypal_payments::create(&state.pool, &gateway, input, digest).await?;
    Ok((status, Json(payment)))
}

pub(crate) async fn capture(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<PaypalPaymentResponse>), AppError> {
    let id = payment_id(path)?;
    let digest = capability(&headers, false)?;
    if !body
        .map_err(|_| bad_request("Invalid capture body"))?
        .is_empty()
    {
        return Err(bad_request("Capture request must have an empty body"));
    }
    let gateway = gateway(&state)?;
    let (status, payment) = paypal_payments::capture(&state.pool, &gateway, id, digest).await?;
    Ok((status, Json(payment)))
}

pub(crate) async fn get(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
) -> Result<Json<PaypalPaymentResponse>, AppError> {
    let id = payment_id(path)?;
    let digest = capability(&headers, false)?;
    let gateway = gateway(&state)?;
    Ok(Json(
        paypal_payments::get(&state.pool, &gateway, id, digest).await?,
    ))
}

pub(crate) async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<StatusCode, AppError> {
    let body = body.map_err(|_| bad_request("Invalid PayPal webhook body"))?;
    if body.is_empty() || body.len() > 262_144 {
        return Err(bad_request("Invalid PayPal webhook body"));
    }
    let event =
        serde_json::from_slice(&body).map_err(|_| bad_request("Invalid PayPal webhook JSON"))?;
    let auth_algo = transmission_header(&headers, "paypal-auth-algo")?;
    let cert_url = transmission_header(&headers, "paypal-cert-url")?;
    let transmission_id = transmission_header(&headers, "paypal-transmission-id")?;
    let transmission_sig = transmission_header(&headers, "paypal-transmission-sig")?;
    let transmission_time = transmission_header(&headers, "paypal-transmission-time")?;
    chrono::DateTime::parse_from_rfc3339(&transmission_time)
        .map_err(|_| bad_request("Invalid PayPal transmission time"))?;
    let url = reqwest::Url::parse(&cert_url)
        .map_err(|_| bad_request("Invalid PayPal certificate URL"))?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err(bad_request("Invalid PayPal certificate URL"));
    }
    let gateway = gateway(&state)?;
    let request = VerifyWebhookSignatureRequest {
        auth_algo,
        cert_url,
        transmission_id,
        transmission_sig,
        transmission_time,
        webhook_id: gateway.policy.webhook_id.clone(),
        webhook_event: event,
    };
    paypal_payments::receive_webhook(&state.pool, &gateway, request).await?;
    Ok(StatusCode::OK)
}

fn gateway(state: &AppState) -> Result<Gateway<'_>, AppError> {
    let client = required_client(&state.paypal, "PayPal client unavailable")?;
    let config = state
        .settings
        .client
        .as_ref()
        .and_then(|client| client.paypal.as_ref())
        .ok_or_else(|| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "PayPal payments unavailable",
            )
        })?;
    Gateway::new(client, config, state.settings.browser.insecure_local)
}
fn capability(headers: &HeaderMap, create: bool) -> Result<[u8; 32], AppError> {
    let failure = || {
        if create {
            AppError::new(StatusCode::UNAUTHORIZED, "Invalid payment capability")
                .with_bearer_challenge()
        } else {
            AppError::new(StatusCode::NOT_FOUND, "Payment not found")
        }
    };
    if headers.get_all(AUTHORIZATION).iter().count() != 1 {
        return Err(failure());
    }
    let value = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(failure)?;
    validate::capability_digest(value).map_err(|_| failure())
}
fn payment_id(path: Result<Path<String>, PathRejection>) -> Result<Uuid, AppError> {
    let Path(id) = path.map_err(|_| bad_request("Invalid payment ID"))?;
    Uuid::parse_str(&id).map_err(|_| bad_request("Invalid payment ID"))
}
fn transmission_header(headers: &HeaderMap, name: &'static str) -> Result<String, AppError> {
    if headers.get_all(name).iter().count() != 1 {
        return Err(bad_request("Missing or invalid PayPal transmission header"));
    }
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
        })
        .map(str::to_owned)
        .ok_or_else(|| bad_request("Missing or invalid PayPal transmission header"))
}
fn bad_request(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}
