#![allow(clippy::unwrap_used)]
use super::*;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::State,
    http::{Request, StatusCode},
    response::Response,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tokio::{net::TcpListener, task::JoinHandle};

#[derive(Clone)]
pub(super) struct Reply {
    pub status: u16,
    pub body: String,
    pub headers: Vec<(String, String)>,
}
impl Reply {
    pub fn json(body: Value) -> Self {
        Self {
            status: 200,
            body: body.to_string(),
            headers: vec![],
        }
    }
    pub fn status(status: u16) -> Self {
        Self {
            status,
            body: "{}".into(),
            headers: vec![],
        }
    }
}
type Captured = (String, HeaderMap, Value);
#[derive(Clone)]
struct FixtureState {
    replies: Arc<Mutex<VecDeque<Reply>>>,
    requests: Arc<Mutex<Vec<Captured>>>,
}
pub(super) struct Fixture {
    pub url: Url,
    state: FixtureState,
    server: JoinHandle<()>,
}
impl Fixture {
    pub async fn new(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!(
            "http://{}/graphql.json",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let state = FixtureState {
            replies: Arc::new(Mutex::new(replies.into())),
            requests: Arc::new(Mutex::new(vec![])),
        };
        let app = Router::new().fallback(respond).with_state(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { url, state, server }
    }
    pub fn requests(&self) -> Vec<Captured> {
        self.state.requests.lock().unwrap().clone()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
async fn respond(State(state): State<FixtureState>, request: Request<Body>) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = to_bytes(body, 1_000_000).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    state
        .requests
        .lock()
        .unwrap()
        .push((parts.uri.to_string(), parts.headers, value));
    let reply = state
        .replies
        .lock()
        .unwrap()
        .pop_front()
        .unwrap_or(Reply::status(500));
    let mut response = Response::builder().status(StatusCode::from_u16(reply.status).unwrap());
    for (key, value) in reply.headers {
        response = response.header(key, value);
    }
    response.body(Body::from(reply.body)).unwrap()
}
fn graphql(fixture: &Fixture) -> GraphqlClient {
    GraphqlClient::new(
        HttpClient::new(Duration::from_secs(1)).unwrap(),
        fixture.url.clone(),
    )
}
async fn execute(fixture: &Fixture) -> Result<Value, ClientError> {
    graphql(fixture)
        .execute(
            HeaderMap::new(),
            &GraphqlRequest {
                query: "query Test { shop { name } }",
                operation_name: "Test",
                variables: json!({}),
            },
        )
        .await
}
#[tokio::test]
async fn decodes_typed_data_and_sends_request_envelope() {
    let fixture = Fixture::new(vec![Reply::json(json!({"data":{"answer":42}}))]).await;
    assert_eq!(execute(&fixture).await.unwrap(), json!({"answer":42}));
    let requests = fixture.requests();
    assert_eq!(requests[0].0, "/graphql.json");
    assert_eq!(requests[0].2["operationName"], "Test");
    assert_eq!(requests[0].2["variables"], json!({}));
}
#[tokio::test]
async fn generic_json_transport_preserves_method_headers_and_body() {
    let fixture = Fixture::new(vec![Reply::json(json!({"ok":true}))]).await;
    let mut headers = HeaderMap::new();
    headers.insert("x-custom", "custom-value".parse().unwrap());
    let result: Value = HttpClient::new(Duration::from_secs(1))
        .unwrap()
        .request_json(
            Method::PUT,
            fixture.url.clone(),
            headers,
            Some(&json!({"value":7})),
        )
        .await
        .unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(fixture.requests()[0].1["x-custom"], "custom-value");
    assert_eq!(fixture.requests()[0].2["value"], 7);
}
#[tokio::test]
async fn never_retries_server_errors_even_with_throttled_body_and_retry_after() {
    for status in [500, 502, 503, 504, 401] {
        let mut reply =
            Reply::json(json!({"errors":[{"message":"wait","extensions":{"code":"THROTTLED"}}]}));
        reply.status = status;
        reply.headers.push(("retry-after".into(), "1".into()));
        let fixture = Fixture::new(vec![reply, Reply::json(json!({"data":{"ok":true}}))]).await;
        assert!(
            matches!(execute(&fixture).await,Err(ClientError::Http { status: actual,.. }) if actual==status)
        );
        assert_eq!(fixture.requests().len(), 1);
    }
}
#[tokio::test]
async fn rejects_graphql_errors_partial_data_and_missing_data_without_retry() {
    for response in [
        json!({"errors":[{"message":"denied","extensions":{"code":"ACCESS_DENIED"}}]}),
        json!({"data":{"partial":true},"errors":[{"message":"wait","extensions":{"code":"THROTTLED"}}]}),
        json!({"errors":[{"message":"wait","extensions":{"code":"THROTTLED"}},{"message":"bad"}]}),
        json!({}),
    ] {
        let fixture = Fixture::new(vec![Reply::json(response)]).await;
        assert!(execute(&fixture).await.is_err());
        assert_eq!(fixture.requests().len(), 1);
    }
}
#[tokio::test]
async fn rejects_invalid_json_and_redirects_without_following_them() {
    let destination = Fixture::new(vec![Reply::json(json!({"data":{}}))]).await;
    let fixture = Fixture::new(vec![Reply {
        status: 302,
        body: String::new(),
        headers: vec![("location".into(), destination.url.to_string())],
    }])
    .await;
    assert!(matches!(
        execute(&fixture).await,
        Err(ClientError::Http { status: 302, .. })
    ));
    assert!(destination.requests().is_empty());
    let fixture = Fixture::new(vec![Reply {
        status: 200,
        body: "invalid".into(),
        headers: vec![],
    }])
    .await;
    assert!(execute(&fixture).await.is_err());
}
#[tokio::test]
async fn retries_only_explicit_throttling_and_keeps_request_identical() {
    for throttled in [
        Reply::status(429),
        Reply::json(json!({"errors":[{"message":"wait","extensions":{"code":"THROTTLED"}}]})),
    ] {
        let fixture = Fixture::new(vec![throttled, Reply::json(json!({"data":{"ok":true}}))]).await;
        assert_eq!(execute(&fixture).await.unwrap()["ok"], true);
        assert_eq!(fixture.requests().len(), 2);
        assert_eq!(fixture.requests()[0].2, fixture.requests()[1].2);
    }
}
#[tokio::test]
async fn excessive_retry_after_does_not_replay() {
    let fixture = Fixture::new(vec![Reply {
        status: 429,
        body: "{}".into(),
        headers: vec![("retry-after".into(), "60".into())],
    }])
    .await;
    assert!(matches!(
        execute(&fixture).await,
        Err(ClientError::RetryExhausted)
    ));
    assert_eq!(fixture.requests().len(), 1);
}
