pub mod providers;

use super::{ProviderConfig, TestClient, TestFixture, body_json, client_with_providers};
use axum::{body::Body, http::Request};
use providers::Providers;
use serde_json::{Value, json};
use uuid::Uuid;

pub const SECRET: &str = "abababababababababababababababababababababababababababababababab";
pub const OTHER_SECRET: &str = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";
pub const CHECKOUT: &str = "/api/v1/order/checkout";

pub async fn configured(providers: &Providers) -> TestClient {
    client_with_providers(ProviderConfig {
        shopify_url: Some(&format!("{}/graphql.json", providers.url)),
        stripe_url: Some(&format!("{}/stripe/", providers.url)),
        paypal_url: Some(&providers.url),
        checkout_enabled: true,
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await
}

pub async fn purchase(fixture: &TestFixture, provider: &str) -> Value {
    let slug = format!("{}-{}", fixture.prefix(), Uuid::new_v4().simple());
    purchase_with_slug(fixture, provider, &slug).await
}

pub async fn purchase_with_slug(fixture: &TestFixture, provider: &str, slug: &str) -> Value {
    let convention = Uuid::new_v4();
    let variant = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO conventions (
             id, slug, name, country_code, event_starts_on, event_ends_on,
             description_markdown, preorder_enabled, currency_code, payment_methods
         )
         VALUES ($1, $2, 'E2E convention', 'SG', '2028-02-29', '2028-02-29',
                 'Synthetic checkout fixture', TRUE, 'SGD', ARRAY['stripe', 'paypal'])",
    )
    .bind(convention)
    .bind(slug)
    .execute(&fixture.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO convention_variants (
             id, convention_id, shopify_product_id, shopify_variant_id,
             final_price, product_title, variant_title, sku
         )
         VALUES ($1, $2, 'gid://shopify/Product/1', 'gid://shopify/ProductVariant/2',
                 20.00, 'Synthetic product', 'Blue', 'SKU-E2E')",
    )
    .bind(variant)
    .bind(convention)
    .execute(&fixture.pool())
    .await
    .unwrap();
    json!({
        "operation_id": Uuid::new_v4(), "convention_id": convention,
        "email": "buyer@example.test", "provider": provider, "expected_currency": "SGD",
        "items": [{"variant_id": variant, "quantity": 2, "expected_unit_price": 2000}]
    })
}

pub fn request(path: &str, token: Option<&str>, body: &Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

pub fn verify(id: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/order/{id}/verify"));
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::empty()).unwrap()
}

pub async fn accepted(client: &TestClient, input: &Value) -> Value {
    let response = client
        .send(request(CHECKOUT, Some(SECRET), input))
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = body_json(response).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["checkout_id"], input["operation_id"]);
    assert_eq!(body["money"], json!({"minor":4000,"currency":"SGD"}));
    body
}
