use crate::{
    client::{ClientError, ClientErrorKind, HttpClient},
    config::StripeConfig,
    models::stripe::{BalanceTransaction, Charge, Payout, Refund, StripePage},
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
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(ClientError::new(ClientErrorKind::InvalidInput(
            "invalid Stripe object ID",
        )));
    }
    Ok(())
}
