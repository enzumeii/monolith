pub mod paypal;
pub mod stripe;

#[derive(Debug, thiserror::Error)]
#[error("Payment evidence does not match")]
pub(crate) struct EvidenceMismatch;

#[track_caller]
pub(crate) fn evidence_mismatch() -> crate::client::ClientError {
    crate::client::ClientError::new(crate::client::ClientErrorKind::Upstream {
        source: Box::new(EvidenceMismatch),
        timed_out: false,
    })
}
