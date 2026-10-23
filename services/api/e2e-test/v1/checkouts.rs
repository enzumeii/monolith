use crate::common::*;
use axum::{body::Body, http::Request};
#[tokio::test]
async fn verification_requires_guest_bearer_on_both_mounts() {
    let _fixture = fixture().await;
    let client = client().await;
    for mount in ["/v1", "/api/v1"] {
        for bearer in [
            None,
            Some("Basic private"),
            Some("Bearer"),
            Some("Bearer a b"),
        ] {
            let mut request = Request::builder().method("POST").uri(format!(
                "{mount}/checkouts/{}/verify-payment",
                uuid::Uuid::new_v4()
            ));
            if let Some(bearer) = bearer {
                request = request.header("authorization", bearer);
            }
            let response = client
                .send(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), 401);
            assert_eq!(response.headers()["www-authenticate"], "Bearer");
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert_eq!(
                body_json(response).await,
                serde_json::json!({"error":"Invalid checkout credential"})
            );
        }
    }
}

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

struct Checkout {
    id: Uuid,
    snapshot: Value,
    attempt: Value,
    pool: sqlx::PgPool,
}
impl Checkout {
    async fn seed(fixture: &TestFixture, provider: &str) -> Self {
        Self::seed_currency(fixture, provider, "THB", 2).await
    }
    async fn seed_currency(
        fixture: &TestFixture,
        provider: &str,
        currency: &str,
        precision: u8,
    ) -> Self {
        let id = Uuid::new_v4();
        let draft = u64::from_be_bytes(id.as_bytes()[..8].try_into().unwrap());
        let snapshot = json!({"draft_id":format!("gid://shopify/DraftOrder/{draft}"),"accepted":{"minor":1234,"currency":currency,"precision":precision},"lines":[{"variant_id":"gid://shopify/ProductVariant/1","quantity":1,"unit_price":{"minor":1234,"currency":currency,"precision":precision}}],"email":"buyer@example.test","phone":null,"attributes":{"pickup":"Convention desk"},"tags":["PORTAL","CONVENTION:TEST"],"reservation_until":chrono::Utc::now()+chrono::Duration::hours(1)});
        let reference = if provider == "STRIPE" {
            format!("cs_{}", id.simple())
        } else {
            id.simple().to_string().to_uppercase()
        };
        let attempt = json!({"attempt_id":format!("attempt-{id}"),"provider":provider,"account":if provider=="STRIPE"{"acct_fixture"}else{"MERCHANT1"},"environment":"TEST","reference":reference});
        let pool = fixture.pool();
        sqlx::query("INSERT INTO checkout_completions(id,draft_id,snapshot,attempt,guest_token_hash,guest_token_expires_at) VALUES($1,$2,$3::jsonb,$4::jsonb,$5,CURRENT_TIMESTAMP+INTERVAL '1 hour')")
            .bind(id).bind(snapshot["draft_id"].as_str().unwrap()).bind(snapshot.to_string()).bind(attempt.to_string()).bind(Sha256::digest(b"fixture-guest-token").to_vec()).execute(&pool).await.unwrap();
        Self {
            id,
            snapshot,
            attempt,
            pool,
        }
    }
    fn request(&self) -> Request<Body> {
        guest_request(&self.id.to_string(), "Bearer fixture-guest-token", "")
    }
    async fn reset_throttle(&self) {
        sqlx::query("UPDATE checkout_completions SET next_verify_at=NULL WHERE id=$1")
            .bind(self.id)
            .execute(&self.pool)
            .await
            .unwrap();
    }
    async fn row(&self) -> (String, String, Option<String>, bool) {
        sqlx::query_as("SELECT payment_state,completion_state,order_id,lease_token IS NOT NULL FROM checkout_completions WHERE id=$1").bind(self.id).fetch_one(&self.pool).await.unwrap()
    }
}
impl Drop for Checkout {
    fn drop(&mut self) {
        let id = self.id;
        let url = database_url();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async move {
                    let pool = sqlx::PgPool::connect(&url).await.unwrap();
                    sqlx::query("DELETE FROM checkout_completions WHERE id=$1")
                        .bind(id)
                        .execute(&pool)
                        .await
                        .unwrap();
                    pool.close().await;
                });
        })
        .join()
        .unwrap();
    }
}
fn guest_request(id: &str, auth: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/v1/checkouts/{id}/verify-payment"))
        .header("authorization", auth)
        .body(Body::from(body.to_owned()))
        .unwrap()
}
#[derive(Clone)]
struct Reply {
    status: u16,
    body: String,
    delay: bool,
}
impl Reply {
    fn json(body: &Value) -> Self {
        Self {
            status: 200,
            body: body.to_string(),
            delay: false,
        }
    }
}
struct ProviderHttp {
    url: String,
    replies: Arc<Mutex<BTreeMap<String, Reply>>>,
    requests: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}
impl ProviderHttp {
    async fn new() -> Self {
        use axum::{Router, response::IntoResponse};
        let replies: Arc<Mutex<BTreeMap<String, Reply>>> = Arc::default();
        let requests: Arc<Mutex<Vec<String>>> = Arc::default();
        let r = replies.clone();
        let q = requests.clone();
        let router = Router::new().fallback(move |request: axum::extract::Request| {
            let r = r.clone();
            let q = q.clone();
            async move {
                let path = request.uri().path().to_owned();
                q.lock()
                    .unwrap()
                    .push(format!("{} {path}", request.method()));
                let reply = r.lock().unwrap().get(&path).cloned().unwrap_or(Reply {
                    status: 500,
                    body: "unexpected provider operation".into(),
                    delay: false,
                });
                if reply.delay {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
                (
                    axum::http::StatusCode::from_u16(reply.status).unwrap(),
                    [("content-type", "application/json")],
                    reply.body,
                )
                    .into_response()
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self {
            url,
            replies,
            requests,
            task,
        }
    }
    fn set(&self, path: &str, body: &Value) {
        self.replies
            .lock()
            .unwrap()
            .insert(path.into(), Reply::json(body));
    }
    fn calls(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
    fn stripe(&self, checkout: &Checkout, paid: bool) {
        self.set("/account", &json!({"object":"account","id":"acct_fixture"}));
        self.set(&format!("/checkout/sessions/{}",checkout.attempt["reference"].as_str().unwrap()),&json!({"object":"checkout.session","id":checkout.attempt["reference"],"mode":"payment","status":if paid{"complete"}else{"open"},"payment_status":if paid{"paid"}else{"unpaid"},"livemode":false,"amount_total":1234,"currency":"thb","client_reference_id":checkout.id.to_string(),"metadata":{"checkout_id":checkout.id.to_string(),"attempt_id":checkout.attempt["attempt_id"]},"payment_intent":if paid{Some("pi_fixture")}else{None}}));
        self.set("/payment_intents/pi_fixture",&json!({"object":"payment_intent","id":"pi_fixture","status":"succeeded","amount":1234,"amount_received":1234,"currency":"thb","livemode":false,"latest_charge":"ch_fixture","metadata":{"checkout_id":checkout.id.to_string(),"attempt_id":checkout.attempt["attempt_id"]}}));
        self.set("/charges/ch_fixture",&json!({"id":"ch_fixture","paid":true,"captured":true,"refunded":false,"disputed":false,"amount_captured":1234,"amount_refunded":0,"created":1,"currency":"thb","livemode":false,"metadata":{},"payment_intent":"pi_fixture","balance_transaction":null}));
    }
}
impl Drop for ProviderHttp {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn verification_pending_is_persistently_throttled_and_rejects_nonowners() {
    let fixture = fixture().await;
    let checkout = Checkout::seed(&fixture, "STRIPE").await;
    let provider = ProviderHttp::new().await;
    provider.stripe(&checkout, false);
    let client = client_with_providers(ProviderConfig {
        shopify_url: Some(&provider.url),
        stripe_url: Some(&provider.url),
        ..Default::default()
    })
    .await;
    for (id, token, body, status) in [
        ("bad-id".to_owned(), "Bearer fixture-guest-token", "", 400),
        (
            checkout.id.to_string(),
            "Bearer fixture-guest-token",
            "{}",
            400,
        ),
        (
            checkout.id.to_string(),
            "Bearer fixture-guest-token",
            " ",
            400,
        ),
        (checkout.id.to_string(), "Bearer wrong-token", "", 404),
        (
            Uuid::new_v4().to_string(),
            "Bearer fixture-guest-token",
            "",
            404,
        ),
    ] {
        let response = client.send(guest_request(&id, token, body)).await.unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["cache-control"], "no-store");
        if status == 404 {
            assert_eq!(
                body_json(response).await,
                json!({"error":"Checkout not found"})
            );
        }
    }
    assert_eq!(provider.calls(), [] as [String; 0]);
    for sql in [
        "UPDATE checkout_completions SET guest_token_expires_at=CURRENT_TIMESTAMP-INTERVAL '1 second' WHERE id=$1",
        "UPDATE checkout_completions SET guest_token_expires_at=CURRENT_TIMESTAMP+INTERVAL '1 hour',guest_token_revoked_at=CURRENT_TIMESTAMP WHERE id=$1",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(checkout.id)
            .execute(&checkout.pool)
            .await
            .unwrap();
        let response = client.send(checkout.request()).await.unwrap();
        assert_eq!(response.status(), 404);
        assert_eq!(
            body_json(response).await,
            json!({"error":"Checkout not found"})
        );
    }
    assert_eq!(provider.calls(), [] as [String; 0]);
    sqlx::query("UPDATE checkout_completions SET guest_token_revoked_at=NULL WHERE id=$1")
        .bind(checkout.id)
        .execute(&checkout.pool)
        .await
        .unwrap();
    let origin = "https://convention.enzumeii.com";
    assert_verification_preflight(&client, &checkout, origin).await;
    for _ in 0..2 {
        let mut request = checkout.request();
        request
            .headers_mut()
            .insert("origin", origin.parse().unwrap());
        let response = client.send(request).await.unwrap();
        assert_eq!(response.status(), 202);
        assert_verification_exposed_headers(&response, origin);
        assert_eq!(response.headers()["retry-after"], "5");
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            body_json(response).await,
            json!({"payment_state":"PENDING","completion_state":"READY","order_id":null})
        );
    }
    assert_eq!(provider.calls().len(), 2);
    let second = client_with_providers(ProviderConfig {
        shopify_url: Some(&provider.url),
        stripe_url: Some(&provider.url),
        ..Default::default()
    })
    .await;
    assert_eq!(second.send(checkout.request()).await.unwrap().status(), 202);
    assert_eq!(provider.calls().len(), 2);
    assert!(!checkout.row().await.3);
}

async fn assert_verification_preflight(client: &TestClient, checkout: &Checkout, origin: &str) {
    let preflight = client
        .send(
            Request::builder()
                .method("OPTIONS")
                .uri(format!("/api/v1/checkouts/{}/verify-payment", checkout.id))
                .header("origin", origin)
                .header("access-control-request-method", "POST")
                .header("access-control-request-headers", "authorization")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(preflight.status(), 200);
    assert_eq!(preflight.headers()["access-control-allow-origin"], origin);
    assert!(
        preflight.headers()["access-control-allow-headers"]
            .to_str()
            .unwrap()
            .split(',')
            .any(|header| header.trim() == "authorization")
    );
}

fn assert_verification_exposed_headers(response: &axum::response::Response, origin: &str) {
    assert_eq!(response.headers()["access-control-allow-origin"], origin);
    assert_eq!(
        response.headers()["access-control-allow-credentials"],
        "true"
    );
    let exposed = response.headers()["access-control-expose-headers"]
        .to_str()
        .unwrap();
    for header in ["retry-after", "content-disposition"] {
        assert!(
            exposed
                .split(',')
                .any(|value| value.trim().eq_ignore_ascii_case(header)),
            "missing {header}: {exposed}"
        );
    }
}

impl ProviderHttp {
    fn paypal(&self, checkout: &Checkout, paid: bool) {
        self.set(
            "/v1/oauth2/token",
            &json!({"access_token":"private-fixture-token","token_type":"Bearer","expires_in":3600}),
        );
        self.set(&format!("/v2/checkout/orders/{}",checkout.attempt["reference"].as_str().unwrap()),&json!({"id":checkout.attempt["reference"],"intent":"CAPTURE","status":if paid{"COMPLETED"}else{"APPROVED"},"purchase_units":[{"reference_id":checkout.attempt["attempt_id"],"custom_id":checkout.id.to_string(),"payee":{"merchant_id":"MERCHANT1"},"amount":{"currency_code":"THB","value":"12.34"},"payments":{"captures":if paid{vec![json!({"id":"CAPTURE1","status":"COMPLETED","amount":{"currency_code":"THB","value":"12.34"},"final_capture":true})]}else{vec![]}}}]}));
    }
    fn change(&self, path: &str, pointer: &str, value: Value) {
        let mut replies = self.replies.lock().unwrap();
        let reply = replies.get_mut(path).unwrap();
        let mut body: Value = serde_json::from_str(&reply.body).unwrap();
        *body.pointer_mut(pointer).unwrap() = value;
        reply.body = body.to_string();
    }
}
fn bag(value: &str) -> Value {
    json!({"presentmentMoney":{"amount":value,"currencyCode":"THB"}})
}
fn contents(checkout: &Checkout) -> Value {
    json!({"email":"buyer@example.test","phone":null,"customAttributes":[{"key":"pickup","value":"Convention desk"}],"tags":["PORTAL","CONVENTION:TEST"],"taxesIncluded":true,"totalPriceSet":bag("12.34"),"totalShippingPriceSet":bag("0.00"),"lineItems":shopify_page(vec![json!({"variant":{"id":"gid://shopify/ProductVariant/1"},"quantity":1,"originalUnitPriceSet":bag("12.34")})],None),"reserveInventoryUntil":checkout.snapshot["reservation_until"]})
}
fn order_id(checkout: &Checkout) -> String {
    format!(
        "gid://shopify/Order/{}",
        u64::from_be_bytes(checkout.id.as_bytes()[..8].try_into().unwrap())
    )
}
fn aliases(checkout: &Checkout) -> Value {
    if checkout.attempt["provider"] == "STRIPE" {
        json!([{"kind":"STRIPE_SESSION","value":checkout.attempt["reference"]},{"kind":"STRIPE_PAYMENT_INTENT","value":"pi_fixture"},{"kind":"STRIPE_CHARGE","value":"ch_fixture"}])
    } else {
        json!([{"kind":"PAYPAL_ORDER","value":checkout.attempt["reference"]},{"kind":"PAYPAL_CAPTURE","value":"CAPTURE1"}])
    }
}
fn shopify_replies(checkout: &Checkout) -> Vec<Value> {
    let id = order_id(checkout);
    let mut draft = contents(checkout);
    draft["id"] = checkout.snapshot["draft_id"].clone();
    draft["ready"] = json!(true);
    draft["status"] = json!("OPEN");
    draft["order"] = Value::Null;
    draft["paymentTerms"] = Value::Null;
    let mut order = contents(checkout);
    order["id"] = json!(id);
    order["displayFinancialStatus"] = json!("PAID");
    order["displayFulfillmentStatus"] = json!("UNFULFILLED");
    order["cancelledAt"] = Value::Null;
    order["metafield"] = Value::Null;
    let metadata = json!({"checkout_id":checkout.id,"draft_id":checkout.snapshot["draft_id"],"attempt_id":checkout.attempt["attempt_id"],"provider":checkout.attempt["provider"],"account":checkout.attempt["account"],"environment":"TEST","amount":checkout.snapshot["accepted"],"aliases":aliases(checkout)});
    let mut confirmed = order.clone();
    confirmed["metafield"] = json!({"value":metadata.to_string(),"compareDigest":"fixture-digest"});
    vec![
        json!({"draftOrder":draft}),
        json!({"draftOrderComplete":{"draftOrder":{"id":checkout.snapshot["draft_id"],"order":{"id":id}},"userErrors":[]}}),
        json!({"order":order}),
        json!({"metafieldsSet":{"metafields":[{"namespace":"convention_checkout","key":"payment_references","value":metadata.to_string()}],"userErrors":[]}}),
        json!({"order":confirmed}),
    ]
}
#[tokio::test]
async fn verification_settled_stripe_and_paypal_complete_once_and_cache_without_clients() {
    let fixture = fixture().await;
    for kind in ["STRIPE", "PAYPAL"] {
        let checkout = Checkout::seed(&fixture, kind).await;
        let provider = ProviderHttp::new().await;
        if kind == "STRIPE" {
            provider.stripe(&checkout, true);
        } else {
            provider.paypal(&checkout, true);
        }
        let (shopify_url, shopify) = scripted_upstream(shopify_replies(&checkout), "http").await;
        let client = client_with_providers(ProviderConfig {
            shopify_url: Some(&shopify_url),
            stripe_url: Some(&provider.url),
            paypal_url: Some(&provider.url),
            paypal_verification_environment: Some("SANDBOX"),
            ..Default::default()
        })
        .await;
        let response = client.send(checkout.request()).await.unwrap();
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, 200, "{kind}: {body}");
        assert_eq!(
            body,
            json!({"payment_state":"PAID","completion_state":"COMPLETE","order_id":order_id(&checkout)})
        );
        let calls = provider.calls();
        assert_eq!(calls.len(), if kind == "STRIPE" { 4 } else { 2 });
        assert_eq!(shopify.requests().len(), 5);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM checkout_payment_aliases WHERE checkout_id=$1",
        )
        .bind(checkout.id)
        .fetch_one(&checkout.pool)
        .await
        .unwrap();
        assert_eq!(count, if kind == "STRIPE" { 3 } else { 2 });
        for _ in 0..2 {
            assert_eq!(client.send(checkout.request()).await.unwrap().status(), 200);
        }
        assert_eq!(provider.calls(), calls);
        assert_eq!(shopify.requests().len(), 5);
        let without_clients = crate::common::client().await;
        assert_eq!(
            without_clients
                .send(checkout.request())
                .await
                .unwrap()
                .status(),
            200
        );
        assert!(!checkout.row().await.3);
    }
}
#[tokio::test]
async fn verification_stripe_proof_rejects_wrong_identity_money_and_incomplete_settlement() {
    let fixture = fixture().await;
    let provider = ProviderHttp::new().await;
    let client = client_with_providers(ProviderConfig {
        shopify_url: Some(&provider.url),
        stripe_url: Some(&provider.url),
        ..Default::default()
    })
    .await;
    for (resource, pointer, value, status) in [
        ("account", "/id", json!("acct_other"), 409),
        ("session", "/id", json!("cs_other"), 409),
        ("session", "/livemode", json!(true), 409),
        ("session", "/mode", json!("setup"), 409),
        ("session", "/mode", json!("future"), 502),
        ("session", "/status", json!("future"), 502),
        ("session", "/payment_status", json!("future"), 502),
        ("session", "/client_reference_id", Value::Null, 409),
        (
            "session",
            "/metadata/attempt_id",
            json!("other-attempt"),
            409,
        ),
        (
            "session",
            "/metadata/checkout_id",
            json!(Uuid::new_v4()),
            409,
        ),
        ("session", "/amount_total", json!(1235), 409),
        ("session", "/currency", json!("usd"), 409),
        ("session", "/amount_total", Value::Null, 502),
        ("session", "/payment_intent", Value::Null, 502),
        (
            "session",
            "/payment_intent",
            json!({"id":"pi_fixture"}),
            502,
        ),
        ("intent", "/id", json!("pi_other"), 409),
        ("intent", "/livemode", json!(true), 409),
        (
            "intent",
            "/metadata/attempt_id",
            json!("other-attempt"),
            409,
        ),
        ("intent", "/amount_received", json!(1200), 409),
        ("intent", "/amount", json!(1200), 409),
        ("intent", "/currency", json!("xyz"), 409),
        ("intent", "/status", json!("processing"), 202),
        ("intent", "/status", json!("future"), 502),
        ("intent", "/latest_charge", Value::Null, 502),
        ("charge", "/id", json!("ch_other"), 409),
        ("charge", "/livemode", Value::Null, 502),
        ("charge", "/livemode", json!(true), 409),
        ("charge", "/payment_intent", json!("pi_other"), 409),
        ("charge", "/amount_captured", json!(1200), 409),
        ("charge", "/amount_refunded", json!(1), 409),
        ("charge", "/refunded", json!(true), 409),
        ("charge", "/disputed", json!(true), 409),
        ("charge", "/paid", json!(false), 202),
        ("charge", "/captured", json!(false), 202),
    ] {
        let checkout = Checkout::seed(&fixture, "STRIPE").await;
        provider.stripe(&checkout, true);
        let path = match resource {
            "account" => "/account".into(),
            "session" => format!(
                "/checkout/sessions/{}",
                checkout.attempt["reference"].as_str().unwrap()
            ),
            "intent" => "/payment_intents/pi_fixture".into(),
            _ => "/charges/ch_fixture".into(),
        };
        provider.change(&path, pointer, value);
        let response = client.send(checkout.request()).await.unwrap();
        let actual = response.status();
        let body = body_json(response).await;
        assert_eq!(actual, status, "{resource} {pointer}: {body}");
        assert_eq!(checkout.row().await.0, "PENDING");
        assert!(!checkout.row().await.3);
    }
    assert!(provider.calls().iter().all(|call| call.starts_with("GET ")));
}

#[tokio::test]
async fn verification_paypal_proof_rejects_wrong_identity_money_and_partial_capture() {
    let fixture = fixture().await;
    let provider = ProviderHttp::new().await;
    let client = client_with_providers(ProviderConfig {
        shopify_url: Some(&provider.url),
        paypal_url: Some(&provider.url),
        paypal_verification_environment: Some("SANDBOX"),
        ..Default::default()
    })
    .await;
    for (pointer, value, status) in [
        ("/id", json!("OTHERORDER"), 409),
        ("/intent", json!("AUTHORIZE"), 409),
        ("/intent", Value::Null, 502),
        ("/intent", json!("FUTURE"), 502),
        ("/status", json!("FUTURE"), 502),
        ("/status", json!("APPROVED"), 202),
        ("/purchase_units", json!([]), 502),
        (
            "/purchase_units/0/reference_id",
            json!("wrong-attempt"),
            409,
        ),
        ("/purchase_units/0/custom_id", Value::Null, 409),
        ("/purchase_units/0/payee", Value::Null, 502),
        ("/purchase_units/0/payee/merchant_id", json!("OTHER"), 409),
        ("/purchase_units/0/amount/currency_code", json!("USD"), 409),
        ("/purchase_units/0/amount/value", json!("12.35"), 409),
        ("/purchase_units/0/amount/value", json!("invalid"), 502),
        ("/purchase_units/0/payments", Value::Null, 502),
        ("/purchase_units/0/payments/captures", json!([]), 502),
        (
            "/purchase_units/0/payments/captures/0/final_capture",
            json!(false),
            502,
        ),
        (
            "/purchase_units/0/payments/captures/0/final_capture",
            Value::Null,
            502,
        ),
        (
            "/purchase_units/0/payments/captures/0/amount/value",
            json!("12.33"),
            409,
        ),
        (
            "/purchase_units/0/payments/captures/0/amount/value",
            json!("12.341"),
            502,
        ),
        (
            "/purchase_units/0/payments/captures/0/status",
            json!("PENDING"),
            202,
        ),
        (
            "/purchase_units/0/payments/captures/0/status",
            json!("REFUNDED"),
            409,
        ),
        (
            "/purchase_units/0/payments/captures/0/status",
            json!("PARTIALLY_REFUNDED"),
            409,
        ),
        (
            "/purchase_units/0/payments/captures/0/status",
            json!("DENIED"),
            502,
        ),
        ("/purchase_units/0/payments/captures/0/id", json!(""), 502),
    ] {
        let checkout = Checkout::seed(&fixture, "PAYPAL").await;
        provider.paypal(&checkout, true);
        let path = format!(
            "/v2/checkout/orders/{}",
            checkout.attempt["reference"].as_str().unwrap()
        );
        provider.change(&path, pointer, value);
        let response = client.send(checkout.request()).await.unwrap();
        let actual = response.status();
        let body = body_json(response).await;
        assert_eq!(actual, status, "{pointer}: {body}");
        assert_eq!(checkout.row().await.0, "PENDING");
        assert!(!checkout.row().await.3);
    }
    assert!(provider.calls().iter().all(
        |call| call.starts_with("GET /v2/checkout/orders/") || call == "POST /v1/oauth2/token"
    ));
}
#[tokio::test]
async fn verification_provider_failures_remain_errors_and_release_the_lease() {
    let fixture = fixture().await;
    let provider = ProviderHttp::new().await;
    for kind in ["STRIPE", "PAYPAL"] {
        for operation in if kind == "STRIPE" {
            vec!["account", "session", "intent", "charge"]
        } else {
            vec!["oauth", "order"]
        } {
            for failure in ["http", "malformed", "shape", "timeout"] {
                let checkout = Checkout::seed(&fixture, kind).await;
                if kind == "STRIPE" {
                    provider.stripe(&checkout, true);
                } else {
                    provider.paypal(&checkout, true);
                }
                let path = match operation {
                    "account" => "/account".into(),
                    "session" => format!(
                        "/checkout/sessions/{}",
                        checkout.attempt["reference"].as_str().unwrap()
                    ),
                    "intent" => "/payment_intents/pi_fixture".into(),
                    "charge" => "/charges/ch_fixture".into(),
                    "oauth" => "/v1/oauth2/token".into(),
                    _ => format!(
                        "/v2/checkout/orders/{}",
                        checkout.attempt["reference"].as_str().unwrap()
                    ),
                };
                provider.replies.lock().unwrap().insert(
                    path.clone(),
                    Reply {
                        status: if failure == "http" { 403 } else { 200 },
                        body: if failure == "malformed" {
                            "private-fixture-token".into()
                        } else {
                            "{}".into()
                        },
                        delay: failure == "timeout",
                    },
                );
                let client = client_with_providers(ProviderConfig {
                    shopify_url: Some(&provider.url),
                    stripe_url: Some(&provider.url),
                    paypal_url: Some(&provider.url),
                    paypal_verification_environment: Some("SANDBOX"),
                    ..Default::default()
                })
                .await;
                let response = client.send(checkout.request()).await.unwrap();
                let status = response.status();
                let body = body_json(response).await;
                assert_eq!(
                    status,
                    if failure == "timeout" { 504 } else { 502 },
                    "{kind} {operation} {failure}: {body}"
                );
                assert!(!body.to_string().contains("private-fixture-token"));
                assert_eq!(checkout.row().await.0, "PENDING");
                assert!(!checkout.row().await.3);
                if kind == "STRIPE" {
                    provider.stripe(&checkout, false);
                } else {
                    provider.paypal(&checkout, false);
                }
                assert_eq!(client.send(checkout.request()).await.unwrap().status(), 202);
            }
        }
    }
    assert!(
        provider
            .calls()
            .iter()
            .all(|call| call.starts_with("GET ") || call == "POST /v1/oauth2/token")
    );
}
#[tokio::test]
async fn verification_requires_configured_clients_and_trusted_environment() {
    let fixture = fixture().await;
    let provider = ProviderHttp::new().await;
    for kind in ["STRIPE", "PAYPAL"] {
        let checkout = Checkout::seed(&fixture, kind).await;
        let missing = crate::common::client().await;
        assert_eq!(
            missing.send(checkout.request()).await.unwrap().status(),
            503
        );
        let missing_provider = client_with_providers(ProviderConfig {
            shopify_url: Some(&provider.url),
            ..Default::default()
        })
        .await;
        assert_eq!(
            missing_provider
                .send(checkout.request())
                .await
                .unwrap()
                .status(),
            503
        );
        if kind == "PAYPAL" {
            provider.paypal(&checkout, true);
            let unknown_environment = client_with_providers(ProviderConfig {
                shopify_url: Some(&provider.url),
                paypal_url: Some(&provider.url),
                ..Default::default()
            })
            .await;
            assert_eq!(
                unknown_environment
                    .send(checkout.request())
                    .await
                    .unwrap()
                    .status(),
                503
            );
            assert_eq!(provider.calls(), [] as [String; 0]);
            checkout.reset_throttle().await;
            let wrong_environment = client_with_providers(ProviderConfig {
                shopify_url: Some(&provider.url),
                paypal_url: Some(&provider.url),
                paypal_verification_environment: Some("LIVE"),
                ..Default::default()
            })
            .await;
            assert_eq!(
                wrong_environment
                    .send(checkout.request())
                    .await
                    .unwrap()
                    .status(),
                409
            );
            assert_eq!(provider.calls(), [] as [String; 0]);
        }
    }
}
#[tokio::test]
async fn verification_parallel_calls_and_existing_completion_lease_do_not_overlap_provider_reads() {
    let fixture = fixture().await;
    let checkout = Checkout::seed(&fixture, "STRIPE").await;
    let provider = ProviderHttp::new().await;
    provider.stripe(&checkout, false);
    let client = client_with_providers(ProviderConfig {
        shopify_url: Some(&provider.url),
        stripe_url: Some(&provider.url),
        ..Default::default()
    })
    .await;
    let (a, b) = tokio::join!(
        client.send(checkout.request()),
        client.send(checkout.request())
    );
    assert_eq!(a.unwrap().status(), 202);
    assert_eq!(b.unwrap().status(), 202);
    assert_eq!(provider.calls().len(), 2);
    checkout.reset_throttle().await;
    let owner = Uuid::new_v4();
    sqlx::query("UPDATE checkout_completions SET lease_token=$2,lease_until=CURRENT_TIMESTAMP+INTERVAL '1 minute' WHERE id=$1").bind(checkout.id).bind(owner).execute(&checkout.pool).await.unwrap();
    assert_eq!(client.send(checkout.request()).await.unwrap().status(), 202);
    assert_eq!(provider.calls().len(), 2);
    sqlx::query("UPDATE checkout_completions SET lease_until=CURRENT_TIMESTAMP-INTERVAL '1 second' WHERE id=$1").bind(checkout.id).execute(&checkout.pool).await.unwrap();
    assert_eq!(client.send(checkout.request()).await.unwrap().status(), 202);
    assert_eq!(provider.calls().len(), 4);
    assert!(!checkout.row().await.3);
}
#[tokio::test]
async fn verification_authoritative_expiry_is_terminal_and_operator_state_is_a_conflict() {
    let fixture = fixture().await;
    let provider = ProviderHttp::new().await;
    let client = client_with_providers(ProviderConfig {
        shopify_url: Some(&provider.url),
        stripe_url: Some(&provider.url),
        paypal_url: Some(&provider.url),
        paypal_verification_environment: Some("SANDBOX"),
        ..Default::default()
    })
    .await;
    for kind in ["STRIPE", "PAYPAL"] {
        let checkout = Checkout::seed(&fixture, kind).await;
        if kind == "STRIPE" {
            provider.stripe(&checkout, false);
            provider.change(
                &format!(
                    "/checkout/sessions/{}",
                    checkout.attempt["reference"].as_str().unwrap()
                ),
                "/status",
                json!("expired"),
            );
        } else {
            provider.paypal(&checkout, false);
            provider.change(
                &format!(
                    "/v2/checkout/orders/{}",
                    checkout.attempt["reference"].as_str().unwrap()
                ),
                "/status",
                json!("VOIDED"),
            );
        }
        let response = client.send(checkout.request()).await.unwrap();
        assert_eq!(response.status(), 200);
        assert!(response.headers().get("retry-after").is_none());
        assert_eq!(
            body_json(response).await,
            json!({"payment_state":"EXPIRED","completion_state":"READY","order_id":null})
        );
        let calls = provider.calls();
        assert_eq!(client.send(checkout.request()).await.unwrap().status(), 200);
        assert_eq!(provider.calls(), calls);
        sqlx::query("UPDATE checkout_completions SET completion_state='OPERATOR' WHERE id=$1")
            .bind(checkout.id)
            .execute(&checkout.pool)
            .await
            .unwrap();
        assert_eq!(client.send(checkout.request()).await.unwrap().status(), 409);
        assert_eq!(provider.calls(), calls);
    }
}

#[tokio::test]
async fn verification_shopify_failures_preserve_paid_facts_and_never_retry_mutation_in_request() {
    let fixture = fixture().await;
    let provider = ProviderHttp::new().await;
    for completed_operations in 0..5 {
        for failure in ["http", "malformed", "shape", "timeout"] {
            let checkout = Checkout::seed(&fixture, "STRIPE").await;
            provider.stripe(&checkout, true);
            let prefix = shopify_replies(&checkout)
                .into_iter()
                .take(completed_operations)
                .collect();
            let (url, shopify) = scripted_upstream(prefix, failure).await;
            let client = client_with_providers(ProviderConfig {
                shopify_url: Some(&url),
                stripe_url: Some(&provider.url),
                ..Default::default()
            })
            .await;
            let response = client.send(checkout.request()).await.unwrap();
            let status = response.status();
            let body = body_json(response).await;
            assert_eq!(
                status,
                if failure == "timeout" { 504 } else { 502 },
                "phase {completed_operations} {failure}: {body}"
            );
            assert_eq!(checkout.row().await.0, "PAID");
            assert!(!checkout.row().await.3);
            let mutations = shopify
                .requests()
                .iter()
                .filter(|request| request["operationName"] == "CompleteCheckoutDraft")
                .count();
            assert!(mutations <= 1, "must not retry an ambiguous completion");
            let calls = provider.calls();
            let shopify_calls = shopify.requests().len();
            if failure != "timeout" {
                assert_eq!(client.send(checkout.request()).await.unwrap().status(), 202);
                assert_eq!(provider.calls(), calls);
                assert_eq!(shopify.requests().len(), shopify_calls);
            }
        }
    }
}
#[tokio::test]
async fn verification_paid_recovery_uses_saved_proof_when_provider_is_unavailable_or_stale() {
    let fixture = fixture().await;
    let checkout = Checkout::seed(&fixture, "STRIPE").await;
    let provider = ProviderHttp::new().await;
    provider.stripe(&checkout, true);
    let (url, _shopify) = scripted_upstream(vec![], "http").await;
    let initial = client_with_providers(ProviderConfig {
        shopify_url: Some(&url),
        stripe_url: Some(&provider.url),
        ..Default::default()
    })
    .await;
    assert_eq!(
        initial.send(checkout.request()).await.unwrap().status(),
        502
    );
    assert_eq!(checkout.row().await.0, "PAID");
    checkout.reset_throttle().await;
    let calls = provider.calls();
    provider.stripe(&checkout, false);
    let (recovered_url, recovered) = scripted_upstream(shopify_replies(&checkout), "http").await;
    let recovery = client_with_providers(ProviderConfig {
        shopify_url: Some(&recovered_url),
        ..Default::default()
    })
    .await;
    let response = recovery.send(checkout.request()).await.unwrap();
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(checkout.row().await.0, "PAID");
    assert_eq!(provider.calls(), calls);
    assert_eq!(recovered.requests().len(), 5);
}
struct CheckoutFault {
    name: String,
}
impl CheckoutFault {
    async fn new(checkout: &Checkout, condition: &str) -> Self {
        let name = format!("checkout_fault_{}", checkout.id.simple());
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {name}() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN IF NEW.id='{}'::uuid AND ({condition}) THEN RAISE EXCEPTION 'private database fault'; END IF; RETURN NEW; END; $$; CREATE TRIGGER {name} BEFORE UPDATE ON checkout_completions FOR EACH ROW EXECUTE FUNCTION {name}();",checkout.id)))
            .execute(&checkout.pool).await.unwrap();
        Self { name }
    }
}
impl Drop for CheckoutFault {
    fn drop(&mut self) {
        let name = self.name.clone();
        let url = database_url();
        std::thread::spawn(move||tokio::runtime::Runtime::new().unwrap().block_on(async move{
            let pool=sqlx::PgPool::connect(&url).await.unwrap();
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP TRIGGER IF EXISTS {name} ON checkout_completions; DROP FUNCTION IF EXISTS {name}();"))).execute(&pool).await.unwrap();pool.close().await;
        })).join().unwrap();
    }
}
#[tokio::test]
async fn verification_database_faults_return_safe_500_and_preserve_saved_paid_state() {
    let fixture = fixture().await;
    let provider = ProviderHttp::new().await;
    for (condition, want_payment) in [
        (
            "NEW.next_verify_at IS DISTINCT FROM OLD.next_verify_at",
            "PENDING",
        ),
        (
            "NEW.payment_state='PAID' AND OLD.payment_state<>'PAID'",
            "PENDING",
        ),
        (
            "NEW.completion_intent AND NOT OLD.completion_intent",
            "PAID",
        ),
        ("NEW.order_id IS NOT NULL AND OLD.order_id IS NULL", "PAID"),
    ] {
        let checkout = Checkout::seed(&fixture, "STRIPE").await;
        provider.stripe(&checkout, true);
        let _fault = CheckoutFault::new(&checkout, condition).await;
        let (url, _shopify) = scripted_upstream(shopify_replies(&checkout), "http").await;
        let client = client_with_providers(ProviderConfig {
            shopify_url: Some(&url),
            stripe_url: Some(&provider.url),
            ..Default::default()
        })
        .await;
        let response = client.send(checkout.request()).await.unwrap();
        assert_eq!(response.status(), 500);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            body_json(response).await,
            json!({"error":"Internal server error"})
        );
        assert_eq!(checkout.row().await.0, want_payment);
    }
    let invalid = Checkout::seed(&fixture, "BAD").await;
    assert_eq!(
        crate::common::client()
            .await
            .send(invalid.request())
            .await
            .unwrap()
            .status(),
        500
    );
}

#[tokio::test]
async fn verification_invalid_utf8_path_uses_safe_json_error() {
    let _fixture = fixture().await;
    let client = client().await;
    let response = client
        .send(guest_request("%FF", "Bearer fixture-guest-token", ""))
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        body_json(response).await,
        json!({"error":"Invalid checkout ID"})
    );
}

#[tokio::test]
async fn verification_stripe_uses_charge_currency_units_and_rejects_unsupported_precision() {
    let fixture = fixture().await;
    let provider = ProviderHttp::new().await;
    let client = client_with_providers(ProviderConfig {
        shopify_url: Some(&provider.url),
        stripe_url: Some(&provider.url),
        ..Default::default()
    })
    .await;
    for (currency, precision, status) in [
        ("THB", 2, 202),
        ("JPY", 0, 202),
        ("HUF", 2, 202),
        ("TWD", 2, 202),
        ("BHD", 3, 409),
        ("IQD", 3, 409),
        ("LYD", 3, 409),
        ("XYZ", 2, 409),
        ("ISK", 0, 409),
        ("UGX", 0, 409),
    ] {
        let checkout = Checkout::seed_currency(&fixture, "STRIPE", currency, precision).await;
        provider.stripe(&checkout, false);
        provider.change(
            &format!(
                "/checkout/sessions/{}",
                checkout.attempt["reference"].as_str().unwrap()
            ),
            "/currency",
            json!(currency.to_ascii_lowercase()),
        );
        let response = client.send(checkout.request()).await.unwrap();
        let actual = response.status();
        let body = body_json(response).await;
        assert_eq!(actual, status, "{currency} precision{precision}: {body}");
        assert_eq!(checkout.row().await.0, "PENDING");
    }
}
#[tokio::test]
async fn verification_provider_connection_refusal_is_safe_502() {
    let fixture = fixture().await;
    for kind in ["STRIPE", "PAYPAL"] {
        let checkout = Checkout::seed(&fixture, kind).await;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        drop(listener);
        let client = client_with_providers(ProviderConfig {
            shopify_url: Some(&url),
            stripe_url: Some(&url),
            paypal_url: Some(&url),
            paypal_verification_environment: Some("SANDBOX"),
            ..Default::default()
        })
        .await;
        let response = client.send(checkout.request()).await.unwrap();
        assert_eq!(response.status(), 502);
        assert_eq!(
            body_json(response).await,
            json!({"error":"Upstream request failed"})
        );
        assert_eq!(checkout.row().await.0, "PENDING");
        assert!(!checkout.row().await.3);
    }
}
