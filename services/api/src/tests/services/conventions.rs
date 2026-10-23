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
    assert!(
        enabled_by_id(&pool, None, record.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(enabled_by_slug(&pool, None, &slug).await.unwrap().is_none());
    assert!(
        enabled_list(&pool, None, 1, 100)
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
    assert!(
        enabled_by_id(&pool, None, record.id)
            .await
            .unwrap()
            .is_some()
    );
    assert!(enabled_by_slug(&pool, None, &slug).await.unwrap().is_some());
    assert!(
        enabled_list(&pool, None, 1, 100)
            .await
            .unwrap()
            .items
            .iter()
            .any(|v| v.id == record.id.to_string())
    );
    sqlx::query("ROLLBACK").execute(&pool).await.unwrap();
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
