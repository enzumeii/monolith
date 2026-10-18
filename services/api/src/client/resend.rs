use crate::{
    client::{ClientError, ClientErrorKind},
    config::ResendConfig,
};
use resend_rs::{
    ConfigBuilder, Resend,
    types::{CreateEmailBaseOptions, EmailId},
};
use std::time::Duration;

pub struct ResendClient {
    inner: Resend,
    default_sender: String,
}

impl ResendClient {
    pub fn new(config: &ResendConfig) -> Result<Self, ClientError> {
        let base_url = resend_http::Url::parse(&config.base_url)
            .ok()
            .filter(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none()
            })
            .ok_or_else(|| ClientError::new(ClientErrorKind::InvalidInput("invalid Resend URL")))?;
        let api_key = config
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| {
                ClientError::new(ClientErrorKind::InvalidInput("Resend API key is missing"))
            })?;
        resend_http::header::HeaderValue::from_str(api_key).map_err(|_| {
            ClientError::new(ClientErrorKind::InvalidInput(
                "invalid Resend API key header",
            ))
        })?;
        if config.timeout_secs == 0 || config.default_sender.trim().is_empty() {
            return Err(ClientError::new(ClientErrorKind::InvalidInput(
                "Resend timeout must be positive and default sender must not be empty",
            )));
        }
        // The SDK can panic on its ambient rate-limit setting; keep Resend optional.
        let settings = std::panic::catch_unwind(|| {
            let http = resend_http::Client::builder()
                .timeout(Duration::from_secs(config.timeout_secs))
                .redirect(resend_http::redirect::Policy::none())
                .retry(resend_http::retry::never())
                .build()
                .map_err(resend_rs::Error::from)?;
            Ok::<_, ClientError>(
                ConfigBuilder::new(api_key)
                    .base_url(base_url)
                    .client(http)
                    .build(),
            )
        })
        .map_err(|_| {
            ClientError::new(ClientErrorKind::InvalidInput(
                "Resend initialization failed",
            ))
        })??;
        Ok(Self {
            inner: Resend::with_config(settings),
            default_sender: config.default_sender.clone(),
        })
    }

    pub async fn send_email(
        &self,
        from: Option<&str>,
        to: &[&str],
        subject: &str,
        email: &str,
    ) -> Result<EmailId, ClientError> {
        let options = CreateEmailBaseOptions::new(
            from.unwrap_or(&self.default_sender),
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
