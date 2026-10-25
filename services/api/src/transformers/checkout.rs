use crate::{
    models::checkout::{Checkout, PaypalAuthentication, WebhookInput},
    models::paypal::VerifyWebhookSignatureRequest,
};
use chrono::SubsecRound;
use domain::dto::checkout::{
    CheckoutMoney, CheckoutNextAction, CheckoutOrder, CheckoutResponse, CheckoutStatus,
};

pub(crate) fn paypal_webhook(checked: PaypalAuthentication) -> WebhookInput<'static> {
    WebhookInput::Paypal(VerifyWebhookSignatureRequest {
        auth_algo: checked.auth_algo,
        cert_url: checked.cert_url,
        transmission_id: checked.transmission_id,
        transmission_sig: checked.transmission_sig,
        transmission_time: checked.transmission_time,
        webhook_id: checked.webhook_id,
        webhook_event: checked.webhook_event,
    })
}

pub(crate) fn response(
    checkout: &Checkout,
    status: CheckoutStatus,
    money: CheckoutMoney,
    next_action: Option<CheckoutNextAction>,
    order: Option<CheckoutOrder>,
) -> CheckoutResponse {
    CheckoutResponse {
        checkout_id: checkout.id.to_string(),
        status,
        money,
        reservation_expires_at: checkout
            .reservation_expires_at
            .map(|value| value.trunc_subsecs(6).to_rfc3339()),
        next_action,
        order,
    }
}
