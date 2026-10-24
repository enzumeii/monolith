#![allow(clippy::too_many_lines)]
use crate::common::{self, TestClient, TestFixture, body_json, fixture};
use axum::{body::Body, http::Request};
use serde_json::{Value, json};
use uuid::Uuid;
const URI: &str = "/api/v1/conventions";

async fn seed(
    f: &TestFixture,
    label: &str,
    country: &str,
    starts: &str,
    ends: &str,
    enabled: bool,
    code: &str,
) -> (Uuid, String) {
    let slug = format!("{}-{label}", f.prefix());
    let id = sqlx::query_scalar(
        "INSERT INTO conventions (
             slug, name, country_code, event_starts_on, event_ends_on,
             description_markdown, currency_code, payment_methods, preorder_enabled
         )
         VALUES ($1, $2, $3, $4::text::date, $5::text::date, $6, $7, ARRAY['stripe'], $8)
         RETURNING id",
    )
    .bind(&slug)
    .bind(format!("Customer {label}"))
    .bind(country)
    .bind(starts)
    .bind(ends)
    .bind("# Welcome\n<script>private HTML</script>")
    .bind(code)
    .bind(enabled)
    .fetch_one(&f.pool())
    .await
    .unwrap();
    (id, slug)
}

async fn event(f: &TestFixture) -> (Uuid, String) {
    seed(f, "event", "TH", "2028-01-01", "2028-01-03", true, "THB").await
}

async fn select(f: &TestFixture, id: Uuid, variant: u64, price: &str) {
    sqlx::query(
        "INSERT INTO convention_variants (
             convention_id, shopify_product_id, shopify_variant_id, final_price
         )
         VALUES ($1, 'gid://shopify/Product/1', $2, $3::text::numeric)",
    )
    .bind(id)
    .bind(gid(variant))
    .bind(price)
    .execute(&f.pool())
    .await
    .unwrap();
}

fn gid(variant: u64) -> String {
    format!("gid://shopify/ProductVariant/{variant}")
}
fn live(variant: u64, quantity: i64, policy: &str) -> Value {
    json!({"__typename":"ProductVariant","id":gid(variant),"title":format!("Variant {variant}"),
        "sku":format!("SKU-{variant}"),"image":{"url":"https://example.test/variant.png"},
        "selectedOptions":[{"name":"Color","value":"Blue"}],"inventoryPolicy":policy,
        "inventoryQuantity":quantity,"inventoryItem":{"tracked":true},
        "product":{"id":"gid://shopify/Product/1","title":"Fresh title","description":"Fresh description",
            "productType":"Print","featuredImage":{"url":"https://example.test/product.png"},"status":"ACTIVE"}})
}
fn nodes(values: Vec<Value>) -> Value {
    json!({"nodes":Value::Array(values)})
}
fn quote(items: Vec<Value>) -> Value {
    json!({"items":Value::Array(items)})
}
fn line(variant: u64, quantity: u32) -> Value {
    json!({"variant_id":gid(variant),"quantity":quantity})
}
fn request(method: &str, path: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder().method(method).uri(path);
    if let Some(body) = body {
        builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    }
}
async fn response(
    c: &TestClient,
    method: &str,
    path: &str,
    body: Option<Value>,
    status: u16,
) -> Value {
    let r = c.send(request(method, path, body)).await.unwrap();
    assert_eq!(r.headers()["cache-control"], "no-store");
    assert!(r.headers().get("www-authenticate").is_none());
    assert_eq!(r.status(), status, "{}", body_json(r).await);
    body_json(r).await
}
async fn get(c: &TestClient, path: &str) -> Value {
    response(c, "GET", path, None, 200).await
}

mod dependencies;
mod directory;
mod products;
mod quotes;
mod races;
