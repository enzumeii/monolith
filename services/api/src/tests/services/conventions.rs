#![allow(clippy::unwrap_used)]
use super::*;
#[tokio::test]
async fn enabled_only_lookup_agreement_excludes_disabled_conventions() {
    let url = std::env::var("E2E_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    // The single owned connection rolls back on drop as well as on the success path.
    sqlx::query("BEGIN").execute(&pool).await.unwrap();
    let slug = format!("native-{}", Uuid::new_v4().simple());
    let input=validate::create(serde_json::from_value(serde_json::json!({"slug":slug,"name":"Native","country_code":"TH","event_starts_on":"2028-02-29","event_ends_on":"2028-02-29","description_markdown":"Owned","currency_code":"THB","payment_methods":["stripe"]})).unwrap()).unwrap();
    let record = repo::insert(&pool, &input).await.unwrap();
    assert!(enabled_by_id(&pool, record.id).await.unwrap().is_none());
    assert!(enabled_by_slug(&pool, &slug).await.unwrap().is_none());
    assert!(
        enabled_list(&pool, 1, 100)
            .await
            .unwrap()
            .items
            .iter()
            .all(|v| v.id != record.id.to_string())
    );
    sqlx::query("UPDATE conventions SET preorder_enabled=true WHERE id=$1")
        .bind(record.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(enabled_by_id(&pool, record.id).await.unwrap().is_some());
    assert!(enabled_by_slug(&pool, &slug).await.unwrap().is_some());
    assert!(
        enabled_list(&pool, 1, 100)
            .await
            .unwrap()
            .items
            .iter()
            .any(|v| v.id == record.id.to_string())
    );
    sqlx::query("ROLLBACK").execute(&pool).await.unwrap();
    pool.close().await;
}
