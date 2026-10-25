use crate::services;
use crate::{error::AppError, models::checkout::RecoveryCode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryOutcome {
    Observed(RecoveryCode),
    Retry(RecoveryCode),
    Manual(RecoveryCode),
}

#[derive(Debug, thiserror::Error)]
#[error("Checkout requires manual reconciliation")]
struct ManualRecovery(RecoveryCode);

pub(super) fn manual(error: AppError, code: RecoveryCode) -> AppError {
    error.with_source(ManualRecovery(code))
}

pub(super) fn classify(error: &AppError) -> RecoveryOutcome {
    if let Some(marker) = error.find_source::<ManualRecovery>() {
        return RecoveryOutcome::Manual(marker.0);
    }
    if error
        .find_source::<services::payments::EvidenceMismatch>()
        .is_some()
        || error
            .find_source::<crate::client::shopify::CheckoutIdentityMismatch>()
            .is_some()
        || error
            .find_source::<crate::models::checkout_completion::CompletionEvidenceConflict>()
            .is_some()
    {
        RecoveryOutcome::Manual(RecoveryCode::EvidenceConflict)
    } else {
        RecoveryOutcome::Retry(RecoveryCode::TransientFailure)
    }
}

#[cfg(test)]
#[path = "../../tests/services/checkout/recovery.rs"]
mod tests;
