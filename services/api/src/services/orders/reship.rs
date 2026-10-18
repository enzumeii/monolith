use super::{
    super::common::{resend_client, shopify_client, upstream},
    workflow::ensure_same_order,
};
use crate::{
    client::{ClientError, ClientErrorKind},
    error::AppError,
    state::AppState,
};
use axum::http::StatusCode;
use domain::dto::orders::ReshipOrderResponse;
use std::time::Duration;

pub async fn reship(state: &AppState, id: String) -> Result<ReshipOrderResponse, AppError> {
    let shopify = shopify_client(state)?;
    let resend = resend_client(state)?;
    let timeout = state
        .settings
        .reship
        .as_ref()
        .filter(|settings| settings.email_timeout_secs > 0)
        .ok_or_else(|| {
            AppError::new(StatusCode::SERVICE_UNAVAILABLE, "Reship email unavailable")
        })?;
    let order = shopify
        .reship_order(&id)
        .await
        .map_err(|error| upstream(error))?
        .order
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Order not found"))?;
    ensure_same_order(&id, &order.id).map_err(|error| upstream(error))?;
    if order.number <= 0 {
        return Err(upstream(ClientError::new(ClientErrorKind::Protocol(
            "invalid order number",
        ))));
    }
    let email = order
        .email
        .as_deref()
        .filter(|email| !email.trim().is_empty())
        .ok_or_else(|| AppError::new(StatusCode::CONFLICT, "Order email missing"))?;
    let tagged = shopify
        .add_order_tags(&id, &["RETURNED"])
        .await
        .map_err(|error| upstream(error))?;
    ensure_same_order(&id, &tagged).map_err(|error| upstream(error))?;
    let (subject, html) = notice(order.number);
    let email_id = tokio::time::timeout(
        Duration::from_secs(timeout.email_timeout_secs),
        resend.send_email(None, &[email], &subject, &html),
    )
    .await
    .map_err(|error| {
        AppError::new(StatusCode::GATEWAY_TIMEOUT, "Upstream request failed").with_source(error)
    })?
    .map_err(|error| upstream(error))?;
    if email_id.as_ref().trim().is_empty() {
        return Err(upstream(ClientError::new(ClientErrorKind::Protocol(
            "email result missing",
        ))));
    }
    Ok(ReshipOrderResponse { success: true })
}

fn notice(number: i64) -> (String, String) {
    (
        format!("[ENZUMEII] Urgent : Reshipping your returned package order #{number}"),
        include_str!("reship_notice.html").replace("{order_no}", &number.to_string()),
    )
}
