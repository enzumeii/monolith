use crate::{
    client::{ClientError, ClientErrorKind},
    config::ResendConfig,
};
use resend_rs::{
    Resend,
    types::{CreateEmailBaseOptions, EmailId},
};

pub struct ResendClient {
    inner: Resend,
    timeout: std::time::Duration,
}

impl ResendClient {
    pub fn new(config: &ResendConfig) -> Result<Self, ClientError> {
        let api_key = config
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| {
                ClientError::new(ClientErrorKind::InvalidInput("Resend API key is missing"))
            })?;
        let timeout = config
            .timeout_secs
            .filter(|seconds| *seconds > 0)
            .ok_or_else(|| {
                ClientError::new(ClientErrorKind::InvalidInput(
                    "Resend timeout is missing or zero",
                ))
            })?;
        // The SDK can panic on its own environment settings; keep Resend optional.
        let inner = std::panic::catch_unwind(|| Resend::new(api_key)).map_err(|_| {
            ClientError::new(ClientErrorKind::InvalidInput(
                "Resend initialization failed",
            ))
        })?;
        Ok(Self {
            inner,
            timeout: std::time::Duration::from_secs(timeout),
        })
    }

    pub async fn send_email(
        &self,
        options: impl Into<resend_rs::idempotent::Idempotent<CreateEmailBaseOptions>>,
    ) -> Result<EmailId, ClientError> {
        let response = tokio::time::timeout(self.timeout, self.inner.emails.send(options))
            .await
            .map_err(|source| {
                ClientError::new(ClientErrorKind::Upstream {
                    source: Box::new(source),
                    timed_out: true,
                })
            })??;
        if response.id.as_ref().trim().is_empty() {
            return Err(ClientError::new(ClientErrorKind::Protocol(
                "email result missing",
            )));
        }
        Ok(response.id)
    }
}

impl From<resend_rs::Error> for ClientError {
    #[track_caller]
    fn from(source: resend_rs::Error) -> Self {
        let timed_out = matches!(&source, resend_rs::Error::Http(error) if error.is_timeout());
        Self::new(ClientErrorKind::Upstream {
            source: Box::new(source),
            timed_out,
        })
    }
}

#[cfg(test)]
#[path = "../tests/client/resend.rs"]
pub(crate) mod tests;
