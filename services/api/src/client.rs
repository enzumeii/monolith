use reqwest::{
    Method, Response, Url,
    header::{HeaderMap, RETRY_AFTER},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, fmt, time::Duration};
use thiserror::Error;

pub mod shopify;

#[derive(Error)]
pub enum ClientError {
    #[error("HTTP transport failed in {origin}")]
    Transport {
        origin: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("HTTP request failed ({status})")]
    Http {
        status: u16,
        request_id: Option<String>,
    },
    #[error("Response decoding failed in {origin}")]
    Decode {
        origin: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("GraphQL operation failed")]
    Graphql {
        errors: Vec<GraphqlError>,
        extensions: serde_json::Value,
    },
    #[error("GraphQL mutation rejected")]
    Mutation { errors: Vec<MutationUserError> },
    #[error("Invalid upstream response: {0}")]
    Protocol(&'static str),
    #[error("Invalid client input: {0}")]
    InvalidInput(&'static str),
    #[error("Retry budget exhausted")]
    RetryExhausted,
}
impl fmt::Debug for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
#[derive(Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
}
impl HttpClient {
    pub fn new(timeout: Duration) -> Result<Self, ClientError> {
        if timeout.is_zero() {
            return Err(ClientError::InvalidInput("timeout must be positive"));
        }
        let inner = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(transport_error)?;
        Ok(Self { inner })
    }
    pub async fn request_json<B: Serialize, R: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        headers: HeaderMap,
        body: Option<&B>,
    ) -> Result<R, ClientError> {
        let mut retry = RetryState::default();
        loop {
            let response = self
                .send(method.clone(), url.clone(), headers.clone(), body)
                .await?;
            if response.status().as_u16() == 429 {
                retry.wait(response.headers()).await?;
                continue;
            }
            check_status(&response)?;
            return decode(response).await;
        }
    }
    async fn send<B: Serialize>(
        &self,
        method: Method,
        url: Url,
        headers: HeaderMap,
        body: Option<&B>,
    ) -> Result<Response, ClientError> {
        let mut request = self.inner.request(method, url).headers(headers);
        if let Some(body) = body {
            request = request.json(body);
        }
        request.send().await.map_err(transport_error)
    }
}
#[derive(Clone)]
pub struct GraphqlClient {
    http: HttpClient,
    endpoint: Url,
}
impl GraphqlClient {
    #[must_use]
    pub fn new(http: HttpClient, endpoint: Url) -> Self {
        Self { http, endpoint }
    }
    pub async fn execute<V: Serialize, D: DeserializeOwned>(
        &self,
        headers: HeaderMap,
        request: &GraphqlRequest<'_, V>,
    ) -> Result<D, ClientError> {
        let mut retry = RetryState::default();
        loop {
            let response = self
                .http
                .send(
                    Method::POST,
                    self.endpoint.clone(),
                    headers.clone(),
                    Some(request),
                )
                .await?;
            if response.status().as_u16() == 429 {
                retry.wait(response.headers()).await?;
                continue;
            }
            check_status(&response)?;
            let response_headers = response.headers().clone();
            let envelope: GraphqlResponse = decode(response).await?;
            if !envelope.errors.is_empty() {
                if envelope.data.is_none()
                    && envelope.errors.iter().all(|e| {
                        e.extensions.get("code").and_then(serde_json::Value::as_str)
                            == Some("THROTTLED")
                    })
                {
                    retry.wait(&response_headers).await?;
                    continue;
                }
                return Err(ClientError::Graphql {
                    errors: envelope.errors,
                    extensions: envelope.extensions,
                });
            }
            let data = envelope
                .data
                .ok_or(ClientError::Protocol("GraphQL data is missing"))?;
            return serde_json::from_value(data).map_err(|source| ClientError::Decode {
                origin: "api::client::GraphqlClient::execute",
                source,
            });
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlRequest<'a, V> {
    pub query: &'a str,
    pub operation_name: &'a str,
    pub variables: V,
}
#[derive(Deserialize)]
struct GraphqlResponse {
    data: Option<serde_json::Value>,
    #[serde(default)]
    errors: Vec<GraphqlError>,
    #[serde(default)]
    extensions: serde_json::Value,
}
#[derive(Clone, Deserialize)]
pub struct GraphqlError {
    pub message: String,
    #[serde(default)]
    pub path: Vec<serde_json::Value>,
    #[serde(default)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct MutationUserError {
    pub field: Option<Vec<String>>,
    pub message: String,
}
#[derive(Default)]
struct RetryState {
    retries: u32,
    slept: Duration,
}
impl RetryState {
    async fn wait(&mut self, headers: &HeaderMap) -> Result<(), ClientError> {
        if self.retries >= 3 {
            return Err(ClientError::RetryExhausted);
        }
        let base = Duration::from_secs(1 << self.retries);
        let minimum = retry_after(headers).unwrap_or_default();
        let remaining = Duration::from_secs(30).saturating_sub(self.slept);
        if minimum > remaining {
            return Err(ClientError::RetryExhausted);
        }
        let delay = base.max(minimum) + Duration::from_millis(rand::random_range(0..=250));
        if delay > remaining {
            return Err(ClientError::RetryExhausted);
        }
        self.retries += 1;
        self.slept += delay;
        tokio::time::sleep(delay).await;
        Ok(())
    }
}
fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let target = chrono::DateTime::parse_from_rfc2822(raw).ok()?;
    Some(
        (target.with_timezone(&chrono::Utc) - chrono::Utc::now())
            .to_std()
            .unwrap_or_default(),
    )
}
fn check_status(response: &Response) -> Result<(), ClientError> {
    if response.status().is_success() {
        return Ok(());
    }
    Err(ClientError::Http {
        status: response.status().as_u16(),
        request_id: response
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
    })
}
async fn decode<D: DeserializeOwned>(response: Response) -> Result<D, ClientError> {
    let bytes = response.bytes().await.map_err(transport_error)?;
    serde_json::from_slice(&bytes).map_err(|source| ClientError::Decode {
        origin: "api::client::decode",
        source,
    })
}
fn transport_error(source: reqwest::Error) -> ClientError {
    ClientError::Transport {
        origin: "api::client::HttpClient",
        source,
    }
}
#[cfg(test)]
#[path = "tests/client.rs"]
pub(crate) mod tests;
