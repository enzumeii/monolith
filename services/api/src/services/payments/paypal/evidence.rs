use super::ExpectedPayment;
use crate::models::paypal::payment::exact_money;
use crate::{
    client::ClientError,
    models::paypal::{Capture, Order, PaymentMoney},
};

pub(super) fn validate_order(expected: &ExpectedPayment, order: &Order) -> Result<(), ClientError> {
    if order.id != expected.order_id || order.purchase_units.len() != 1 {
        return Err(mismatch());
    }
    let unit = &order.purchase_units[0];
    if unit.reference_id.as_deref() != Some(expected.reference_id.as_str())
        || unit.custom_id.as_deref() != Some(expected.custom_id.as_str())
        || !money_matches(expected, &unit.amount)
        || unit
            .payee
            .as_ref()
            .is_none_or(|payee| payee.merchant_id != expected.merchant_id)
    {
        return Err(mismatch());
    }
    Ok(())
}

pub(super) fn validate_capture_binding(
    expected: &ExpectedPayment,
    embedded: &Capture,
    capture: &Capture,
) -> Result<(), ClientError> {
    if capture.id != embedded.id || capture.status != embedded.status {
        return Err(mismatch());
    }
    for observed in [embedded, capture] {
        if !money_matches(expected, &observed.amount)
            || observed
                .payee
                .as_ref()
                .is_some_and(|payee| payee.merchant_id != expected.merchant_id)
            || observed
                .custom_id
                .as_deref()
                .is_some_and(|id| id != expected.custom_id)
            || observed
                .supplementary_data
                .as_ref()
                .and_then(|data| data.related_ids.order_id.as_deref())
                .is_some_and(|id| id != expected.order_id)
            || observed
                .supplementary_data
                .as_ref()
                .and_then(|data| data.related_ids.capture_id.as_deref())
                .is_some_and(|id| id != capture.id)
        {
            return Err(mismatch());
        }
    }
    Ok(())
}

#[track_caller]
pub(super) fn mismatch() -> ClientError {
    super::super::evidence_mismatch()
}

fn money_matches(expected: &ExpectedPayment, amount: &PaymentMoney) -> bool {
    amount.currency_code == expected.currency
        && exact_money(&amount.value, &amount.currency_code)
            .is_ok_and(|minor_units| minor_units == expected.amount)
}
