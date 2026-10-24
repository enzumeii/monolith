use super::*;
use common::{ProviderConfig, client_with_providers};

// Scenarios: disabled/missing clients; refused HTTP, non-2xx, GraphQL/partial,
// missing/malformed/shape/throttled data and timeout; isolated database-role
// read failures through each endpoint, with safe JSON and no partial totals.
#[tokio::test]
async fn public_provider_failures_are_safe_and_do_not_return_zero_stock_success() {
    let f = fixture().await;
    let (id, slug) = event(&f).await;
    select(&f, id, 2, "12.99").await;
    let c = common::client().await;
    for (suffix, method, body) in [
        ("products", "GET", None),
        ("quote", "POST", Some(quote(vec![line(2, 1)]))),
    ] {
        assert_eq!(
            response(&c, method, &format!("{URI}/{slug}/{suffix}"), body, 503).await,
            json!({"error":"Shopify client unavailable"})
        );
    }
    for failure in [
        "http",
        "graphql",
        "partial",
        "malformed",
        "missing",
        "shape",
        "throttled",
        "timeout",
    ] {
        let (c, upstream) = common::client_with_upstream_script(vec![], failure).await;
        let status = if failure == "timeout" { 504 } else { 502 };
        for (suffix, method, body) in [
            ("products", "GET", None),
            ("quote", "POST", Some(quote(vec![line(2, 1)]))),
        ] {
            assert_eq!(
                response(&c, method, &format!("{URI}/{slug}/{suffix}"), body, status).await,
                json!({"error":"Upstream request failed"})
            );
        }
        assert_ne!(upstream.requests().len(), 0);
    }
    let c = common::client_with_unreachable_shopify().await;
    response(&c, "GET", &format!("{URI}/{slug}/products"), None, 502).await;
    response(
        &c,
        "POST",
        &format!("{URI}/{slug}/quote"),
        Some(quote(vec![line(2, 1)])),
        502,
    )
    .await;
    for config in [
        ProviderConfig {
            s3_disabled: true,
            ..Default::default()
        },
        ProviderConfig {
            s3_invalid_credentials: true,
            ..Default::default()
        },
    ] {
        let c = client_with_providers(config).await;
        for path in [
            format!("{URI}?today=2028-01-01"),
            format!("{URI}/{slug}"),
            format!("{URI}/{slug}/products"),
        ] {
            assert_eq!(
                response(&c, "GET", &path, None, 503).await,
                json!({"error":"S3 client unavailable"})
            );
        }
        for suffix in ["", "/products"] {
            response(
                &c,
                "GET",
                &format!("{URI}/missing-event{suffix}"),
                None,
                404,
            )
            .await;
        }
        // A quote needs no image projection or S3 client.
        response(
            &c,
            "POST",
            &format!("{URI}/{slug}/quote"),
            Some(quote(vec![])),
            200,
        )
        .await;
    }
}

pub(super) async fn database_role(f: &TestFixture) -> (String, String) {
    let role = format!("{}_database", f.prefix());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE ROLE {role} LOGIN PASSWORD 'e2e-convention';
         GRANT USAGE ON SCHEMA public TO {role};
         GRANT SELECT ON ALL TABLES IN SCHEMA public TO {role};"
    )))
    .execute(&f.pool())
    .await
    .unwrap();
    let mut url = reqwest::Url::parse(&common::database_url()).unwrap();
    url.set_username(&role).unwrap();
    url.set_password(Some("e2e-convention")).unwrap();
    (role, url.to_string())
}

#[tokio::test]
async fn isolated_database_read_failures_cover_every_public_endpoint() {
    let f = fixture().await;
    let (id, slug) = event(&f).await;
    select(&f, id, 2, "12.99").await;
    let (role, url) = database_role(&f).await;
    let c = client_with_providers(ProviderConfig {
        database_url: Some(&url),
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await;
    for table in ["conventions", "convention_variants"] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "REVOKE SELECT ON {table} FROM {role};"
        )))
        .execute(&f.pool())
        .await
        .unwrap();
        let mut requests = vec![
            ("GET", format!("{URI}/{slug}/products"), None),
            (
                "POST",
                format!("{URI}/{slug}/quote"),
                Some(quote(vec![line(2, 1)])),
            ),
        ];
        if table == "conventions" {
            requests.push(("GET", format!("{URI}?today=2028-01-01"), None));
            requests.push(("GET", format!("{URI}/{slug}"), None));
        }
        for (method, path, body) in requests {
            assert_eq!(
                response(&c, method, &path, body, 500).await,
                json!({"error":"Internal server error"})
            );
        }
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "GRANT SELECT ON {table} TO {role};"
        )))
        .execute(&f.pool())
        .await
        .unwrap();
    }
}
