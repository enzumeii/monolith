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
    let record = create(&pool, None, &input).await.unwrap();
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
    let updated = logo::project(
        repo::update(&mut connection, id, &input).await.unwrap(),
        None,
    )
    .await
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
    let enabled = list(&pool, None, 1, 100, Some(true)).await.unwrap();
    assert!(enabled.items.iter().any(|v| v.id == record.id));
    let disabled = list(&pool, None, 1, 100, Some(false)).await.unwrap();
    assert!(disabled.items.iter().all(|v| v.id != record.id));
    sqlx::query("ROLLBACK").execute(&pool).await.unwrap();
    assert!(repo::find(&pool, id).await.unwrap().is_none());
    pool.close().await;
}

fn projected_convention(location: Option<String>) -> Convention {
    Convention {
        id: Uuid::new_v4(),
        slug: "fair".into(),
        name: "Fair".into(),
        logo_location: location,
        country_code: "TH".into(),
        event_starts_on: chrono::NaiveDate::from_ymd_opt(2028, 2, 29).unwrap(),
        event_ends_on: chrono::NaiveDate::from_ymd_opt(2028, 2, 29).unwrap(),
        description_markdown: "Owned".into(),
        preorder_enabled: false,
        currency_code: "THB".into(),
        payment_methods: vec!["stripe".into()],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn logo_projection_is_local_and_degrades_without_losing_stored_location() {
    let key = "conventions/fair/550e8400-e29b-41d4-a716-446655440000.png".to_owned();
    let fixture = crate::s3_fixture::Fixture::new().await;
    let projected = logo::project(
        projected_convention(Some(key.clone())),
        Some(&fixture.client),
    )
    .await
    .unwrap();
    assert_eq!(projected.logo_location.as_deref(), Some(key.as_str()));
    assert!(projected.logo_url.unwrap().contains("X-Amz-Signature="));
    assert!(chrono::DateTime::parse_from_rfc3339(&projected.logo_url_expires_at.unwrap()).is_ok());
    assert_eq!(fixture.data.lock().unwrap().calls, Vec::<String>::new());
    let unavailable = logo::project(projected_convention(Some(key.clone())), None)
        .await
        .unwrap();
    assert_eq!(unavailable.logo_location, Some(key));
    assert!(unavailable.logo_url.is_none());
    assert!(unavailable.logo_url_expires_at.is_none());
    let empty = logo::project(projected_convention(None), None)
        .await
        .unwrap();
    assert!(
        empty.logo_location.is_none()
            && empty.logo_url.is_none()
            && empty.logo_url_expires_at.is_none()
    );
    let invalid = logo::project(
        projected_convention(Some("reports/private.csv".into())),
        Some(&fixture.client),
    )
    .await
    .unwrap();
    assert!(invalid.logo_url.is_none() && invalid.logo_url_expires_at.is_none());
}

#[test]
fn options_follow_intrinsic_currency_and_provider_policy_without_runtime_configuration() {
    let result = options();
    assert!(
        result
            .currencies
            .windows(2)
            .all(|pair| pair[0].code < pair[1].code)
    );
    let find = |code: &str| {
        result
            .currencies
            .iter()
            .find(|currency| currency.code == code)
            .unwrap()
    };
    assert_eq!(
        find("THB").payment_methods,
        vec![
            ConventionPaymentMethod::Stripe,
            ConventionPaymentMethod::Paypal,
            ConventionPaymentMethod::Promptpay
        ]
    );
    assert_eq!(
        find("USD").payment_methods,
        vec![
            ConventionPaymentMethod::Stripe,
            ConventionPaymentMethod::Paypal
        ]
    );
    assert_eq!(
        find("KWD").payment_methods,
        vec![ConventionPaymentMethod::Stripe]
    );
    assert!(
        !result
            .currencies
            .iter()
            .any(|currency| currency.code == "ZZZ")
    );
    for currency in result.currencies {
        assert!(money::methods(&currency.code, &currency.payment_methods).is_ok());
    }
}
