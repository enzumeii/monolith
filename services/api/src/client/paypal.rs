use super::{ClientError, ClientErrorKind, HttpClient};
use crate::{
    config::PaypalConfig,
    models::paypal::{
        Capture, CreateOrderRequest, Order, TransactionPage, VerifyWebhookSignatureRequest,
        VerifyWebhookSignatureResponse,
    },
};
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::{
    RequestBuilder, Url,
    header::{AUTHORIZATION, HeaderValue},
};
use serde::{Deserialize, de::DeserializeOwned};
use std::{sync::Arc, time::Duration};
use tokio::{sync::Mutex, time::Instant};

pub mod payment;
pub mod webhook;

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
        let client_id = credential(config.client_id.as_deref())?;
        let client_secret = credential(config.client_secret.as_deref())?;
        Ok(Self {
            http: HttpClient::new(Duration::from_secs(config.timeout_secs))?,
            base_url,
            client_id,
            client_secret,
            token: Mutex::new(None),
        })
    }

    pub(crate) fn api_origin(&self) -> String {
        self.base_url.origin().ascii_serialization()
    }

    pub async fn get_transactions(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        page_size: u16,
        page: u32,
    ) -> Result<TransactionPage, ClientError> {
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
        self.bearer_request(|| self.http.inner.get(url.clone()))
            .await
    }

    pub async fn create_order(
        &self,
        request_id: &str,
        payload: &CreateOrderRequest,
    ) -> Result<Order, ClientError> {
        let url = self.operation_url(&["v2", "checkout", "orders"])?;
        self.bearer_request(|| {
            self.http
                .inner
                .post(url.clone())
                .header("PayPal-Request-Id", request_id)
                .header("Prefer", "return=representation")
                .json(payload)
        })
        .await
    }

    pub async fn get_order(&self, order_id: &str) -> Result<Order, ClientError> {
        let url = self.operation_url(&["v2", "checkout", "orders", order_id])?;
        self.bearer_request(|| self.http.inner.get(url.clone()))
            .await
    }

    pub async fn capture_order(
        &self,
        order_id: &str,
        request_id: &str,
    ) -> Result<Order, ClientError> {
        let url = self.operation_url(&["v2", "checkout", "orders", order_id, "capture"])?;
        self.bearer_request(|| {
            self.http
                .inner
                .post(url.clone())
                .header("PayPal-Request-Id", request_id)
                .header("Prefer", "return=representation")
                .json(&serde_json::json!({}))
        })
        .await
    }

    pub async fn get_capture(&self, capture_id: &str) -> Result<Capture, ClientError> {
        let url = self.operation_url(&["v2", "payments", "captures", capture_id])?;
        self.bearer_request(|| self.http.inner.get(url.clone()))
            .await
    }

    pub async fn verify_webhook_signature(
        &self,
        payload: &VerifyWebhookSignatureRequest,
    ) -> Result<VerifyWebhookSignatureResponse, ClientError> {
        let url = self.operation_url(&["v1", "notifications", "verify-webhook-signature"])?;
        self.bearer_request(|| self.http.inner.post(url.clone()).json(payload))
            .await
    }

    fn operation_url(&self, segments: &[&str]) -> Result<Url, ClientError> {
        let mut url = self.base_url.clone();
        url.set_query(None);
        url.set_fragment(None);
        url.path_segments_mut()
            .map_err(|()| ClientError::new(ClientErrorKind::InvalidInput("invalid PayPal URL")))?
            .pop_if_empty()
            .extend(segments.iter().copied());
        Ok(url)
    }

    async fn basic_auth_request<R: DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> Result<R, ClientError> {
        self.http
            .execute(
                request.basic_auth(&self.client_id, Some(&self.client_secret)),
                |status| status.as_u16() == 429,
                |_| false,
            )
            .await
    }

    async fn bearer_request<R: DeserializeOwned>(
        &self,
        make_request: impl Fn() -> RequestBuilder,
    ) -> Result<R, ClientError> {
        let mut token = self.access_token(None).await?;
        let mut renewed = false;
        loop {
            let response = self
                .http
                .execute(
                    make_request().header(AUTHORIZATION, token.authorization.clone()),
                    |status| status.as_u16() == 429,
                    |_| false,
                )
                .await;
            if !renewed
                && let Err(error) = &response
                && matches!(error.kind(), ClientErrorKind::Http { status: 401, .. })
            {
                token = self.access_token(Some(&token)).await?;
                renewed = true;
                continue;
            }
            return response;
        }
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
            .basic_auth_request(
                self.http
                    .inner
                    .post(url)
                    .form(&[("grant_type", "client_credentials")]),
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
    value.map(str::to_owned).ok_or_else(|| {
        ClientError::new(ClientErrorKind::InvalidInput(
            "PayPal credentials are missing",
        ))
    })
}

#[cfg(test)]
#[path = "../tests/client/paypal.rs"]
mod tests;
