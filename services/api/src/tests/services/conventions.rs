#![allow(clippy::unwrap_used)]
use super::*;
use chrono::{DateTime, Utc};

#[tokio::test]
async fn convention_timestamps_stay_utc_in_a_non_utc_database_session() {
    let url = std::env::var("E2E_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    // The owned single connection keeps repository pool calls in this rollback-only transaction.
    sqlx::query("BEGIN").execute(&pool).await.unwrap();
    sqlx::query("SET LOCAL TIME ZONE 'Asia/Bangkok'")
        .execute(&pool)
        .await
        .unwrap();
    let (database_now, local_time): (DateTime<Utc>, String) =
        sqlx::query_as("SELECT CURRENT_TIMESTAMP, CURRENT_TIMESTAMP::text")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(local_time.ends_with("+07"), "{local_time}");
    let mut input = validate::create(
        serde_json::from_value(serde_json::json!({
            "slug": format!("native-{}", Uuid::new_v4().simple()), "name": "Native", "country_code": "TH",
            "event_starts_on": "2028-02-29", "event_ends_on": "2028-03-01",
            "description_markdown": "Owned", "currency_code": "THB", "payment_methods": ["stripe"]
        }))
        .unwrap(),
    )
    .unwrap();
    let record = create(&pool, &input).await.unwrap();
    let id = Uuid::parse_str(&record.id).unwrap();
    assert_eq!(
        DateTime::parse_from_rfc3339(&record.created_at).unwrap(),
        database_now
    );
    let mut connection = pool.acquire().await.unwrap();
    let mut items = vec![ConventionVariantInput {
        shopify_product_id: "gid://shopify/Product/1".into(),
        shopify_variant_id: "gid://shopify/ProductVariant/2".into(),
        final_price: "12.99".into(),
    }];
    let first = repo::replace(&mut connection, id, &items)
        .await
        .unwrap()
        .remove(0)
        .into_dto("THB")
        .unwrap();
    input.preorder_enabled = true;
    let updated = repo::update(&mut connection, id, &input)
        .await
        .unwrap()
        .into_dto()
        .unwrap();
    items[0].final_price = "13.99".into();
    let changed = repo::replace(&mut connection, id, &items)
        .await
        .unwrap()
        .remove(0)
        .into_dto("THB")
        .unwrap();
    assert_eq!(updated.created_at, record.created_at);
    assert!(DateTime::parse_from_rfc3339(&updated.updated_at).unwrap() > database_now);
    assert_eq!(first.created_at, changed.created_at);
    assert!(
        DateTime::parse_from_rfc3339(&changed.updated_at).unwrap()
            > DateTime::parse_from_rfc3339(&first.updated_at).unwrap()
    );
    assert_eq!(
        DateTime::parse_from_rfc3339(&first.created_at).unwrap(),
        database_now
    );
    assert_eq!(updated.event_starts_on, "2028-02-29");
    assert_eq!(updated.event_ends_on, "2028-03-01");
    for timestamp in [
        &record.created_at,
        &record.updated_at,
        &updated.created_at,
        &updated.updated_at,
        &first.created_at,
        &first.updated_at,
        &changed.created_at,
        &changed.updated_at,
    ] {
        assert_eq!(
            DateTime::parse_from_rfc3339(timestamp)
                .unwrap()
                .offset()
                .local_minus_utc(),
            0,
            "{timestamp}"
        );
    }
    drop(connection);
    let enabled = list(&pool, 1, 100, Some(true)).await.unwrap();
    assert!(enabled.items.iter().any(|v| v.id == record.id));
    let disabled = list(&pool, 1, 100, Some(false)).await.unwrap();
    assert!(disabled.items.iter().all(|v| v.id != record.id));
    sqlx::query("ROLLBACK").execute(&pool).await.unwrap();
    assert!(repo::find(&pool, id).await.unwrap().is_none());
    pool.close().await;
}
