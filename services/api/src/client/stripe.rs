pub mod webhook;

use crate::{
    client::{ClientError, ClientErrorKind, HttpClient},
    config::StripeConfig,
    models::stripe::{
        Account, BalanceTransaction, Charge, CheckoutMode, CheckoutSession, CheckoutStatus,
        CreateCheckoutSession, PaymentIntent, Payout, Refund, StripePage,
    },
};
use reqwest::{
    Method, Url,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;
use std::time::Duration;

pub struct StripeClient {
    http: HttpClient,
    base_url: Url,
    headers: HeaderMap,
}

impl StripeClient {
    pub fn new(config: &StripeConfig) -> Result<Self, ClientError> {
        let key = config
            .secret_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| {
                ClientError::new(ClientErrorKind::InvalidInput(
                    "Stripe secret key is missing",
                ))
            })?;
        let mut auth = HeaderValue::from_str(&format!("Bearer {key}")).map_err(|_| {
            ClientError::new(ClientErrorKind::InvalidInput("invalid Stripe secret key"))
        })?;
        auth.set_sensitive(true);
        let base_url = Url::parse(&config.base_url)
            .map_err(|_| ClientError::new(ClientErrorKind::InvalidInput("invalid Stripe URL")))?;
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, auth);
        Ok(Self {
            http: HttpClient::new(Duration::from_secs(config.timeout_secs))?,
            base_url,
            headers,
        })
    }

    pub async fn list_payouts(
        &self,
        start: i64,
        end: i64,
        status: Option<&str>,
        limit: u8,
        cursor: Option<&str>,
    ) -> Result<StripePage<Payout>, ClientError> {
        if start > end {
            return Err(ClientError::new(ClientErrorKind::InvalidInput(
                "Stripe arrival date range is reversed",
            )));
        }
        let status = status.unwrap_or("paid");
        if !matches!(status, "pending" | "paid" | "failed" | "canceled") {
            return Err(ClientError::new(ClientErrorKind::InvalidInput(
                "invalid Stripe payout status",
            )));
        }
        let mut url = self.list_url("payouts", limit, cursor)?;
        url.query_pairs_mut()
            .append_pair("arrival_date[gte]", &start.to_string())
            .append_pair("arrival_date[lte]", &end.to_string())
            .append_pair("status", status);
        self.get(url).await
    }

    pub async fn list_balance_transactions(
        &self,
        payout_id: &str,
        limit: u8,
        cursor: Option<&str>,
    ) -> Result<StripePage<BalanceTransaction>, ClientError> {
        validate_id(payout_id)?;
        let mut url = self.list_url("balance_transactions", limit, cursor)?;
        url.query_pairs_mut().append_pair("payout", payout_id);
        self.get(url).await
    }

    pub async fn get_charge(&self, id: &str) -> Result<Charge, ClientError> {
        self.get(self.resource_url("charges", id)?).await
    }

    pub async fn get_refund(&self, id: &str) -> Result<Refund, ClientError> {
        self.get(self.resource_url("refunds", id)?).await
    }

    pub async fn create_checkout_session(
        &self,
        input: &CreateCheckoutSession,
    ) -> Result<CheckoutSession, ClientError> {
        self.checkout_session(input, false).await
    }

    pub async fn recover_checkout_session(
        &self,
        input: &CreateCheckoutSession,
    ) -> Result<CheckoutSession, ClientError> {
        self.checkout_session(input, true).await
    }

    pub async fn get_checkout_session(&self, id: &str) -> Result<CheckoutSession, ClientError> {
        validate_id(id)?;
        self.get(self.operation_url(&["checkout", "sessions", id])?)
            .await
    }

    pub async fn get_payment_intent(&self, id: &str) -> Result<PaymentIntent, ClientError> {
        self.get(self.resource_url("payment_intents", id)?).await
    }

    pub async fn get_account(&self) -> Result<Account, ClientError> {
        self.get(self.operation_url(&["account"])?).await
    }

    async fn checkout_session(
        &self,
        input: &CreateCheckoutSession,
        recovery: bool,
    ) -> Result<CheckoutSession, ClientError> {
        validate_checkout(input, recovery)?;
        let mut form = vec![
            ("mode".to_owned(), "payment".to_owned()),
            ("payment_method_types[0]".into(), "card".into()),
            ("adaptive_pricing[enabled]".into(), "false".into()),
            ("line_items[0][quantity]".into(), "1".into()),
            (
                "line_items[0][price_data][unit_amount]".into(),
                input.amount.to_string(),
            ),
            (
                "line_items[0][price_data][currency]".into(),
                input.currency.to_ascii_lowercase(),
            ),
            (
                "line_items[0][price_data][product_data][name]".into(),
                input.product_name.clone(),
            ),
            ("success_url".into(), input.success_url.clone()),
            ("cancel_url".into(), input.cancel_url.clone()),
        ];
        if let Some(expires_at) = input.expires_at {
            form.push(("expires_at".into(), expires_at.to_string()));
        }
        for (key, value) in &input.metadata {
            form.push((format!("metadata[{key}]"), value.clone()));
            form.push((
                format!("payment_intent_data[metadata][{key}]"),
                value.clone(),
            ));
        }
        let session: CheckoutSession = self
            .post(
                self.operation_url(&["checkout", "sessions"])?,
                &input.idempotency_key,
                &form,
            )
            .await?;
        if session.object != "checkout.session"
            || validate_id(&session.id).is_err()
            || !matches!(session.mode, CheckoutMode::Payment)
            || session.amount_total != Some(input.amount)
            || session.currency.as_deref() != Some(input.currency.to_ascii_lowercase().as_str())
            || session.metadata != input.metadata
            || input
                .expires_at
                .is_some_and(|expiry| session.expires_at != Some(expiry))
        {
            return Err(ClientError::new(ClientErrorKind::Protocol(
                "Checkout Session does not match creation request",
            )));
        }
        let valid_url = session
            .url
            .as_deref()
            .and_then(|raw| Url::parse(raw).ok())
            .is_some_and(|url| {
                url.scheme() == "https"
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
            });
        if !(valid_url || session.status == CheckoutStatus::Complete && session.url.is_none()) {
            return Err(ClientError::new(ClientErrorKind::Protocol(
                "Checkout hosted URL is missing or invalid",
            )));
        }
        Ok(session)
    }

    async fn post<T: DeserializeOwned>(
        &self,
        url: Url,
        key: &str,
        form: &[(String, String)],
    ) -> Result<T, ClientError> {
        let mut headers = self.headers.clone();
        headers.insert("idempotency-key", idempotency_header(key)?);
        headers.insert(
            "stripe-version",
            HeaderValue::from_static("2026-08-26.dahlia"),
        );
        self.http
            .request_form(Method::POST, url, headers, form)
            .await
    }

    fn operation_url(&self, segments: &[&str]) -> Result<Url, ClientError> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|()| ClientError::new(ClientErrorKind::InvalidInput("invalid Stripe URL")))?
            .pop_if_empty()
            .extend(segments.iter().copied());
        Ok(url)
    }

    fn list_url(
        &self,
        resource: &str,
        limit: u8,
        cursor: Option<&str>,
    ) -> Result<Url, ClientError> {
        if !(1..=100).contains(&limit) {
            return Err(ClientError::new(ClientErrorKind::InvalidInput(
                "Stripe page size must be between 1 and 100",
            )));
        }
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|()| ClientError::new(ClientErrorKind::InvalidInput("invalid Stripe URL")))?
            .pop_if_empty()
            .push(resource);
        url.query_pairs_mut()
            .append_pair("limit", &limit.to_string());
        if let Some(cursor) = cursor {
            validate_id(cursor)?;
            url.query_pairs_mut().append_pair("starting_after", cursor);
        }
        Ok(url)
    }

    fn resource_url(&self, resource: &str, id: &str) -> Result<Url, ClientError> {
        validate_id(id)?;
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|()| ClientError::new(ClientErrorKind::InvalidInput("invalid Stripe URL")))?
            .pop_if_empty()
            .push(resource)
            .push(id);
        Ok(url)
    }

    async fn get<T: DeserializeOwned>(&self, url: Url) -> Result<T, ClientError> {
        self.http
            .request_json::<(), T>(Method::GET, url, self.headers.clone(), None)
            .await
    }
}

fn validate_id(id: &str) -> Result<(), ClientError> {
    if !crate::models::stripe::valid_id(id) {
        return Err(ClientError::new(ClientErrorKind::InvalidInput(
            "invalid Stripe object ID",
        )));
    }
    Ok(())
}

fn idempotency_header(key: &str) -> Result<HeaderValue, ClientError> {
    if key.is_empty() || key.len() > 255 || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(ClientError::new(ClientErrorKind::InvalidInput(
            "invalid Stripe idempotency key",
        )));
    }
    HeaderValue::from_str(key).map_err(|_| {
        ClientError::new(ClientErrorKind::InvalidInput(
            "invalid Stripe idempotency key",
        ))
    })
}

fn validate_checkout(input: &CreateCheckoutSession, recovery: bool) -> Result<(), ClientError> {
    if !(1..=99_999_999).contains(&input.amount) {
        return Err(ClientError::new(ClientErrorKind::InvalidInput(
            "Stripe amount must be a positive integer of at most eight digits",
        )));
    }
    if input.currency.len() != 3 || !input.currency.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(ClientError::new(ClientErrorKind::InvalidInput(
            "invalid Stripe currency",
        )));
    }
    if input.product_name.trim().is_empty() || input.product_name.len() > 250 {
        return Err(ClientError::new(ClientErrorKind::InvalidInput(
            "invalid Stripe product name",
        )));
    }
    if let Some(expires_at) = input.expires_at {
        let remaining = expires_at.saturating_sub(chrono::Utc::now().timestamp());
        let minimum_remaining = if recovery { 1 } else { 1800 };
        if !(minimum_remaining..=86_400).contains(&remaining) {
            return Err(ClientError::new(ClientErrorKind::InvalidInput(
                if recovery {
                    "Stripe Checkout recovery requires an unexpired original expiry within 24 hours"
                } else {
                    "Stripe Checkout expiry must be between 30 minutes and 24 hours"
                },
            )));
        }
    } else if recovery {
        return Err(ClientError::new(ClientErrorKind::InvalidInput(
            "Stripe Checkout recovery requires the original expiry",
        )));
    }
    idempotency_header(&input.idempotency_key)?;
    for raw in [&input.success_url, &input.cancel_url] {
        let url = Url::parse(raw).map_err(|_| {
            ClientError::new(ClientErrorKind::InvalidInput("invalid Checkout return URL"))
        })?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if !(url.scheme() == "https" || (url.scheme() == "http" && local))
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(ClientError::new(ClientErrorKind::InvalidInput(
                "invalid Checkout return URL",
            )));
        }
    }
    if input.metadata.len() > 50
        || input.metadata.iter().any(|(key, value)| {
            key.is_empty() || key.len() > 40 || key.contains(['[', ']']) || value.len() > 500
        })
    {
        return Err(ClientError::new(ClientErrorKind::InvalidInput(
            "invalid Stripe metadata",
        )));
    }
    Ok(())
}
