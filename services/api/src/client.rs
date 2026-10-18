use reqwest::{
    Method, Response, Url,
    header::{HeaderMap, RETRY_AFTER},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, fmt, panic::Location, time::Duration};

pub mod resend;
pub mod shopify;

pub struct ClientError {
    kind: ClientErrorKind,
    location: &'static Location<'static>,
}

pub enum ClientErrorKind {
    Transport {
        source: reqwest::Error,
    },
    Http {
        status: u16,
        request_id: Option<String>,
    },
    Decode {
        source: serde_json::Error,
    },
    Graphql {
        errors: Vec<GraphqlError>,
        extensions: serde_json::Value,
    },
    Mutation {
        errors: Vec<MutationUserError>,
    },
    Protocol(&'static str),
    InvalidInput(&'static str),
    RetryExhausted,
    Upstream {
        source: Box<dyn std::error::Error + Send + Sync>,
        timed_out: bool,
    },
}
impl ClientError {
    #[must_use]
    #[track_caller]
    pub fn new(kind: ClientErrorKind) -> Self {
        Self {
            kind,
            location: Location::caller(),
        }
    }

    #[must_use]
    pub fn kind(&self) -> &ClientErrorKind {
        &self.kind
    }

    #[must_use]
    pub fn location(&self) -> &'static Location<'static> {
        self.location
    }
}

impl From<reqwest::Error> for ClientError {
    #[track_caller]
    fn from(source: reqwest::Error) -> Self {
        Self::new(ClientErrorKind::Transport { source })
    }
}

impl From<serde_json::Error> for ClientError {
    #[track_caller]
    fn from(source: serde_json::Error) -> Self {
        Self::new(ClientErrorKind::Decode { source })
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            ClientErrorKind::Transport { .. } => f.write_str("HTTP transport failed"),
            ClientErrorKind::Http { status, .. } => write!(f, "HTTP request failed ({status})"),
            ClientErrorKind::Decode { .. } => f.write_str("Response decoding failed"),
            ClientErrorKind::Graphql { .. } => f.write_str("GraphQL operation failed"),
            ClientErrorKind::Mutation { .. } => f.write_str("GraphQL mutation rejected"),
            ClientErrorKind::Protocol(message) => write!(f, "Invalid upstream response: {message}"),
            ClientErrorKind::InvalidInput(message) => write!(f, "Invalid client input: {message}"),
            ClientErrorKind::RetryExhausted => f.write_str("Retry budget exhausted"),
            ClientErrorKind::Upstream { .. } => f.write_str("Upstream request failed"),
        }
    }
}
impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.kind() {
            ClientErrorKind::Transport { source } => Some(source),
            ClientErrorKind::Decode { source } => Some(source),
            ClientErrorKind::Upstream { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
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
            return Err(ClientError::new(ClientErrorKind::InvalidInput(
                "timeout must be positive",
            )));
        }
        let inner = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { inner })
    }
    pub async fn request_json<B: Serialize, R: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        headers: HeaderMap,
        body: Option<&B>,
    ) -> Result<R, ClientError> {
        self.request_with_retry(method, url, headers, body, |_| false)
            .await
    }
    async fn request_with_retry<B: Serialize, R: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        headers: HeaderMap,
        body: Option<&B>,
        should_retry: impl Fn(&R) -> bool,
    ) -> Result<R, ClientError> {
        let mut retry = RetryState::default();
        loop {
            let response = self
                .send(method.clone(), url.clone(), headers.clone(), body)
                .await?;
            if response.status().is_client_error() {
                let hints = if response.status().as_u16() == 429 {
                    response.headers().clone()
                } else {
                    HeaderMap::new()
                };
                retry.wait(&hints).await?;
                continue;
            }
            check_status(&response)?;
            let response_headers = response.headers().clone();
            let decoded = decode(response).await?;
            if should_retry(&decoded) {
                retry.wait(&response_headers).await?;
                continue;
            }
            return Ok(decoded);
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
        Ok(request.send().await?)
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
        let envelope: GraphqlResponse = self
            .http
            .request_with_retry(
                Method::POST,
                self.endpoint.clone(),
                headers,
                Some(request),
                |response: &GraphqlResponse| {
                    response.data.is_none()
                        && !response.errors.is_empty()
                        && response.errors.iter().all(|error| {
                            error
                                .extensions
                                .get("code")
                                .and_then(serde_json::Value::as_str)
                                == Some("THROTTLED")
                        })
                },
            )
            .await?;
        if !envelope.errors.is_empty() {
            return Err(ClientError::new(ClientErrorKind::Graphql {
                errors: envelope.errors,
                extensions: envelope.extensions,
            }));
        }
        let data = envelope.data.ok_or_else(|| {
            ClientError::new(ClientErrorKind::Protocol("GraphQL data is missing"))
        })?;
        Ok(serde_json::from_value(data)?)
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
            return Err(ClientError::new(ClientErrorKind::RetryExhausted));
        }
        let base = Duration::from_secs(1 << self.retries);
        let minimum = retry_after(headers).unwrap_or_default();
        let remaining = Duration::from_secs(30).saturating_sub(self.slept);
        if minimum > remaining {
            return Err(ClientError::new(ClientErrorKind::RetryExhausted));
        }
        let delay = base.max(minimum) + Duration::from_millis(rand::random_range(0..=250));
        if delay > remaining {
            return Err(ClientError::new(ClientErrorKind::RetryExhausted));
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
    Err(ClientError::new(ClientErrorKind::Http {
        status: response.status().as_u16(),
        request_id: response
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
    }))
}
async fn decode<D: DeserializeOwned>(response: Response) -> Result<D, ClientError> {
    let bytes = response.bytes().await?;
    Ok(serde_json::from_slice(&bytes)?)
}
#[cfg(test)]
#[path = "tests/client.rs"]
pub(crate) mod tests;
