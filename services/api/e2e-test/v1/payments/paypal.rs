#![allow(clippy::unwrap_used)]
use crate::common::{
    ProviderConfig, TestClient, TestFixture, body_json, client_with_providers, fixture,
};
use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;

pub(crate) const CAP: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
pub(crate) const OTHER: &str = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBA";
const ROOT: &str = "/api/v1/payments/paypal";

#[derive(Default)]
struct ProviderState {
    calls: Vec<(String, String, HeaderMap, Value)>,
    orders: HashMap<String, Value>,
    complete: bool,
    capture_status: Option<String>,
    capture_ids: HashMap<String, String>,
    patches: Vec<(String, Value)>,
    fault: Option<(String, String, usize)>,
    signature: Option<String>,
}
pub(crate) struct PaypalProvider {
    state: Arc<Mutex<ProviderState>>,
    server: tokio::task::JoinHandle<()>,
    pub(crate) url: String,
}
impl PaypalProvider {
    pub(crate) async fn start() -> Self {
        let state = Arc::new(Mutex::new(ProviderState::default()));
        let router = axum::Router::new()
            .fallback(any(provider_request))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self { state, server, url }
    }
    pub(crate) async fn client(&self) -> TestClient {
        client_with_providers(ProviderConfig {
            paypal_url: Some(&self.url),
            paypal_payment: Some(true),
            paypal_timeout_secs: Some(1),
            ..Default::default()
        })
        .await
    }
    pub(crate) fn calls(&self) -> Vec<(String, String, HeaderMap, Value)> {
        self.state.lock().unwrap().calls.clone()
    }
    pub(crate) fn fault(&self, path: &str, kind: &str) {
        self.state.lock().unwrap().fault = Some((path.into(), kind.into(), 0));
    }
    fn fault_after(&self, path: &str, kind: &str, skip: usize) {
        self.state.lock().unwrap().fault = Some((path.into(), kind.into(), skip));
    }
    pub(crate) fn clear_fault(&self) {
        self.state.lock().unwrap().fault = None;
    }
    pub(crate) fn completed(&self) {
        self.state.lock().unwrap().complete = true;
    }
    pub(crate) fn signature(&self, value: &str) {
        self.state.lock().unwrap().signature = Some(value.into());
    }
    pub(crate) fn stop(&self) {
        self.server.abort();
    }
    pub(crate) fn capture_status(&self, value: &str) {
        self.state.lock().unwrap().capture_status = Some(value.into());
    }
    pub(crate) fn capture_identity(&self, order: &str, capture: &str) {
        self.state
            .lock()
            .unwrap()
            .capture_ids
            .insert(order.into(), capture.into());
    }
    fn patch(&self, path: &str, value: Value) {
        self.state
            .lock()
            .unwrap()
            .patches
            .push((path.into(), value));
    }
}
impl Drop for PaypalProvider {
    fn drop(&mut self) {
        self.server.abort();
    }
}

pub(crate) fn request(
    method: &str,
    path: &str,
    cap: Option<&str>,
    body: Option<&Value>,
) -> Request<Body> {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(cap) = cap {
        request = request.header("authorization", format!("Bearer {cap}"));
    }
    let body = if let Some(body) = body {
        request = request.header("content-type", "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };
    request.body(body).unwrap()
}
pub(crate) fn input(fixture: &TestFixture) -> Value {
    json!({"idempotency_key":Uuid::new_v4().to_string(),"reference":format!("{}-{}",fixture.prefix(),Uuid::new_v4()),"amount":"12.34","currency":"USD"})
}
pub(crate) async fn create(client: &TestClient, input: &Value) -> Value {
    let response = client
        .send(request("POST", ROOT, Some(CAP), Some(input)))
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    body_json(response).await
}
pub(crate) async fn call(client: &TestClient, method: &str, id: &str) -> Response {
    client
        .send(request(
            method,
            &format!(
                "{ROOT}/{id}{}",
                if method == "POST" { "/capture" } else { "" }
            ),
            Some(CAP),
            None,
        ))
        .await
        .unwrap()
}

#[tokio::test]
async fn paypal_create_capture_replay_and_owned_status_use_real_connections() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let input = input(&fixture);
    let created = create(&client, &input).await;
    assert_eq!(created["status"], "AWAITING_APPROVAL");
    assert_eq!(created["amount"], "12.34");
    assert!(
        created["approval_url"]
            .as_str()
            .unwrap()
            .starts_with("https://www.sandbox.paypal.com/")
    );
    let id = created["id"].as_str().unwrap();
    let replay = client
        .send(request("POST", ROOT, Some(CAP), Some(&input)))
        .await
        .unwrap();
    assert_eq!(replay.status(), 200);
    assert_eq!(body_json(replay).await["id"], id);
    let mut changed = input.clone();
    changed["amount"] = json!("12.35");
    assert_eq!(
        client
            .send(request("POST", ROOT, Some(CAP), Some(&changed)))
            .await
            .unwrap()
            .status(),
        409
    );
    assert_eq!(
        client
            .send(request("POST", ROOT, Some(OTHER), Some(&input)))
            .await
            .unwrap()
            .status(),
        404
    );
    let got = call(&client, "GET", id).await;
    assert_eq!(got.status(), 200);
    assert_eq!(body_json(got).await["status"], "AWAITING_APPROVAL");
    assert!(
        !provider
            .calls()
            .iter()
            .any(|(method, path, _, _)| method == "POST" && path.ends_with("/capture"))
    );
    let captured = call(&client, "POST", id).await;
    assert_eq!(captured.status(), 200);
    assert_eq!(body_json(captured).await["status"], "COMPLETED");
    let count = provider.calls().len();
    assert_eq!(call(&client, "POST", id).await.status(), 200);
    assert_eq!(call(&client, "GET", id).await.status(), 200);
    assert_eq!(provider.calls().len(), count);
    let (digest, status, capture_count): (Vec<u8>, String, i64) = sqlx::query_as("SELECT capability_digest,status,(SELECT count(*) FROM paypal_payment_captures c WHERE c.payment_id=p.id) FROM paypal_payments p WHERE id=$1").bind(Uuid::parse_str(id).unwrap()).fetch_one(&fixture.pool()).await.unwrap();
    assert_eq!(digest.len(), 32);
    assert_ne!(digest, CAP.as_bytes());
    assert_eq!(status, "COMPLETED");
    assert_eq!(capture_count, 1);
    let calls = provider.calls();
    let (_, _, headers, payload) = calls
        .iter()
        .find(|(m, p, _, _)| m == "POST" && p == "/v2/checkout/orders")
        .unwrap();
    assert_eq!(headers["prefer"], "return=representation");
    assert!(Uuid::parse_str(headers["paypal-request-id"].to_str().unwrap()).is_ok());
    assert_eq!(payload["intent"], "CAPTURE");
    assert_eq!(payload["purchase_units"][0]["reference_id"], id);
    assert_eq!(
        payload["purchase_units"][0]["custom_id"],
        input["reference"]
    );
    assert_eq!(
        payload["purchase_units"][0]["payee"]["merchant_id"],
        "synthetic-merchant"
    );
    assert_eq!(
        payload["payment_source"]["paypal"]["experience_context"]["return_url"],
        "https://example.test/paypal/return"
    );
    assert_eq!(
        calls
            .iter()
            .filter(|(_, p, _, _)| p == "/v1/oauth2/token")
            .count(),
        1
    );
}

#[tokio::test]
async fn paypal_request_validation_and_ownership_stop_before_upstream() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let valid = input(&fixture);
    for field in ["idempotency_key", "reference", "amount", "currency"] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert_eq!(
            client
                .send(request("POST", ROOT, Some(CAP), Some(&missing)))
                .await
                .unwrap()
                .status(),
            400,
            "missing {field}"
        );
        let mut wrong_type = valid.clone();
        wrong_type[field] = json!(123);
        assert_eq!(
            client
                .send(request("POST", ROOT, Some(CAP), Some(&wrong_type)))
                .await
                .unwrap()
                .status(),
            400,
            "wrong type {field}"
        );
    }
    for (field, value) in [
        ("idempotency_key", json!("invalid")),
        ("reference", json!("")),
        ("reference", json!(" \t")),
        ("reference", json!("hello\nworld")),
        ("reference", json!("x".repeat(128))),
        ("amount", json!("0")),
        ("amount", json!("-1")),
        ("amount", json!("01")),
        ("amount", json!("+1")),
        ("amount", json!("1e2")),
        ("amount", json!("1.001")),
        ("amount", json!("1.")),
        ("amount", json!(".5")),
        ("amount", json!(" 1")),
        ("amount", json!("92233720368547758.08")),
        ("amount", json!("9".repeat(33))),
        ("currency", json!("usd")),
        ("currency", json!("EUR")),
        ("unknown", json!(true)),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        assert_eq!(
            client
                .send(request("POST", ROOT, Some(CAP), Some(&invalid)))
                .await
                .unwrap()
                .status(),
            400,
            "{invalid}"
        );
    }
    let mut jpy = valid.clone();
    jpy["currency"] = json!("JPY");
    jpy["amount"] = json!("1.0");
    assert_eq!(
        client
            .send(request("POST", ROOT, Some(CAP), Some(&jpy)))
            .await
            .unwrap()
            .status(),
        400
    );
    let invalid_json = Request::builder()
        .method("POST")
        .uri(ROOT)
        .header("authorization", format!("Bearer {CAP}"))
        .header("content-type", "application/json")
        .body(Body::from("{"))
        .unwrap();
    assert_eq!(client.send(invalid_json).await.unwrap().status(), 400);
    assert_eq!(provider.calls().len(), 0);
}

#[tokio::test]
async fn paypal_money_boundaries_and_zero_decimal_currency_are_accepted() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    for (amount, currency, normalized) in [
        ("0.01", "USD", "0.01"),
        ("1.2", "SGD", "1.20"),
        ("1", "JPY", "1"),
        ("92233720368547758.07", "USD", "92233720368547758.07"),
    ] {
        let mut value = input(&fixture);
        value["amount"] = json!(amount);
        value["currency"] = json!(currency);
        assert_eq!(create(&client, &value).await["amount"], normalized);
    }
}

#[tokio::test]
async fn paypal_dependency_failures_are_safe_and_capture_loss_recovers() {
    let fixture = fixture().await;
    for (operation, path) in [
        ("oauth", "/v1/oauth2/token"),
        ("create", "/v2/checkout/orders"),
        ("order", "/v2/checkout/orders/"),
        ("capture", "/capture"),
        ("evidence", "/v2/payments/captures/"),
    ] {
        for fault in ["http", "malformed", "timeout", "disconnect"] {
            let provider = PaypalProvider::start().await;
            let client = provider.client().await;
            let input = input(&fixture);
            let created = if matches!(operation, "oauth" | "create") {
                None
            } else {
                Some(create(&client, &input).await)
            };
            provider.fault(path, fault);
            let response = if let Some(created) = &created {
                call(&client, "POST", created["id"].as_str().unwrap()).await
            } else {
                client
                    .send(request("POST", ROOT, Some(CAP), Some(&input)))
                    .await
                    .unwrap()
            };
            let status = response.status();
            let body = body_json(response).await;
            let expected = if matches!(operation, "oauth" | "create") && fault != "http" {
                202
            } else if fault == "timeout" {
                504
            } else {
                502
            };
            assert_eq!(status, expected, "{operation}/{fault}: {body}");
            assert!(!body.to_string().contains("private"));
            assert_ne!(body["status"], "COMPLETED");
            if let Some(created) = &created {
                let verified: Option<String> = sqlx::query_scalar(
                    "SELECT verified_capture_id FROM paypal_payments WHERE id=$1",
                )
                .bind(Uuid::parse_str(created["id"].as_str().unwrap()).unwrap())
                .fetch_one(&fixture.pool())
                .await
                .unwrap();
                assert!(verified.is_none());
            }
            provider.clear_fault();
        }
    }
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let input = input(&fixture);
    let created = create(&client, &input).await;
    let id = created["id"].as_str().unwrap();
    provider.fault("/capture", "lost");
    assert_eq!(call(&client, "POST", id).await.status(), 502);
    provider.clear_fault();
    assert_eq!(call(&client, "POST", id).await.status(), 200);
    assert_eq!(
        provider
            .calls()
            .iter()
            .filter(|(m, p, _, _)| m == "POST" && p.ends_with("/capture"))
            .count(),
        1
    );
}

#[tokio::test]
async fn paypal_capture_evidence_mismatches_never_verify_payment() {
    let fixture = fixture().await;
    for (path, value) in [
        ("/amount/value", json!("1.00")),
        ("/amount/currency_code", json!("SGD")),
        ("/payee/merchant_id", json!("another-merchant")),
        ("/custom_id", json!("another-reference")),
        (
            "/supplementary_data/related_ids/order_id",
            json!("another-order"),
        ),
        (
            "/supplementary_data/related_ids/capture_id",
            json!("another-capture"),
        ),
        ("/final_capture", json!(false)),
        ("/final_capture", Value::Null),
        ("/id", json!("another-capture")),
        ("/status", json!("REFUNDED")),
    ] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        let created = create(&client, &input(&fixture)).await;
        let id = created["id"].as_str().unwrap();
        provider.patch(path, value);
        let response = call(&client, "POST", id).await;
        assert_eq!(response.status(), 502, "{path}");
        let (verified,count):(Option<String>,i64)=sqlx::query_as("SELECT verified_capture_id,(SELECT count(*) FROM paypal_payment_captures WHERE payment_id=p.id) FROM paypal_payments p WHERE id=$1").bind(Uuid::parse_str(id).unwrap()).fetch_one(&fixture.pool()).await.unwrap();
        assert!(verified.is_none());
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn paypal_pending_denied_concurrent_and_duplicate_reference_are_retained() {
    let fixture = fixture().await;
    for state in ["PENDING", "DENIED", "DECLINED", "FAILED"] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        let created = create(&client, &input(&fixture)).await;
        provider.capture_status(state);
        let response = call(&client, "POST", created["id"].as_str().unwrap()).await;
        if state == "PENDING" {
            assert_eq!(response.status(), 202);
            assert_eq!(body_json(response).await["status"], state);
        } else {
            assert_eq!(response.status(), 409);
            let status = call(&client, "GET", created["id"].as_str().unwrap()).await;
            assert_eq!(status.status(), 200);
            assert_eq!(body_json(status).await["status"], "DENIED");
        }
    }
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let first_input = input(&fixture);
    let first = create(&client, &first_input).await;
    let id = first["id"].as_str().unwrap();
    let (left, right) = tokio::join!(call(&client, "POST", id), call(&client, "POST", id));
    assert!([200, 202].contains(&left.status().as_u16()));
    assert!([200, 202].contains(&right.status().as_u16()));
    assert_eq!(
        provider
            .calls()
            .iter()
            .filter(|(m, p, _, _)| m == "POST" && p.ends_with("/capture"))
            .count(),
        1
    );
    let mut second_input = first_input.clone();
    second_input["idempotency_key"] = json!(Uuid::new_v4().to_string());
    let second = create(&client, &second_input).await;
    let second_result = call(&client, "POST", second["id"].as_str().unwrap()).await;
    assert_eq!(second_result.status(), 200);
    assert_eq!(
        body_json(second_result).await["exception"],
        "DUPLICATE_SUCCESSFUL_REFERENCE"
    );
    let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT status,exception,verified_capture_id FROM paypal_payments WHERE reference=$1",
    )
    .bind(first_input["reference"].as_str().unwrap())
    .fetch_all(&fixture.pool())
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|(_,exception,_)|exception.as_deref()==Some("DUPLICATE_SUCCESSFUL_REFERENCE")));
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM paypal_payment_captures WHERE payment_id IN (SELECT id FROM paypal_payments WHERE reference=$1)").bind(first_input["reference"].as_str().unwrap()).fetch_one(&fixture.pool()).await.unwrap();
    assert_eq!(count, 2);
}

async fn provider_request(
    State(state): State<Arc<Mutex<ProviderState>>>,
    method: axum::http::Method,
    uri: axum::http::Uri,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    let path = uri.path();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let fault = {
        let mut state = state.lock().unwrap();
        state.calls.push((
            method.to_string(),
            path.into(),
            headers.clone(),
            body.clone(),
        ));
        state.fault.as_mut().and_then(|(target, kind, skip)| {
            let (expected_method, target) = target
                .split_once(' ')
                .map_or((None, target.as_str()), |(method, path)| {
                    (Some(method), path)
                });
            if !path.contains(target)
                || expected_method.is_some_and(|expected| expected != method.as_str())
            {
                return None;
            }
            if *skip > 0 {
                *skip -= 1;
                return None;
            }
            Some(kind.clone())
        })
    };
    if let Some(fault) = fault {
        return provider_failure(&state, &fault).await;
    }
    let mut state = state.lock().unwrap();
    if path == "/v1/oauth2/token" {
        assert!(
            headers["authorization"]
                .to_str()
                .unwrap()
                .starts_with("Basic ")
        );
        return axum::Json(
            json!({"access_token":"synthetic-token","expires_in":3600,"token_type":"Bearer"}),
        )
        .into_response();
    }
    assert_eq!(headers["authorization"], "Bearer synthetic-token");
    if path == "/v1/notifications/verify-webhook-signature" {
        assert_eq!(body["webhook_id"], "synthetic-webhook");
        return axum::Json(
            json!({"verification_status":state.signature.as_deref().unwrap_or("SUCCESS")}),
        )
        .into_response();
    }
    if method == axum::http::Method::POST && path == "/v2/checkout/orders" {
        return axum::Json(created_order(&mut state, &body)).into_response();
    }
    let id = path
        .trim_start_matches("/v2/checkout/orders/")
        .trim_end_matches("/capture");
    if path.starts_with("/v2/checkout/orders/") {
        if method == axum::http::Method::POST {
            state.complete = true;
            return axum::Json(json!({"id":id,"status":"COMPLETED"})).into_response();
        }
        let mut order = state.orders[id].clone();
        order["status"] = json!(if state.complete {
            "COMPLETED"
        } else {
            "APPROVED"
        });
        if state.complete {
            order["purchase_units"][0]["payments"] = json!({"captures":[capture_for(&state,id)]});
        }
        for (pointer, value) in &state.patches {
            if let Some(pointer) = pointer.strip_prefix("/order")
                && let Some(target) = order.pointer_mut(pointer)
            {
                *target = value.clone();
            }
        }
        return axum::Json(order).into_response();
    }
    if let Some(capture_id) = path.strip_prefix("/v2/payments/captures/") {
        return retrieved_capture(&state, capture_id);
    }
    (StatusCode::NOT_FOUND, "unexpected provider path").into_response()
}
fn capture_for(state: &ProviderState, id: &str) -> Value {
    let unit = &state.orders[id]["purchase_units"][0];
    let capture_id = state
        .capture_ids
        .get(id)
        .cloned()
        .unwrap_or_else(|| format!("CAP-{id}"));
    json!({"id":capture_id,"status":state.capture_status.as_deref().unwrap_or("COMPLETED"),"amount":unit["amount"],"final_capture":true,"payee":unit["payee"],"custom_id":unit["custom_id"],"supplementary_data":{"related_ids":{"order_id":id,"capture_id":capture_id}}})
}

#[tokio::test]
async fn paypal_expired_ambiguous_operations_never_issue_fresh_mutations() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let input = input(&fixture);
    provider.fault("/v2/checkout/orders", "malformed");
    let response = client
        .send(request("POST", ROOT, Some(CAP), Some(&input)))
        .await
        .unwrap();
    assert_eq!(response.status(), 202);
    let created = body_json(response).await;
    let id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE paypal_payments SET create_started_at=clock_timestamp()-interval '7 hours' WHERE id=$1").bind(id).execute(&fixture.pool()).await.unwrap();
    provider.clear_fault();
    let count = provider.calls().len();
    let response = client
        .send(request("POST", ROOT, Some(CAP), Some(&input)))
        .await
        .unwrap();
    assert_eq!(response.status(), 202);
    assert_eq!(body_json(response).await["exception"], "UNRESOLVED_CREATE");
    assert_eq!(provider.calls().len(), count);
    let created = create(&client, &self::input(&fixture)).await;
    let id = created["id"].as_str().unwrap();
    sqlx::query("UPDATE paypal_payments SET capture_started_at=clock_timestamp()-interval '7 hours' WHERE id=$1").bind(Uuid::parse_str(id).unwrap()).execute(&fixture.pool()).await.unwrap();
    let count = provider.calls().len();
    assert_eq!(call(&client, "POST", id).await.status(), 409);
    assert!(
        provider.calls()[count..]
            .iter()
            .all(|(method, _, _, _)| method == "GET")
    );
}

#[tokio::test]
async fn paypal_capture_without_optional_redundant_fields_is_verified() {
    let fixture = fixture().await;
    for path in ["/payee", "/custom_id", "/supplementary_data"] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        let created = create(&client, &input(&fixture)).await;
        provider.patch(path, Value::Null);
        let response = call(&client, "POST", created["id"].as_str().unwrap()).await;
        assert_eq!(response.status(), 200);
        assert_eq!(body_json(response).await["status"], "COMPLETED");
    }
}

pub(crate) struct DatabaseFault {
    url: String,
    name: String,
    table: &'static str,
}
impl Drop for DatabaseFault {
    fn drop(&mut self) {
        let url = self.url.clone();
        let name = self.name.clone();
        let table = self.table;
        std::thread::spawn(move||tokio::runtime::Runtime::new().unwrap().block_on(async move {
            let pool=sqlx::PgPool::connect(&url).await.unwrap();
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP TRIGGER IF EXISTS {name} ON {table}; DROP FUNCTION IF EXISTS {name}();"))).execute(&pool).await.unwrap();
        })).join().unwrap();
    }
}
pub(crate) async fn database_fault(
    fixture: &TestFixture,
    table: &'static str,
    operation: &str,
    condition: &str,
) -> DatabaseFault {
    let name = format!("{}_paypal_fault", fixture.prefix());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF {condition} THEN RAISE EXCEPTION 'private synthetic payment database failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER {name} BEFORE {operation} ON {table} FOR EACH ROW EXECUTE FUNCTION {name}();"))).execute(&fixture.pool()).await.unwrap();
    DatabaseFault {
        url: crate::common::database_url(),
        name,
        table,
    }
}

#[tokio::test]
async fn paypal_database_writes_fail_without_success_or_partial_verification() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let body = input(&fixture);
    let condition = format!("NEW.reference = '{}'", body["reference"].as_str().unwrap());
    let fault = database_fault(&fixture, "paypal_payments", "INSERT", &condition).await;
    let response = client
        .send(request("POST", ROOT, Some(CAP), Some(&body)))
        .await
        .unwrap();
    assert_eq!(response.status(), 500);
    assert!(!body_json(response).await.to_string().contains("private"));
    assert_eq!(provider.calls().len(), 0);
    drop(fault);
    let another = input(&fixture);
    let condition = format!(
        "NEW.reference = '{}' AND NEW.paypal_order_id IS NOT NULL",
        another["reference"].as_str().unwrap()
    );
    let fault = database_fault(&fixture, "paypal_payments", "UPDATE", &condition).await;
    assert_eq!(
        client
            .send(request("POST", ROOT, Some(CAP), Some(&another)))
            .await
            .unwrap()
            .status(),
        500
    );
    drop(fault);
    let created = create(&client, &body).await;
    let id = created["id"].as_str().unwrap();
    let condition = format!("NEW.id = '{id}'::uuid AND NEW.operation IS NOT NULL");
    let fault = database_fault(&fixture, "paypal_payments", "UPDATE", &condition).await;
    let count = provider.calls().len();
    assert_eq!(call(&client, "GET", id).await.status(), 500);
    assert_eq!(provider.calls().len(), count);
    drop(fault);
    let condition = format!("NEW.payment_id = '{id}'::uuid");
    let fault = database_fault(&fixture, "paypal_payment_captures", "INSERT", &condition).await;
    assert_eq!(call(&client, "POST", id).await.status(), 500);
    drop(fault);
    let verified: Option<String> =
        sqlx::query_scalar("SELECT verified_capture_id FROM paypal_payments WHERE id=$1")
            .bind(Uuid::parse_str(id).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert!(verified.is_none());
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM paypal_payment_captures WHERE payment_id=$1")
            .bind(Uuid::parse_str(id).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn paypal_unapproved_and_invalid_order_evidence_never_capture() {
    let fixture = fixture().await;
    for (pointer, value, status) in [
        ("/order/status", json!("CREATED"), 409),
        ("/order/status", json!("VOIDED"), 409),
        ("/order/id", json!("wrong-order"), 502),
        ("/order/purchase_units", json!([]), 502),
        (
            "/order/purchase_units/0/reference_id",
            json!("wrong-payment"),
            502,
        ),
        ("/order/purchase_units/0/custom_id", Value::Null, 502),
        ("/order/purchase_units/0/payee", Value::Null, 502),
        ("/order/purchase_units/0/amount/value", json!("0.01"), 502),
        (
            "/order/purchase_units/0/amount/currency_code",
            json!("SGD"),
            502,
        ),
    ] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        let created = create(&client, &input(&fixture)).await;
        provider.patch(pointer, value);
        let count = provider.calls().len();
        assert_eq!(
            call(&client, "POST", created["id"].as_str().unwrap())
                .await
                .status(),
            status,
            "{pointer}"
        );
        assert!(
            provider.calls()[count..]
                .iter()
                .all(|(method, _, _, _)| method == "GET")
        );
    }
}

#[tokio::test]
async fn paypal_transport_connection_refusal_never_returns_completed() {
    let fixture = fixture().await;
    for create_first in [false, true] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        let body = input(&fixture);
        let created = if create_first {
            Some(create(&client, &body).await)
        } else {
            None
        };
        provider.stop();
        tokio::task::yield_now().await;
        let response = if let Some(created) = created {
            call(&client, "POST", created["id"].as_str().unwrap()).await
        } else {
            client
                .send(request("POST", ROOT, Some(CAP), Some(&body)))
                .await
                .unwrap()
        };
        assert_eq!(response.status(), if create_first { 502 } else { 202 });
        assert_ne!(body_json(response).await["status"], "COMPLETED");
    }
}

#[tokio::test]
async fn paypal_reporting_only_configuration_disables_all_gateway_operations() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = client_with_providers(ProviderConfig {
        paypal_url: Some(&provider.url),
        ..Default::default()
    })
    .await;
    for (method, path, body) in [
        ("POST", ROOT.to_owned(), Some(input(&fixture))),
        ("GET", format!("{ROOT}/{}", Uuid::new_v4()), None),
        ("POST", format!("{ROOT}/{}/capture", Uuid::new_v4()), None),
    ] {
        assert_eq!(
            client
                .send(request(method, &path, Some(CAP), body.as_ref()))
                .await
                .unwrap()
                .status(),
            503
        );
    }
    assert_eq!(provider.calls().len(), 0);
}

#[tokio::test]
async fn paypal_ambiguous_retry_reuses_stable_provider_request_ids() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let body = input(&fixture);
    provider.fault("/v2/checkout/orders", "malformed");
    assert_eq!(
        client
            .send(request("POST", ROOT, Some(CAP), Some(&body)))
            .await
            .unwrap()
            .status(),
        202
    );
    provider.clear_fault();
    let response = client
        .send(request("POST", ROOT, Some(CAP), Some(&body)))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let created = body_json(response).await;
    let calls = provider.calls();
    let create_requests = calls
        .iter()
        .filter(|(m, p, _, _)| m == "POST" && p == "/v2/checkout/orders")
        .collect::<Vec<_>>();
    assert_eq!(create_requests.len(), 2);
    assert_eq!(
        create_requests[0].2["paypal-request-id"],
        create_requests[1].2["paypal-request-id"]
    );
    assert_eq!(create_requests[0].3, create_requests[1].3);
    provider.fault("/capture", "http");
    let id = created["id"].as_str().unwrap();
    assert_eq!(call(&client, "POST", id).await.status(), 502);
    provider.clear_fault();
    assert_eq!(call(&client, "POST", id).await.status(), 200);
    let calls = provider.calls();
    let captures = calls
        .iter()
        .filter(|(m, p, _, _)| m == "POST" && p.ends_with("/capture"))
        .collect::<Vec<_>>();
    assert_eq!(captures.len(), 2);
    assert_eq!(
        captures[0].2["paypal-request-id"],
        captures[1].2["paypal-request-id"]
    );
    let first_capture = calls
        .iter()
        .position(|(m, p, _, _)| m == "POST" && p.ends_with("/capture"))
        .unwrap();
    assert_eq!(calls[first_capture + 1].0, "GET");
}

#[tokio::test]
async fn paypal_oauth_semantic_protocol_failure_returns_safe_gateway_error() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    provider.fault("/v1/oauth2/token", "protocol");
    let response = client
        .send(request("POST", ROOT, Some(CAP), Some(&input(&fixture))))
        .await
        .unwrap();
    assert_eq!(response.status(), 502);
    assert_eq!(
        body_json(response).await,
        json!({"error":"Upstream request failed"})
    );
    assert_eq!(provider.calls().len(), 1);
}

#[tokio::test]
async fn paypal_post_capture_authoritative_order_failure_is_not_success() {
    let fixture = fixture().await;
    for kind in ["http", "malformed", "timeout"] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        let created = create(&client, &input(&fixture)).await;
        let id = created["id"].as_str().unwrap();
        provider.fault_after("GET /v2/checkout/orders/", kind, 1);
        let response = call(&client, "POST", id).await;
        assert_eq!(response.status(), if kind == "timeout" { 504 } else { 502 });
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM paypal_payment_captures WHERE payment_id=$1")
                .bind(Uuid::parse_str(id).unwrap())
                .fetch_one(&fixture.pool())
                .await
                .unwrap();
        assert_eq!(count, 0);
        provider.clear_fault();
        assert_eq!(call(&client, "POST", id).await.status(), 200);
        assert_eq!(
            provider
                .calls()
                .iter()
                .filter(|(m, p, _, _)| m == "POST" && p.ends_with("/capture"))
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn paypal_capability_validation_rejects_before_upstream() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let valid = input(&fixture);
    for cap in [
        None,
        Some("invalid"),
        Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB"),
        Some(""),
    ] {
        let response = client
            .send(request("POST", ROOT, cap, Some(&valid)))
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
    }
    let mut duplicate = request("POST", ROOT, Some(CAP), Some(&valid));
    duplicate
        .headers_mut()
        .append("authorization", format!("Bearer {CAP}").parse().unwrap());
    assert_eq!(client.send(duplicate).await.unwrap().status(), 401);
    let mut wrong_scheme = request("POST", ROOT, None, Some(&valid));
    wrong_scheme
        .headers_mut()
        .insert("authorization", "Basic synthetic".parse().unwrap());
    assert_eq!(client.send(wrong_scheme).await.unwrap().status(), 401);
    assert_eq!(provider.calls().len(), 0);
}
#[tokio::test]
async fn paypal_payment_ids_and_capabilities_enforce_ownership() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let valid = input(&fixture);
    let created = create(&client, &valid).await;
    let id = created["id"].as_str().unwrap();
    let count = provider.calls().len();
    for method in ["GET", "POST"] {
        for cap in [None, Some(OTHER), Some("invalid")] {
            let path = format!(
                "{ROOT}/{id}{}",
                if method == "POST" { "/capture" } else { "" }
            );
            assert_eq!(
                client
                    .send(request(method, &path, cap, None))
                    .await
                    .unwrap()
                    .status(),
                404
            );
        }
        assert_eq!(call(&client, method, "not-a-uuid").await.status(), 400);
        assert_eq!(
            call(&client, method, &Uuid::new_v4().to_string())
                .await
                .status(),
            404
        );
    }
    for body in [
        json!({}),
        json!({"amount":"12.34"}),
        json!({"paypal_order_id":"forged"}),
    ] {
        assert_eq!(
            client
                .send(request(
                    "POST",
                    &format!("{ROOT}/{id}/capture"),
                    Some(CAP),
                    Some(&body)
                ))
                .await
                .unwrap()
                .status(),
            400
        );
    }
    assert_eq!(provider.calls().len(), count);
}

fn created_order(state: &mut ProviderState, body: &Value) -> Value {
    let id = format!(
        "ORDER-{}",
        body["purchase_units"][0]["reference_id"].as_str().unwrap()
    );
    let mut order = json!({"id":id,"status":"CREATED","purchase_units":body["purchase_units"],"links":[{"rel":"payer-action","href":format!("https://www.sandbox.paypal.com/checkoutnow?token={id}")}]});
    for (pointer, value) in &state.patches {
        if let Some(pointer) = pointer.strip_prefix("/create") {
            *order.pointer_mut(pointer).unwrap() = value.clone();
        }
    }
    state.orders.insert(id, order.clone());
    order
}

async fn provider_failure(state: &Arc<Mutex<ProviderState>>, fault: &str) -> Response {
    match fault {
        "timeout" => {
            tokio::time::sleep(Duration::from_secs(2)).await;
            (StatusCode::OK, "{}").into_response()
        }
        "malformed" => (StatusCode::OK, "{").into_response(),
        "disconnect" => Response::new(Body::new(http_body_util::Limited::new(
            http_body_util::Full::new(Bytes::from_static(b"broken")),
            1,
        ))),
        "protocol" => {
            axum::Json(json!({"access_token":"", "token_type":"Bearer", "expires_in":3600}))
                .into_response()
        }
        "lost" => {
            state.lock().unwrap().complete = true;
            (StatusCode::BAD_GATEWAY, "private lost response").into_response()
        }
        _ => (StatusCode::BAD_GATEWAY, "private provider body").into_response(),
    }
}

#[tokio::test]
async fn paypal_status_dependency_failures_return_safe_errors_without_verification() {
    let fixture = fixture().await;
    for path in [
        "/v1/oauth2/token",
        "/v2/checkout/orders/",
        "/v2/payments/captures/",
    ] {
        for fault in ["http", "malformed", "timeout", "disconnect"] {
            let provider = PaypalProvider::start().await;
            let client = provider.client().await;
            let created = create(&client, &input(&fixture)).await;
            provider.completed();
            let fresh = provider.client().await;
            provider.fault(path, fault);
            let id = created["id"].as_str().unwrap();
            let response = call(&fresh, "GET", id).await;
            assert_eq!(
                response.status(),
                if fault == "timeout" { 504 } else { 502 },
                "{path}/{fault}"
            );
            assert_eq!(
                body_json(response).await,
                json!({"error":"Upstream request failed"})
            );
            assert!(
                !provider
                    .calls()
                    .iter()
                    .any(|(method, path, _, _)| method == "POST" && path.ends_with("/capture"))
            );
            let verified: Option<String> =
                sqlx::query_scalar("SELECT verified_capture_id FROM paypal_payments WHERE id=$1")
                    .bind(Uuid::parse_str(id).unwrap())
                    .fetch_one(&fixture.pool())
                    .await
                    .unwrap();
            assert!(verified.is_none());
        }
    }
}

#[tokio::test]
async fn paypal_isolated_database_read_failure_returns_safe_errors_before_provider_calls() {
    let fixture = fixture().await;
    let provider = PaypalProvider::start().await;
    let client = provider.client().await;
    let created = create(&client, &input(&fixture)).await;
    let mut url = reqwest::Url::parse(&crate::common::database_url()).unwrap();
    url.query_pairs_mut()
        .append_pair("options", "-c search_path=pg_catalog");
    let broken = client_with_providers(ProviderConfig {
        paypal_url: Some(&provider.url),
        paypal_payment: Some(true),
        paypal_timeout_secs: Some(1),
        database_url: Some(url.as_str()),
        ..Default::default()
    })
    .await;
    let count = provider.calls().len();
    let id = created["id"].as_str().unwrap();
    for method in ["GET", "POST"] {
        let response = call(&broken, method, id).await;
        assert_eq!(response.status(), 500);
        assert_eq!(
            body_json(response).await,
            json!({"error":"Internal server error"})
        );
    }
    assert_eq!(provider.calls().len(), count);
    let saved: String = sqlx::query_scalar("SELECT status FROM paypal_payments WHERE id=$1")
        .bind(Uuid::parse_str(id).unwrap())
        .fetch_one(&fixture.pool())
        .await
        .unwrap();
    assert_eq!(saved, "AWAITING_APPROVAL");
}

#[tokio::test]
async fn paypal_regression_embedded_optional_capture_conflicts_reject_even_without_retrieved_duplicates()
 {
    let fixture = fixture().await;
    for (embedded, omitted) in [
        ("payee/merchant_id", "payee"),
        ("custom_id", "custom_id"),
        (
            "supplementary_data/related_ids/order_id",
            "supplementary_data",
        ),
        (
            "supplementary_data/related_ids/capture_id",
            "supplementary_data",
        ),
    ] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        let created = create(&client, &input(&fixture)).await;
        provider.patch(
            &format!("/order/purchase_units/0/payments/captures/0/{embedded}"),
            json!("mismatched-present-identity"),
        );
        provider.patch(&format!("/{omitted}"), Value::Null);
        let response = call(&client, "POST", created["id"].as_str().unwrap()).await;
        assert_eq!(response.status(), 502, "{embedded}");
        assert!(
            provider
                .calls()
                .iter()
                .any(|(method, path, _, _)| method == "GET"
                    && path.starts_with("/v2/payments/captures/"))
        );
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM paypal_payment_captures WHERE payment_id=$1")
                .bind(Uuid::parse_str(created["id"].as_str().unwrap()).unwrap())
                .fetch_one(&fixture.pool())
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn paypal_provider_evidence_unusable_create_order_is_durable_exception() {
    let fixture = fixture().await;
    for (pointer, value) in [
        (
            "/create/links/0/href",
            json!("https://untrusted.example/approve"),
        ),
        ("/create/links", json!([])),
        ("/create/purchase_units/0/amount/value", json!("0.01")),
    ] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        provider.patch(pointer, value);
        let body = input(&fixture);
        let response = client
            .send(request("POST", ROOT, Some(CAP), Some(&body)))
            .await
            .unwrap();
        assert_eq!(response.status(), 502);
        assert_eq!(
            body_json(response).await,
            json!({"error":"PayPal payment evidence does not match"})
        );
        let row:(String,Option<String>,Option<String>,Option<String>)=sqlx::query_as("SELECT status,exception,paypal_order_id,verified_capture_id FROM paypal_payments WHERE idempotency_key=$1").bind(Uuid::parse_str(body["idempotency_key"].as_str().unwrap()).unwrap()).fetch_one(&fixture.pool()).await.unwrap();
        assert_eq!(
            row,
            (
                "EXCEPTION".into(),
                Some("INVALID_ORDER_EVIDENCE".into()),
                None,
                None
            )
        );
        assert_eq!(provider.calls().len(), 2);
        assert!(
            provider
                .calls()
                .iter()
                .all(|(_, path, _, _)| path == "/v1/oauth2/token" || path == "/v2/checkout/orders")
        );
    }
}

#[tokio::test]
async fn paypal_provider_evidence_zero_or_multiple_captures_never_verify() {
    let fixture = fixture().await;
    for count in [0, 2] {
        let provider = PaypalProvider::start().await;
        let client = provider.client().await;
        let created = create(&client, &input(&fixture)).await;
        let order = created["paypal_order_id"].as_str().unwrap();
        let capture = capture_for(&provider.state.lock().unwrap(), order);
        provider.completed();
        provider.patch(
            "/order/purchase_units/0/payments/captures",
            Value::Array(vec![capture; count]),
        );
        let response = call(&client, "POST", created["id"].as_str().unwrap()).await;
        assert_eq!(response.status(), 502);
        assert_eq!(
            body_json(response).await,
            json!({"error":"PayPal payment evidence does not match"})
        );
        let (status, verified, exception): (String, Option<String>, Option<String>) =
            sqlx::query_as(
                "SELECT status,verified_capture_id,exception FROM paypal_payments WHERE id=$1",
            )
            .bind(Uuid::parse_str(created["id"].as_str().unwrap()).unwrap())
            .fetch_one(&fixture.pool())
            .await
            .unwrap();
        assert_eq!(status, "EXCEPTION");
        assert!(verified.is_none());
        assert_eq!(exception.as_deref(), Some("UNEXPECTED_CAPTURE_COUNT"));
        assert!(
            !provider
                .calls()
                .iter()
                .any(|(method, path, _, _)| method == "POST" && path.ends_with("/capture"))
        );
    }
}

fn retrieved_capture(state: &ProviderState, capture_id: &str) -> Response {
    let Some(order) = state
        .orders
        .keys()
        .find(|order| capture_for(state, order)["id"] == capture_id)
    else {
        return (StatusCode::NOT_FOUND, "unknown synthetic capture").into_response();
    };
    let mut capture = capture_for(state, order);
    for (pointer, value) in &state.patches {
        if !pointer.starts_with("/order") && !pointer.starts_with("/create") {
            *capture.pointer_mut(pointer).unwrap() = value.clone();
        }
    }
    axum::Json(capture).into_response()
}
