#![allow(clippy::unwrap_used, clippy::needless_pass_by_value)]
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
pub(crate) struct Reply {
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
    methods: Arc<Mutex<Vec<Method>>>,
    raw_bodies: Arc<Mutex<Vec<Vec<u8>>>>,
}
pub(crate) struct Fixture {
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
            methods: Arc::new(Mutex::new(vec![])),
            raw_bodies: Arc::new(Mutex::new(vec![])),
        };
        let app = Router::new().fallback(respond).with_state(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { url, state, server }
    }
    pub fn raw_bodies(&self) -> Vec<Vec<u8>> {
        self.state.raw_bodies.lock().unwrap().clone()
    }
    pub fn methods(&self) -> Vec<Method> {
        self.state.methods.lock().unwrap().clone()
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
    state.methods.lock().unwrap().push(parts.method);
    let bytes = to_bytes(body, 1_000_000).await.unwrap();
    state.raw_bodies.lock().unwrap().push(bytes.to_vec());
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
    for status in [500, 502, 503, 504] {
        let mut reply =
            Reply::json(json!({"errors":[{"message":"wait","extensions":{"code":"THROTTLED"}}]}));
        reply.status = status;
        reply.headers.push(("retry-after".into(), "1".into()));
        let fixture = Fixture::new(vec![reply, Reply::json(json!({"data":{"ok":true}}))]).await;
        assert!(
            matches!(execute(&fixture).await.unwrap_err().kind(),ClientErrorKind::Http { status: actual,.. } if *actual==status)
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
        execute(&fixture).await.unwrap_err().kind(),
        ClientErrorKind::Http { status: 302, .. }
    ));
    assert_eq!(destination.requests().len(), 0);
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
        execute(&fixture).await.unwrap_err().kind(),
        ClientErrorKind::RetryExhausted
    ));
    assert_eq!(fixture.requests().len(), 1);
}

#[tokio::test]
async fn throttling_stops_after_four_attempts() {
    let fixture = Fixture::new(vec![Reply::status(429); 4]).await;
    assert!(matches!(
        execute(&fixture).await.unwrap_err().kind(),
        ClientErrorKind::RetryExhausted
    ));
    assert_eq!(fixture.requests().len(), 4);
}
#[tokio::test]
async fn retry_after_is_a_minimum_and_invalid_values_fall_back() {
    for header in ["2", "not-a-date"] {
        let fixture = Fixture::new(vec![
            Reply {
                status: 429,
                body: "{}".into(),
                headers: vec![("retry-after".into(), header.into())],
            },
            Reply::json(json!({"data":{}})),
        ])
        .await;
        let start = std::time::Instant::now();
        execute(&fixture).await.unwrap();
        assert!(start.elapsed() >= Duration::from_secs(if header == "2" { 2 } else { 1 }));
        assert_eq!(fixture.requests().len(), 2);
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        "retry-after",
        "Wed, 21 Oct 2099 07:28:00 GMT".parse().unwrap(),
    );
    assert!(retry_after(&headers).unwrap() > Duration::from_secs(30));
}
#[tokio::test]
async fn stalled_response_times_out_without_replay() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let gql = GraphqlClient::new(
        HttpClient::new(Duration::from_millis(50)).unwrap(),
        endpoint,
    );
    let start = std::time::Instant::now();
    let result: Result<Value, _> = gql
        .execute(
            HeaderMap::new(),
            &GraphqlRequest {
                query: "query { shop { name } }",
                operation_name: "Test",
                variables: json!({}),
            },
        )
        .await;
    assert!(matches!(
        result.unwrap_err().kind(),
        ClientErrorKind::Transport { .. }
    ));
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn typed_mutation_null_preserves_graphql_errors_before_decoding_data() {
    #[derive(serde::Deserialize)]
    struct RequiredData {
        #[serde(rename = "orderUpdate")]
        _order_update: bool,
    }
    let fixture = Fixture::new(vec![Reply::json(json!({
        "data":{"orderUpdate":null},
        "errors":[{"message":"Access denied","extensions":{"code":"ACCESS_DENIED"}}],
        "extensions":{"cost":{"requestedQueryCost":10}}
    }))])
    .await;
    let result: Result<RequiredData, ClientError> = graphql(&fixture)
        .execute(
            HeaderMap::new(),
            &GraphqlRequest {
                query: "mutation Test { orderUpdate { order { id } } }",
                operation_name: "Test",
                variables: json!({}),
            },
        )
        .await;
    match result.err().unwrap().kind() {
        ClientErrorKind::Graphql { errors, extensions } => {
            assert_eq!(errors[0].message, "Access denied");
            assert_eq!(extensions["cost"]["requestedQueryCost"], 10);
        }
        _ => panic!("GraphQL execution error was discarded"),
    }
    assert_eq!(fixture.requests().len(), 1);
}
#[tokio::test]
async fn oversized_retry_after_after_an_earlier_retry_does_not_panic() {
    let fixture = Fixture::new(vec![
        Reply::status(429),
        Reply {
            status: 429,
            body: "{}".into(),
            headers: vec![("retry-after".into(), u64::MAX.to_string())],
        },
    ])
    .await;
    assert!(matches!(
        execute(&fixture).await.unwrap_err().kind(),
        ClientErrorKind::RetryExhausted
    ));
    assert_eq!(fixture.requests().len(), 2);
}

#[tokio::test]
async fn http_4xx_retries_share_policy_between_json_and_graphql() {
    for status in [400, 401, 403, 404, 408, 409, 422, 429] {
        let fixture =
            Fixture::new(vec![Reply::status(status), Reply::json(json!({"ok":true}))]).await;
        let result: Value = HttpClient::new(Duration::from_secs(1))
            .unwrap()
            .request_json(
                Method::POST,
                fixture.url.clone(),
                HeaderMap::new(),
                Some(&json!({"same":"body"})),
            )
            .await
            .unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(fixture.requests().len(), 2);
        assert_eq!(fixture.requests()[0], fixture.requests()[1]);
        let fixture = Fixture::new(vec![
            Reply::status(status),
            Reply::json(json!({"data":{"ok":true}})),
        ])
        .await;
        assert_eq!(execute(&fixture).await.unwrap()["ok"], true);
        assert_eq!(fixture.requests().len(), 2);
        assert_eq!(fixture.requests()[0], fixture.requests()[1]);
    }
}
#[tokio::test]
async fn mixed_http_and_graphql_retries_have_one_four_attempt_budget() {
    let fixture = Fixture::new(vec![
        Reply::status(400),
        Reply::json(json!({"errors":[{"message":"wait","extensions":{"code":"THROTTLED"}}]})),
        Reply::status(429),
        Reply::status(403),
        Reply::json(json!({"data":{"unexpected":true}})),
    ])
    .await;
    assert!(matches!(
        execute(&fixture).await.unwrap_err().kind(),
        ClientErrorKind::RetryExhausted
    ));
    assert_eq!(fixture.requests().len(), 4);
}
#[tokio::test]
async fn generic_http_4xx_exhausts_four_attempts() {
    let fixture = Fixture::new(vec![Reply::status(401); 4]).await;
    let result: Result<Value, _> = HttpClient::new(Duration::from_secs(1))
        .unwrap()
        .request_json::<(), Value>(Method::GET, fixture.url.clone(), HeaderMap::new(), None)
        .await;
    assert!(matches!(
        result.unwrap_err().kind(),
        ClientErrorKind::RetryExhausted
    ));
    assert_eq!(fixture.requests().len(), 4);
}

#[test]
fn client_creation_and_propagation_preserve_the_first_location() {
    fn propagate(error: ClientError) -> Result<(), ClientError> {
        Err(error)
    }
    #[track_caller]
    fn protocol_failure() -> ClientError {
        ClientError::new(ClientErrorKind::Protocol("missing quantity"))
    }
    let line = line!() + 1;
    let error = ClientError::new(ClientErrorKind::Protocol("missing quantity"));
    assert_eq!(error.location().file(), file!());
    assert_eq!(error.location().line(), line);
    assert_eq!(error.location().column(), 17);
    let location = error.location();
    let error = propagate(error).unwrap_err();
    assert_eq!(error.location(), location);
    assert!(matches!(
        error.kind(),
        ClientErrorKind::Protocol("missing quantity")
    ));
    let line = line!() + 1;
    let error = protocol_failure();
    assert_eq!(error.location().file(), file!());
    assert_eq!(error.location().line(), line);
    assert_eq!(error.location().column(), 17);
}

#[tokio::test]
async fn asynchronous_client_conversion_keeps_the_conversion_site() {
    let mut line = 0;
    let result: Result<(), ClientError> = async {
        line = line!() + 1;
        serde_json::from_str::<Value>("{")?;
        Ok(())
    }
    .await;
    let error = result.unwrap_err();
    assert_eq!(error.location().file(), file!());
    assert_eq!(error.location().line(), line);
    assert!(error.location().column() > 0);
    assert!(matches!(error.kind(), ClientErrorKind::Decode { .. }));
}

#[test]
fn tracked_client_conversions_keep_the_source_and_calling_location() {
    use std::error::Error;
    #[track_caller]
    fn decode_failure(source: serde_json::Error) -> ClientError {
        ClientError::from(source)
    }
    let source = serde_json::from_str::<Value>("{").unwrap_err();
    let line = line!() + 1;
    let error = decode_failure(source);
    assert_eq!(error.location().file(), file!());
    assert_eq!(error.location().line(), line);
    assert_eq!(error.location().column(), 17);
    assert!(matches!(error.kind(), ClientErrorKind::Decode { .. }));
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<serde_json::Error>()
            .is_some()
    );
    let source = reqwest::Client::new()
        .get("invalid URL")
        .build()
        .unwrap_err();
    let line = line!() + 1;
    let error = ClientError::from(source);
    assert_eq!(error.location().file(), file!());
    assert_eq!(error.location().line(), line);
    assert_eq!(error.location().column(), 17);
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<reqwest::Error>()
            .is_some()
    );
}

#[tokio::test]
async fn serialization_failures_retain_the_original_transport_source_without_sending() {
    struct InvalidBody;
    impl serde::Serialize for InvalidBody {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("fixture serialization failure"))
        }
    }
    let fixture = Fixture::new(vec![]).await;
    let error = HttpClient::new(Duration::from_secs(1))
        .unwrap()
        .request_json::<_, Value>(
            Method::POST,
            fixture.url.clone(),
            HeaderMap::new(),
            Some(&InvalidBody),
        )
        .await
        .unwrap_err();
    assert!(matches!(error.kind(),ClientErrorKind::Transport {source} if source.is_builder()));
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(fixture.requests(), []);
}

#[tokio::test]
async fn one_shot_graphql_preserves_timeout_source_without_replay() {
    use tokio::io::AsyncReadExt;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 8192];
        assert!(stream.read(&mut bytes).await.unwrap() > 0);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    });
    let result: Result<Value, ClientError> = GraphqlClient::new(
        HttpClient::new(Duration::from_millis(50)).unwrap(),
        endpoint,
    )
    .execute_once(
        HeaderMap::new(),
        &GraphqlRequest {
            query: "mutation Test { draftOrderCreate { draftOrder { id } } }",
            operation_name: "Test",
            variables: json!({}),
        },
    )
    .await;
    let error = result.unwrap_err();
    assert!(matches!(error.kind(),ClientErrorKind::Transport {source} if source.is_timeout()));
    assert!(std::error::Error::source(&error).is_some());
    server.await.unwrap();
}

#[tokio::test]
async fn one_shot_graphql_preserves_disconnect_source_without_replay() {
    use tokio::io::AsyncReadExt;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 8192];
        assert!(stream.read(&mut bytes).await.unwrap() > 0);
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let result: Result<Value, ClientError> =
        GraphqlClient::new(HttpClient::new(Duration::from_secs(1)).unwrap(), endpoint)
            .execute_once(
                HeaderMap::new(),
                &GraphqlRequest {
                    query: "mutation Test { draftOrderCreate { draftOrder { id } } }",
                    operation_name: "Test",
                    variables: json!({}),
                },
            )
            .await;
    let error = result.unwrap_err();
    assert!(matches!(error.kind(),ClientErrorKind::Transport {source} if !source.is_timeout()));
    assert!(std::error::Error::source(&error).is_some());
    server.await.unwrap();
}
