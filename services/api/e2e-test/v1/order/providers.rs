#![allow(clippy::too_many_lines)]

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, Method, Uri},
    response::{IntoResponse, Response},
    routing::any,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

pub struct Providers {
    pub url: String,
    state: Arc<Mutex<Scenario>>,
    server: tokio::task::JoinHandle<()>,
}

#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
struct Scenario {
    calls: Vec<Value>,
    draft_input: Value,
    stripe_input: Value,
    paypal_input: Value,
    id: String,
    paid: bool,
    approved: bool,
    captured: bool,
    completed: bool,
    tags: Vec<Value>,
    fault: Option<(String, String)>,
    stock: i64,
    policy: String,
    tracked: bool,
    reservation_valid: bool,
    total_valid: bool,
}

impl Providers {
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(Scenario {
            id: uuid::Uuid::new_v4().simple().to_string(),
            stock: 10,
            policy: "DENY".into(),
            tracked: true,
            reservation_valid: true,
            total_valid: true,
            ..Default::default()
        }));
        let router = Router::new().fallback(any(reply)).with_state(state.clone());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self { url, state, server }
    }

    pub fn calls(&self) -> Vec<Value> {
        self.state.lock().unwrap().calls.clone()
    }

    pub fn count(&self, operation: &str) -> usize {
        self.calls()
            .iter()
            .filter(|v| v["operation"] == operation)
            .count()
    }

    pub fn payment_creates(&self) -> usize {
        self.calls()
            .iter()
            .filter(|v| v["payment_create"] == true)
            .count()
    }

    pub fn pay(&self) {
        let mut scenario = self.state.lock().unwrap();
        scenario.paid = true;
        scenario.approved = true;
    }

    pub fn fault(&self, operation: &str, failure: &str) {
        self.state.lock().unwrap().fault = Some((operation.into(), failure.into()));
    }

    pub fn clear_fault(&self) {
        self.state.lock().unwrap().fault = None;
    }

    pub async fn stop(&mut self) {
        self.server.abort();
        let _ = (&mut self.server).await;
    }

    pub fn inventory(&self, stock: i64, tracked: bool, policy: &str) {
        let mut scenario = self.state.lock().unwrap();
        scenario.stock = stock;
        scenario.tracked = tracked;
        scenario.policy = policy.into();
    }

    pub fn reservation(&self, valid: bool, total_valid: bool) {
        let mut scenario = self.state.lock().unwrap();
        scenario.reservation_valid = valid;
        scenario.total_valid = total_valid;
    }

    pub fn stripe_event(&self, checkout: &str, event: &str) -> Value {
        let state = self.state.lock().unwrap();
        json!({"id":event,"type":"checkout.session.completed","livemode":false,
            "account":"acct_e2e","data":{"object":{"id":format!("cs_{}",state.id),
                "object":"checkout.session","metadata":{"checkout_id":checkout}}}})
    }

    pub fn paypal_event(&self, checkout: &str, event: &str) -> Value {
        let state = self.state.lock().unwrap();
        json!({"id":event,"event_type":"CHECKOUT.ORDER.APPROVED","resource":{
            "id":format!("ORDER{}",state.id),"purchase_units":[{"custom_id":checkout,"reference_id":checkout}]}})
    }

    pub fn assert_navigation_and_private_data(&self, id: &str, secret: &str) {
        let state = self.state.lock().unwrap();
        let (success, cancel) = if state.stripe_input.is_object() {
            assert_eq!(state.stripe_input["metadata[checkout_id]"], id);
            assert_eq!(
                state.stripe_input["payment_intent_data[metadata][checkout_id]"],
                id
            );
            assert_eq!(
                state.stripe_input["line_items[0][price_data][unit_amount]"],
                "4000"
            );
            assert_eq!(
                state.stripe_input["line_items[0][price_data][currency]"],
                "sgd"
            );
            (
                state.stripe_input["success_url"].as_str().unwrap(),
                state.stripe_input["cancel_url"].as_str().unwrap(),
            )
        } else {
            assert_eq!(state.paypal_input["purchase_units"][0]["custom_id"], id);
            assert_eq!(state.paypal_input["purchase_units"][0]["reference_id"], id);
            assert_eq!(
                state.paypal_input["purchase_units"][0]["amount"],
                json!({"value":"40.00","currency_code":"SGD"})
            );
            let context = if state.paypal_input["payment_source"].is_object() {
                &state.paypal_input["payment_source"]["paypal"]["experience_context"]
            } else {
                &state.paypal_input["application_context"]
            };
            (
                context["return_url"].as_str().unwrap(),
                context["cancel_url"].as_str().unwrap(),
            )
        };
        for target in [success, cancel] {
            let url = reqwest::Url::parse(target).unwrap();
            assert_eq!(url.origin().ascii_serialization(), "http://127.0.0.1:8084");
            assert!(target.contains(id));
            assert!(!target.contains("buyer"));
            assert!(!target.contains(secret));
            assert!(!target.contains("DraftOrder"));
            assert_eq!(url.query_pairs().count(), 1);
        }
    }
}

impl Drop for Providers {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn reply(
    State(state): State<Arc<Mutex<Scenario>>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    raw: Bytes,
) -> Response {
    let path = uri.path();
    let body = if path.starts_with("/stripe/") && method == Method::POST {
        let url = reqwest::Url::parse(&format!(
            "http://localhost/?{}",
            String::from_utf8_lossy(&raw)
        ))
        .unwrap();
        Value::Object(
            url.query_pairs()
                .map(|(k, v)| (k.into_owned(), json!(v)))
                .collect(),
        )
    } else {
        serde_json::from_slice(&raw).unwrap_or(Value::Null)
    };
    let operation = body["operationName"].as_str().unwrap_or(path).to_owned();
    if !operation.starts_with('/') {
        assert_eq!(headers["x-shopify-access-token"], "e2e-fixture-token");
    } else if path.starts_with("/stripe/") {
        assert_eq!(headers["authorization"], "Bearer e2e-fixture-key");
    } else if path != "/v1/oauth2/token" {
        assert_eq!(headers["authorization"], "Bearer synthetic-access");
    }
    let failure = {
        let mut scenario = state.lock().unwrap();
        scenario.calls.push(json!({"operation":operation,"path":path,"method":method.as_str(),"body":body,
            "idempotency_key":headers.get("idempotency-key").and_then(|v|v.to_str().ok()),
            "paypal_request_id":headers.get("paypal-request-id").and_then(|v|v.to_str().ok()),
            "payment_create":method == Method::POST && (path == "/stripe/checkout/sessions" || path == "/v2/checkout/orders")}));
        scenario
            .fault
            .as_ref()
            .filter(|(name, _)| operation == *name || *name == provider_operation(path, &method))
            .map(|(_, fault)| fault.clone())
    };
    if let Some(failure) = failure {
        if failure == "lost" {
            let mut scenario = state.lock().unwrap();
            match operation.as_str() {
                "CreateDraftOrder" => {
                    scenario.draft_input = body["variables"]["input"].clone();
                    scenario
                        .tags
                        .clone_from(body["variables"]["input"]["tags"].as_array().unwrap());
                }
                "CompleteCheckoutDraft" => scenario.completed = true,
                "/stripe/checkout/sessions" => scenario.stripe_input = body,
                "/v2/checkout/orders" => scenario.paypal_input = body,
                _ if path.ends_with("/capture") => scenario.captured = true,
                _ => panic!("unsupported lost-response operation {operation}"),
            }
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Synthetic response lost after mutation",
            )
                .into_response();
        }
        return match failure.as_str() {
            "http" => (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "private provider diagnostic",
            )
                .into_response(),
            "malformed" => "{".into_response(),
            "missing" => axum::Json(json!({"data":null})).into_response(),
            "graphql" => axum::Json(json!({"errors":[{"message":"private fixture failure"}]}))
                .into_response(),
            "rejected_signature" => {
                axum::Json(json!({"verification_status":"FAILURE"})).into_response()
            }
            "timeout" => {
                tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                axum::Json(json!({"data":null})).into_response()
            }
            _ => panic!("unknown fixture failure {failure}"),
        };
    }
    let mut scenario = state.lock().unwrap();
    let data = match operation.as_str() {
        "Location" => json!({"location":{"id":"gid://shopify/Location/1"}}),
        "CatalogueProducts" => catalogue(&scenario),
        "CreateDraftOrder" => {
            scenario.draft_input = body["variables"]["input"].clone();
            scenario
                .tags
                .clone_from(body["variables"]["input"]["tags"].as_array().unwrap());
            json!({"draftOrderCreate":{"draftOrder":{
                "id":draft_id(&scenario),"name":"#D1","email":scenario.draft_input["email"],
                "phone":null,"tags":scenario.tags,"reserveInventoryUntil":scenario.draft_input["reserveInventoryUntil"]},"userErrors":[]}})
        }
        "CheckoutDraft" => draft(&scenario),
        "CompleteCheckoutDraft" => {
            assert_eq!(body["variables"]["paymentPending"], false);
            scenario.completed = true;
            json!({"draftOrderComplete":{"draftOrder":{"id":draft_id(&scenario),"order":{"id":order_id(&scenario)}},"userErrors":[]}})
        }
        "CheckoutOrder" => {
            json!({"order":{"id":order_id(&scenario),"name":"#E2E-ORDER","displayFinancialStatus":"PAID","cancelledAt":null,
            "tags":scenario.tags,"totalPriceSet":{"presentmentMoney":{"amount":"40.00","currencyCode":"SGD"}}}})
        }
        "EditOrderTags" => {
            for tag in body["variables"]["tags"].as_array().unwrap() {
                if !scenario.tags.contains(tag) {
                    scenario.tags.push(tag.clone());
                }
            }
            json!({"tagsAdd":{"node":{"id":order_id(&scenario)},"userErrors":[]}})
        }
        "/stripe/checkout/sessions" if method == Method::POST => {
            scenario.stripe_input = body;
            stripe_session(&scenario)
        }
        "/stripe/account" => json!({"id":"acct_e2e","object":"account"}),
        "/v1/oauth2/token" => {
            json!({"access_token":"synthetic-access","token_type":"Bearer","expires_in":3600})
        }
        "/v2/checkout/orders" if method == Method::POST => {
            scenario.paypal_input = body;
            paypal_order(&scenario)
        }
        "/v1/notifications/verify-webhook-signature" => {
            assert_eq!(body["webhook_id"], "webhook_e2e");
            json!({"verification_status":"SUCCESS"})
        }
        _ if path.starts_with("/stripe/checkout/sessions/") => stripe_session(&scenario),
        _ if path.starts_with("/stripe/payment_intents/") => json!({
            "id":format!("pi_{}",scenario.id),"object":"payment_intent","status":"succeeded",
            "amount":4000,"amount_received":4000,"currency":"sgd","livemode":false,
            "metadata":{"checkout_id":scenario.stripe_input["metadata[checkout_id]"]},"latest_charge":format!("ch_{}",scenario.id)}),
        _ if path.starts_with("/stripe/charges/") => json!({
            "id":format!("ch_{}",scenario.id),"object":"charge","status":"succeeded","amount":4000,"amount_captured":4000,
            "currency":"sgd","livemode":false,"paid":true,"captured":true,"refunded":false,"amount_refunded":0,"created":1,
            "disputed":false,"metadata":{},"payment_intent":format!("pi_{}",scenario.id),"balance_transaction":null}),
        _ if path.starts_with("/v2/checkout/orders/") && path.ends_with("/capture") => {
            scenario.captured = true;
            paypal_order(&scenario)
        }
        _ if path.starts_with("/v2/checkout/orders/") => paypal_order(&scenario),
        _ if path.starts_with("/v2/payments/captures/") => paypal_capture(&scenario),
        other => panic!("unexpected checkout upstream call: {other}; {body}"),
    };
    axum::Json(if operation.starts_with('/') {
        data
    } else {
        json!({"data":data})
    })
    .into_response()
}

fn provider_operation(path: &str, method: &Method) -> &'static str {
    if path.starts_with("/stripe/checkout/sessions/") {
        "StripeSession"
    } else if path.starts_with("/stripe/payment_intents/") {
        "StripeIntent"
    } else if path.starts_with("/stripe/charges/") {
        "StripeCharge"
    } else if path.starts_with("/v2/checkout/orders/") && path.ends_with("/capture") {
        "PaypalCaptureOrder"
    } else if path.starts_with("/v2/checkout/orders/") && method == Method::GET {
        "PaypalOrder"
    } else if path.starts_with("/v2/payments/captures/") {
        "PaypalCapture"
    } else {
        ""
    }
}

fn catalogue(scenario: &Scenario) -> Value {
    json!({"products":{"nodes":[{"id":"gid://shopify/Product/1","title":"Synthetic product","status":"ACTIVE","description":"Synthetic product",
        "featuredImage":null,"variants":{"nodes":[{"id":"gid://shopify/ProductVariant/2","title":"Blue","sku":"SKU-E2E","image":null,"selectedOptions":[],
            "inventoryPolicy":scenario.policy,"contextualPricing":{"price":{"amount":"20.00","currencyCode":"SGD"}},
            "inventoryItem":{"id":"gid://shopify/InventoryItem/3","tracked":scenario.tracked,
                "inventoryLevel":{"id":"gid://shopify/InventoryLevel/4","isActive":true,"quantities":[{"name":"available","quantity":scenario.stock}]}}}],
            "pageInfo":{"hasNextPage":false,"endCursor":null}}}],"pageInfo":{"hasNextPage":false,"endCursor":null}}})
}

fn draft_id(scenario: &Scenario) -> String {
    // Shopify numeric resource IDs remain unique across fixture servers.
    format!(
        "gid://shopify/DraftOrder/{}",
        u64::from_str_radix(&scenario.id[..15], 16).unwrap()
    )
}

fn order_id(scenario: &Scenario) -> String {
    format!(
        "gid://shopify/Order/{}",
        u64::from_str_radix(&scenario.id[..15], 16).unwrap()
    )
}

fn draft(scenario: &Scenario) -> Value {
    let reservation = if scenario.reservation_valid {
        scenario.draft_input["reserveInventoryUntil"].clone()
    } else {
        Value::Null
    };
    let lines: Vec<Value> = scenario.draft_input["lineItems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| json!({"quantity":line["quantity"],"variant":{"id":line["variantId"]}}))
        .collect();
    json!({"draftOrder":{"id":draft_id(scenario),"ready":true,"status":if scenario.completed {"COMPLETED"} else {"OPEN"},
        "tags":scenario.tags,"email":scenario.draft_input["email"],"reserveInventoryUntil":reservation,
        "lineItems":{"nodes":lines,"pageInfo":{"hasNextPage":false,"endCursor":null}},
        "order":if scenario.completed {json!({"id":order_id(scenario)})} else {Value::Null},"paymentTerms":null,
        "totalPriceSet":{"presentmentMoney":{"amount":if scenario.total_valid {"40.00"} else {"41.00"},"currencyCode":"SGD"}}}})
}

fn stripe_session(scenario: &Scenario) -> Value {
    json!({"id":format!("cs_{}",scenario.id),"object":"checkout.session","mode":"payment",
        "status":if scenario.paid {"complete"} else {"open"},"payment_status":if scenario.paid {"paid"} else {"unpaid"},
        "livemode":false,"amount_total":4000,"currency":"sgd","metadata":{"checkout_id":scenario.stripe_input["metadata[checkout_id]"]},
        "payment_intent":if scenario.paid {json!(format!("pi_{}",scenario.id))} else {Value::Null},
        "url":"https://checkout.stripe.com/synthetic-session","expires_at":scenario.stripe_input["expires_at"].as_str().and_then(|v|v.parse::<i64>().ok())})
}

fn paypal_order(scenario: &Scenario) -> Value {
    let mut unit = scenario.paypal_input["purchase_units"][0].clone();
    if scenario.captured {
        unit["payments"] = json!({"captures":[paypal_capture(scenario)]});
    }
    json!({"id":format!("ORDER{}",scenario.id),"status":if scenario.captured {"COMPLETED"} else if scenario.approved {"APPROVED"} else {"CREATED"},
        "purchase_units":[unit],"links":[{"rel":"approve","href":"https://www.sandbox.paypal.com/synthetic-approval","method":"GET"}]})
}

fn paypal_capture(scenario: &Scenario) -> Value {
    json!({"id":format!("CAPTURE{}",scenario.id),"status":"COMPLETED","amount":{"value":"40.00","currency_code":"SGD"},
        "final_capture":true,"payee":{"merchant_id":"merchant_e2e"},"custom_id":scenario.paypal_input["purchase_units"][0]["custom_id"],
        "supplementary_data":{"related_ids":{"order_id":format!("ORDER{}",scenario.id),"capture_id":format!("CAPTURE{}",scenario.id)}}})
}
