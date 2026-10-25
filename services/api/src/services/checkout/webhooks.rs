use super::CheckoutClients;
use crate::{
    client::{self, paypal::PaypalClient, stripe::StripeClient},
    config::CheckoutSettings,
    error::{AppError, upstream},
    models::checkout::WebhookInput,
    models::paypal::webhook::WebhookError as PaypalWebhookError,
    services,
};
use axum::http::StatusCode;
use sqlx::PgPool;
use std::time::Duration;

pub(crate) struct WebhookClients<'a> {
    pub stripe: Result<&'a StripeClient, AppError>,
    pub paypal: Result<&'a PaypalClient, AppError>,
    pub advancement: Result<CheckoutClients<'a>, AppError>,
}

pub(crate) async fn receive(
    pool: &PgPool,
    settings: &CheckoutSettings,
    clients: WebhookClients<'_>,
    input: WebhookInput<'_>,
) -> Result<(), AppError> {
    let notification = match input {
        WebhookInput::Stripe {
            raw,
            authentication,
        } => {
            let event = client::stripe::webhook::verify_event(
                raw,
                authentication.signature,
                authentication.secret,
                chrono::Utc::now().timestamp(),
                Duration::from_secs(300),
            )
            .map_err(|_| invalid("Invalid Stripe webhook authentication or event"))?;
            let account = if event.account.is_some() && event.livemode == settings.stripe_livemode {
                Some(
                    clients
                        .stripe?
                        .get_account()
                        .await
                        .map_err(|error| upstream(error))?,
                )
            } else {
                None
            };
            services::checkout::notification::stripe(
                settings,
                event,
                account.as_ref().map(|account| account.id.as_str()),
            )?
        }
        WebhookInput::Paypal(request) => {
            let event = client::paypal::webhook::verify_event(clients.paypal?, &request)
                .await
                .map_err(|error| match error {
                    PaypalWebhookError::Upstream(error) => upstream(error),
                    _ => invalid("Invalid PayPal webhook authentication or event"),
                })?;
            services::checkout::notification::paypal(event)
        }
    };
    let Some(notification) = notification else {
        return Ok(());
    };
    let clients = clients.advancement?;
    services::checkout::webhook(
        pool,
        settings,
        &clients,
        notification.provider,
        notification.resource_id.as_deref(),
        notification.correlation_id,
    )
    .await
}

fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

#[cfg(test)]
#[path = "../../tests/services/checkout/webhooks.rs"]
mod tests;
