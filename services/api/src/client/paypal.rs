use super::{ClientError, ClientErrorKind, HttpClient};
use crate::{config::PaypalConfig, models::paypal::TransactionPage};
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::{
    Url,
    header::{AUTHORIZATION, HeaderValue},
};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use tokio::{sync::Mutex, time::Instant};

pub struct PaypalClient {
    http: HttpClient,
    base_url: Url,
    client_id: String,
    client_secret: String,
    token: Mutex<Option<Arc<AccessToken>>>,
}

struct AccessToken {
    authorization: HeaderValue,
    renew_at: Instant,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
}

impl PaypalClient {
    pub fn new(config: &PaypalConfig) -> Result<Self, ClientError> {
        let base_url = Url::parse(&config.base_url)
            .map_err(|_| ClientError::new(ClientErrorKind::InvalidInput("invalid PayPal URL")))?;
        let loopback = matches!(
            base_url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        );
        if base_url.host_str().is_none()
            || !(base_url.scheme() == "https" || (base_url.scheme() == "http" && loopback))
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
            || base_url.path() != "/"
        {
            return Err(ClientError::new(ClientErrorKind::InvalidInput(
                "invalid PayPal URL",
            )));
        }
        let client_id = credential(config.client_id.as_deref())?;
        let client_secret = credential(config.client_secret.as_deref())?;
        if client_id.contains(':') {
            return Err(ClientError::new(ClientErrorKind::InvalidInput(
                "invalid PayPal client ID",
            )));
        }
        Ok(Self {
            http: HttpClient::new(Duration::from_secs(config.timeout_secs))?,
            base_url,
            client_id,
            client_secret,
            token: Mutex::new(None),
        })
    }

    pub async fn get_transactions(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        page_size: u16,
        page: u32,
    ) -> Result<TransactionPage, ClientError> {
        if start > end
            || end.signed_duration_since(start) > chrono::Duration::days(31)
            || !(1..=500).contains(&page_size)
            || page == 0
            || page > 2_147_483_647
        {
            return Err(ClientError::new(ClientErrorKind::InvalidInput(
                "invalid PayPal transaction search range or page",
            )));
        }
        let mut url = self
            .base_url
            .join("v1/reporting/transactions")
            .map_err(|_| ClientError::new(ClientErrorKind::InvalidInput("invalid PayPal URL")))?;
        url.query_pairs_mut().extend_pairs([
            (
                "start_date",
                start.to_rfc3339_opts(SecondsFormat::AutoSi, true),
            ),
            ("end_date", end.to_rfc3339_opts(SecondsFormat::AutoSi, true)),
            ("fields", "all".into()),
            ("page_size", page_size.to_string()),
            ("page", page.to_string()),
        ]);
        let token = self.access_token(None).await?;
        let response = self.transactions_page(url.clone(), &token).await;
        let response = match response {
            Err(error) if matches!(error.kind(), ClientErrorKind::Http { status: 401, .. }) => {
                let renewed = self.access_token(Some(&token)).await?;
                self.transactions_page(url, &renewed).await?
            }
            other => other?,
        };
        if response.page != page
            || (response.total_items > 0
                && (response.total_pages < page || response.transaction_details.is_empty()))
            || (response.total_items == 0
                && (!response.transaction_details.is_empty() || response.total_pages > 1))
            || response.transaction_details.len() > usize::from(page_size)
        {
            return Err(ClientError::new(ClientErrorKind::Protocol(
                "invalid PayPal pagination metadata",
            )));
        }
        Ok(response)
    }

    async fn transactions_page(
        &self,
        url: Url,
        token: &AccessToken,
    ) -> Result<TransactionPage, ClientError> {
        self.http
            .execute(
                self.http
                    .inner
                    .get(url)
                    .header(AUTHORIZATION, token.authorization.clone()),
                |status| status.as_u16() == 429,
                |_| false,
            )
            .await
    }

    async fn access_token(
        &self,
        rejected: Option<&Arc<AccessToken>>,
    ) -> Result<Arc<AccessToken>, ClientError> {
        let mut cached = self.token.lock().await;
        if let Some(token) = cached.as_ref()
            && token.renew_at > Instant::now()
            && rejected.is_none_or(|rejected| !Arc::ptr_eq(token, rejected))
        {
            return Ok(Arc::clone(token));
        }
        *cached = None;
        let url = self
            .base_url
            .join("v1/oauth2/token")
            .map_err(|_| ClientError::new(ClientErrorKind::InvalidInput("invalid PayPal URL")))?;
        let requested_at = Instant::now();
        let response: TokenResponse = self
            .http
            .execute(
                self.http
                    .inner
                    .post(url)
                    .basic_auth(&self.client_id, Some(&self.client_secret))
                    .form(&[("grant_type", "client_credentials")]),
                |status| status.as_u16() == 429,
                |_| false,
            )
            .await?;
        if response.access_token.trim().is_empty()
            || !response.token_type.eq_ignore_ascii_case("Bearer")
            || response.expires_in == 0
        {
            return Err(ClientError::new(ClientErrorKind::Protocol(
                "invalid PayPal access token",
            )));
        }
        let lifetime = Duration::from_secs(response.expires_in);
        let renew_at = requested_at
            .checked_add(lifetime.saturating_sub(Duration::from_secs(30).min(lifetime / 10)))
            .filter(|expiry| *expiry > Instant::now())
            .ok_or_else(|| {
                ClientError::new(ClientErrorKind::Protocol("invalid PayPal token expiry"))
            })?;
        let mut authorization = HeaderValue::from_str(&format!("Bearer {}", response.access_token))
            .map_err(|_| {
                ClientError::new(ClientErrorKind::Protocol("invalid PayPal access token"))
            })?;
        authorization.set_sensitive(true);
        let token = Arc::new(AccessToken {
            authorization,
            renew_at,
        });
        *cached = Some(Arc::clone(&token));
        Ok(token)
    }
}

fn credential(value: Option<&str>) -> Result<String, ClientError> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            ClientError::new(ClientErrorKind::InvalidInput(
                "PayPal credentials are missing",
            ))
        })
}

#[cfg(test)]
#[path = "../tests/client/paypal.rs"]
mod tests;
