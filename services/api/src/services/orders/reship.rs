use crate::{
    client::shopify::TagAction,
    error::{AppError, upstream},
};
use axum::http::StatusCode;
use domain::dto::orders::ReshipOrderResponse;

pub async fn reship(
    shopify: &crate::client::shopify::ShopifyClient,
    resend: &crate::client::resend::ResendClient,
    number: u16,
) -> Result<ReshipOrderResponse, AppError> {
    let order = shopify
        .get_order_by_order_number(number)
        .await
        .map_err(|error| upstream(error))?
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Order not found"))?;
    let email = order
        .email
        .as_deref()
        .filter(|email| !email.trim().is_empty())
        .ok_or_else(|| AppError::new(StatusCode::CONFLICT, "Order email missing"))?;
    let (subject, html) = notice(order.number);
    let email_id = resend
        .send_email(None, &[email], &subject, &html)
        .await
        .map_err(|error| upstream(error))?;
    super::edit_order_tags(shopify, &order.id, &["RETURNED"], TagAction::Add).await?;
    Ok(ReshipOrderResponse {
        success: true,
        email_id: email_id.as_ref().to_owned(),
    })
}

fn notice(number: i64) -> (String, String) {
    (
        format!("[ENZUMEII] Urgent : Reshipping your returned package order #{number}"),
        include_str!("../../../assets/reship_notice.html")
            .replace("{order_no}", &number.to_string()),
    )
}
