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
        // The SDK can panic on its own environment settings; keep Resend optional.
        let inner = std::panic::catch_unwind(|| Resend::new(api_key)).map_err(|_| {
            ClientError::new(ClientErrorKind::InvalidInput(
                "Resend initialization failed",
            ))
        })?;
        Ok(Self { inner })
    }

    pub async fn send_email(
        &self,
        from: Option<&str>,
        to: &[&str],
        subject: &str,
        email: &str,
    ) -> Result<EmailId, ClientError> {
        let options = CreateEmailBaseOptions::new(
            from.unwrap_or("shipping@enzumeii.com"),
            to.iter().copied(),
            subject,
        )
        .with_html(email);
        let response = self.inner.emails.send(options).await?;
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
mod tests;
