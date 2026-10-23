use crate::{
    error::AppError, models::checkout_completion::CompletionRow,
    repositories::checkout_completions, state::AppState,
};
use axum::{
    body::Bytes,
    extract::{
        Path, State,
        rejection::{BytesRejection, PathRejection},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub(crate) async fn verify_payment(
    State(state): State<AppState>,
    checkout_id: Result<Path<String>, PathRejection>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, AppError> {
    let token = guest_bearer(&headers)?;
    let Path(checkout_id) =
        checkout_id.map_err(|_| AppError::new(StatusCode::BAD_REQUEST, "Invalid checkout ID"))?;
    let id = Uuid::parse_str(&checkout_id)
        .map_err(|_| AppError::new(StatusCode::BAD_REQUEST, "Invalid checkout ID"))?;
    let body = body
        .map_err(|_| AppError::new(StatusCode::BAD_REQUEST, "Verification body must be empty"))?;
    if !body.is_empty() {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "Verification body must be empty",
        ));
    }
    let row = authorize(&state.pool, id, token).await?;
    let outcome = if let Some(outcome) = crate::services::checkout_verification::cached(&row)? {
        outcome
    } else {
        let shopify = super::dependencies::shopify_client(&state)?;
        let (stripe, paypal) = if matches!(
            row.trusted_payment,
            Some(crate::models::checkout_completion::PaymentFact::Paid { .. })
        ) {
            (None, None)
        } else {
            match row.attempt.provider {
                crate::models::checkout_completion::Provider::Stripe => {
                    (Some(super::dependencies::stripe_client(&state)?), None)
                }
                crate::models::checkout_completion::Provider::Paypal => {
                    (None, Some(super::dependencies::paypal_client(&state)?))
                }
            }
        };
        crate::services::checkout_verification::verify(&state.pool, shopify, stripe, paypal, &row)
            .await?
    };
    Ok(response(outcome))
}
async fn authorize(pool: &sqlx::PgPool, id: Uuid, token: &str) -> Result<CompletionRow, AppError> {
    let row = checkout_completions::find(pool, id).await?;
    let hash = Sha256::digest(token.as_bytes());
    let absent = [0u8; 32];
    let expected = row
        .as_ref()
        .map_or(absent.as_slice(), |row| row.guest_token_hash.as_slice());
    let valid = bool::from(hash.as_slice().ct_eq(expected));
    row.filter(|row| {
        valid
            && row.guest_token_expires_at > chrono::Utc::now()
            && row.guest_token_revoked_at.is_none()
    })
    .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "Checkout not found"))
}
fn guest_bearer(headers: &HeaderMap) -> Result<&str, AppError> {
    let invalid = || {
        AppError::new(StatusCode::UNAUTHORIZED, "Invalid checkout credential")
            .with_bearer_challenge()
    };
    if headers.get_all("authorization").iter().count() != 1 {
        return Err(invalid());
    }
    let value = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(invalid)?;
    let (scheme, token) = value.split_once(' ').ok_or_else(invalid)?;
    let bare = token.trim_end_matches('=');
    if !scheme.eq_ignore_ascii_case("Bearer")
        || bare.is_empty()
        || token.len() > 4096
        || !bare
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~+/".contains(&b))
    {
        return Err(invalid());
    }
    Ok(token)
}

fn response(outcome: crate::models::checkout_completion::CompletionOutcome) -> Response {
    use crate::models::checkout_completion::{CompletionState, PaymentState};
    use domain::dto::{
        CheckoutCompletionState as Completion, CheckoutPaymentState as Payment,
        VerifyPaymentResponse,
    };
    let done = outcome.completion_state == CompletionState::Complete
        || matches!(
            outcome.payment_state,
            PaymentState::Unpaid | PaymentState::Expired
        );
    let response = VerifyPaymentResponse {
        payment_state: match outcome.payment_state {
            PaymentState::Pending => Payment::Pending,
            PaymentState::Unpaid => Payment::Unpaid,
            PaymentState::Expired => Payment::Expired,
            PaymentState::Paid => Payment::Paid,
        },
        completion_state: match outcome.completion_state {
            CompletionState::Ready => Completion::Ready,
            CompletionState::Processing => Completion::Processing,
            CompletionState::Uncertain => Completion::Uncertain,
            CompletionState::OrderSaved => Completion::OrderSaved,
            CompletionState::Complete => Completion::Complete,
            CompletionState::Operator => Completion::Operator,
        },
        order_id: outcome.order_id.map(|id| id.as_ref().to_owned()),
    };
    let mut response = (
        if done {
            StatusCode::OK
        } else {
            StatusCode::ACCEPTED
        },
        axum::Json(response),
    )
        .into_response();
    if !done {
        response
            .headers_mut()
            .insert("retry-after", axum::http::HeaderValue::from_static("5"));
    }
    response
}
