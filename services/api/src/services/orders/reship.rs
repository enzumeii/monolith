use super::super::{
    common::{resend_client, shopify_client, upstream},
    pagination::for_each_page,
};
use crate::{
    client::shopify::TagAction,
    error::AppError,
    models::{fulfillment::ReshipOrder, shopify::PageRequest},
    state::AppState,
};
use axum::http::StatusCode;
use domain::dto::orders::ReshipOrderResponse;

pub async fn reship(state: &AppState, number: u16) -> Result<ReshipOrderResponse, AppError> {
    let shopify = shopify_client(state)?;
    let resend = resend_client(state)?;
    let mut found: Option<ReshipOrder> = None;
    for_each_page(
        None,
        async |after| {
            shopify
                .reship_order(number, &PageRequest { first: 250, after })
                .await
                .map_err(upstream)
        },
        |orders| {
            for order in orders {
                if order.number != i64::from(number) {
                    continue;
                }
                if found
                    .as_ref()
                    .is_some_and(|previous| previous.id != order.id)
                {
                    return Err(AppError::new(
                        StatusCode::CONFLICT,
                        "Order number is ambiguous",
                    ));
                }
                found = Some(order);
            }
            Ok(())
        },
        upstream,
    )
    .await?;
    let order = found.ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Order not found"))?;
    let email = order
        .email
        .as_deref()
        .filter(|email| !email.trim().is_empty())
        .ok_or_else(|| AppError::new(StatusCode::CONFLICT, "Order email missing"))?;
    shopify
        .edit_order_tags(&order.id, &["RETURNED"], TagAction::Add)
        .await
        .map_err(upstream)?;
    let (subject, html) = notice(order.number);
    let email_id = resend
        .send_email(None, &[email], &subject, &html)
        .await
        .map_err(upstream)?;
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
