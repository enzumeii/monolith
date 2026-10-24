use super::*;
use std::{sync::Arc, time::Duration};

// Scenarios: no database locks during remote reads; disabled state, replaced
// membership and changed prices are rechecked before a complete success.
#[tokio::test]
async fn live_reads_recheck_enabled_membership_and_price_without_holding_db_locks() {
    let f = fixture().await;
    for suffix in ["products", "quote"] {
        for mutation in ["disabled", "membership", "price"] {
            let (id, slug) = seed(
                &f,
                &format!("{suffix}-{mutation}"),
                "TH",
                "2028-01-01",
                "2028-01-02",
                true,
                "THB",
            )
            .await;
            select(&f, id, 2, "12.99").await;
            let arrived = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            let a = arrived.clone();
            let r = release.clone();
            let router = axum::Router::new().route(
                "/graphql.json",
                axum::routing::post(move || {
                    let a = a.clone();
                    let r = r.clone();
                    async move {
                        a.notify_one();
                        r.notified().await;
                        axum::Json(json!({"data":nodes(vec![live(2,10,"DENY")])}))
                    }
                }),
            );
            let (c, _upstream) = common::client_with_upstream(router).await;
            let path = format!("{URI}/{slug}/{suffix}");
            let cloned = c.clone();
            let pending = tokio::spawn(async move {
                let (method, body) = if suffix == "quote" {
                    ("POST", Some(quote(vec![line(2, 1)])))
                } else {
                    ("GET", None)
                };
                cloned.send(request(method, &path, body)).await.unwrap()
            });
            tokio::time::timeout(Duration::from_secs(3), arrived.notified())
                .await
                .unwrap();
            let sql = match mutation {
                "disabled" => {
                    "UPDATE conventions
                     SET preorder_enabled = FALSE
                     WHERE id = $1"
                }
                "membership" => {
                    "DELETE FROM convention_variants
                     WHERE convention_id = $1"
                }
                _ => {
                    "UPDATE convention_variants
                     SET final_price = 15.25
                     WHERE convention_id = $1"
                }
            };
            tokio::time::timeout(
                Duration::from_secs(1),
                sqlx::query(sql).bind(id).execute(&f.pool()),
            )
            .await
            .unwrap()
            .unwrap();
            release.notify_one();
            let r = pending.await.unwrap();
            assert_eq!(r.status(), if mutation == "disabled" { 404 } else { 409 });
            assert_eq!(r.headers()["cache-control"], "no-store");
            let body = body_json(r).await;
            assert!(body.get("total").is_none());
            assert!(body.get("products").is_none());
        }
    }
}

#[tokio::test]
async fn final_database_recheck_faults_are_safe_for_catalogue_and_quote() {
    let f = fixture().await;
    let (id, slug) = event(&f).await;
    select(&f, id, 2, "12.99").await;
    let (role, url) = super::dependencies::database_role(&f).await;
    for suffix in ["products", "quote"] {
        for table in ["conventions", "convention_variants"] {
            let pool = f.pool();
            let owned_role = role.clone();
            let router = axum::Router::new().route(
                "/graphql.json",
                axum::routing::post(move || {
                    let pool = pool.clone();
                    let role = owned_role.clone();
                    async move {
                        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                            "REVOKE SELECT ON {table} FROM {role};"
                        )))
                        .execute(&pool)
                        .await
                        .unwrap();
                        axum::Json(json!({"data":nodes(vec![live(2,10,"DENY")])}))
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/graphql.json", listener.local_addr().unwrap());
            let task = tokio::spawn(async move {
                axum::serve(listener, router).await.unwrap();
            });
            let c = common::client_with_providers(common::ProviderConfig {
                database_url: Some(&url),
                shopify_url: Some(&endpoint),
                scheduler_enabled: Some(false),
                ..Default::default()
            })
            .await;
            let (method, body) = if suffix == "quote" {
                ("POST", Some(quote(vec![line(2, 1)])))
            } else {
                ("GET", None)
            };
            let result = response(&c, method, &format!("{URI}/{slug}/{suffix}"), body, 500).await;
            task.abort();
            assert_eq!(result, json!({"error":"Internal server error"}));
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "GRANT SELECT ON {table} TO {role};"
            )))
            .execute(&f.pool())
            .await
            .unwrap();
        }
    }
}
