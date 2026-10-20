use crate::common::{
    assert_report_error, authed, body_json, client, client_with_accounting_scenario,
    client_with_empty_accounting_stripe, fixture,
};
use domain::dto::AdminRole;

#[tokio::test]
async fn transactions_requires_both_date_bounds_over_tcp() {
    let fixture = fixture().await;
    let client = client().await;

    for (query, status) in [
        ("", 400),
        ("date_from=2026-10-01T00%3A00%3A00%2B07%3A00", 400),
        ("date_to=2026-10-31T23%3A59%3A59%2B07%3A00", 400),
        (
            "date_from=2026-11-01T00%3A00%3A00%2B07%3A00&date_to=2026-10-31T23%3A59%3A59%2B07%3A00",
            400,
        ),
        (
            "date_from=2026-10-01T00%3A00%3A00%2B07%3A00&date_to=2026-10-31T23%3A59%3A59%2B07%3A00",
            503,
        ),
    ] {
        let response = client
            .send(authed(
                "GET",
                &format!("/v1/admin/transactions?{query}"),
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_report_error(
            response,
            status,
            if status == 400 {
                "Expected valid RFC3339 date_from and date_to values with date_from before date_to"
            } else {
                "Accounting report provider unavailable"
            },
        )
        .await;
    }
}

#[tokio::test]
async fn transactions_returns_empty_report_when_provider_has_no_payouts_over_tcp() {
    let fixture = fixture().await;
    let (client, _upstream) = client_with_empty_accounting_stripe().await;
    let response = client
        .send(authed(
            "GET",
            "/v1/admin/transactions?date_from=2026-10-01T00%3A00%3A00%2B07%3A00&date_to=2026-10-31T23%3A59%3A59%2B07%3A00",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        body_json(response).await,
        serde_json::json!({"transactions": [], "items": []})
    );
}

#[tokio::test]
async fn transactions_returns_provider_failures_as_safe_errors_over_tcp() {
    let fixture = fixture().await;
    for scenario in ["stripe_error", "paypal_error", "shopify_error"] {
        let (client, _upstream) = client_with_accounting_scenario(scenario).await;
        let response = client
            .send(authed(
                "GET",
                "/v1/admin/transactions?date_from=2026-10-01T00%3A00%3A00%2B07%3A00&date_to=2026-10-31T23%3A59%3A59%2B07%3A00",
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_report_error(response, 502, "Upstream request failed").await;
    }
}

#[tokio::test]
async fn transactions_maps_stripe_payout_and_shopify_lines_over_tcp() {
    let fixture = fixture().await;
    let (client, _upstream) = client_with_accounting_scenario("populated").await;
    let response = client
        .send(authed(
            "GET",
            "/v1/admin/transactions?date_from=2026-10-01T00%3A00%3A00%2B07%3A00&date_to=2026-10-31T23%3A59%3A59%2B07%3A00",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let report = body_json(response).await;
    assert_eq!(report["transactions"].as_array().unwrap().len(), 1);
    assert_eq!(report["transactions"][0]["gateway"], "STRIPE");
    assert_eq!(report["transactions"][0]["type"], "SALE");
    assert_eq!(report["transactions"][0]["shopify_order_id"], 123);
    assert_eq!(report["transactions"][0]["shopify_order_number"], 9876);
    assert_eq!(report["transactions"][0]["exchange_rate"], 36.0);
    assert_eq!(report["transactions"][0]["gross_paid"], 7380.0);
    assert_eq!(report["transactions"][0]["shipping_fee"], 180.0);
    assert_eq!(report["items"].as_array().unwrap().len(), 1);
    assert_eq!(report["items"][0]["quantity"], 2);
    assert_eq!(report["items"][0]["price"], 100.0);
    assert_eq!(report["items"][0]["currency"], "USD");
    assert_eq!(report["items"][0]["category"], "KEYCHAIN");
}

#[tokio::test]
async fn transactions_requires_admin_authentication_over_tcp() {
    let fixture = fixture().await;
    let client = client().await;
    let response = client
        .send(axum::http::Request::builder()
            .method("GET")
            .uri("/v1/admin/transactions?date_from=2026-10-01T00%3A00%3A00%2B07%3A00&date_to=2026-10-31T23%3A59%3A59%2B07%3A00")
            .body(axum::body::Body::empty())
            .unwrap())
        .await
        .unwrap();
    assert_report_error(response, 401, "Invalid or expired token").await;
    drop(fixture);
}
