#![allow(clippy::unwrap_used)]
use super::*;
use chrono::{DateTime, Utc};
use serde_json::json;

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Keep the rollback-only session, schema and atomic snapshot timestamp assertions together"
)]
async fn convention_timestamps_stay_utc_in_a_non_utc_database_session() {
    let pool = rollback_pool().await;
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
    let checked = validations::conventions::create(
        serde_json::from_value(serde_json::json!({
            "slug": format!("native-{}", Uuid::new_v4().simple()), "name": "Native", "country_code": "TH",
            "event_starts_on": "2028-02-29", "event_ends_on": "2028-03-01",
            "description_markdown": "Owned", "currency_code": "THB", "payment_methods": ["stripe"]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut input = transformers::conventions::create(checked);
    let fixture = crate::s3_fixture::Fixture::new().await;
    let record = create(&pool, Ok(&fixture.client), &input).await.unwrap();
    let id = Uuid::parse_str(&record.id).unwrap();
    assert_eq!(
        DateTime::parse_from_rfc3339(&record.created_at).unwrap(),
        database_now
    );
    let mut connection = pool.acquire().await.unwrap();
    let mut items = vec![SelectedVariant {
        input: ConventionVariantInput {
            shopify_product_id: "gid://shopify/Product/1".into(),
            shopify_variant_id: "gid://shopify/ProductVariant/2".into(),
            final_price: "12.99".into(),
        },
        display: VariantDisplay {
            product_title: Some("Product".into()),
            ..VariantDisplay::default()
        },
    }];
    let first = variant_response(
        repositories::conventions::replace(&mut connection, id, &items)
            .await
            .unwrap()
            .remove(0),
        "THB",
    )
    .unwrap();
    assert_eq!(first.inventory_policy, None);
    let mut previous = first.clone();
    for policy in [
        domain::dto::CatalogueInventoryPolicy::Deny,
        domain::dto::CatalogueInventoryPolicy::Continue,
    ] {
        items[0].display.inventory_policy = Some(policy);
        let saved = variant_response(
            repositories::conventions::replace(&mut connection, id, &items)
                .await
                .unwrap()
                .remove(0),
            "THB",
        )
        .unwrap();
        assert_eq!(saved.inventory_policy, Some(policy));
        assert_eq!(saved.id, first.id);
        assert_eq!(saved.created_at, first.created_at);
        assert!(
            DateTime::parse_from_rfc3339(&saved.updated_at).unwrap()
                > DateTime::parse_from_rfc3339(&previous.updated_at).unwrap()
        );
        previous = saved;
    }
    items[0].display.inventory_policy = None;
    let cleared = variant_response(
        repositories::conventions::replace(&mut connection, id, &items)
            .await
            .unwrap()
            .remove(0),
        "THB",
    )
    .unwrap();
    assert_eq!(cleared.inventory_policy, None);
    assert!(
        DateTime::parse_from_rfc3339(&cleared.updated_at).unwrap()
            > DateTime::parse_from_rfc3339(&previous.updated_at).unwrap()
    );
    let columns: Vec<(String, String)> = sqlx::query_as(
        "SELECT column_name, is_nullable
         FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'convention_variants'
           AND column_name IN ('inventory_policy', 'product_image_alt_text', 'variant_image_alt_text')
         ORDER BY column_name",
    )
    .fetch_all(&mut *connection)
    .await
    .unwrap();
    assert_eq!(columns, vec![("inventory_policy".into(), "YES".into())]);
    sqlx::query("SAVEPOINT invalid_policy")
        .execute(&mut *connection)
        .await
        .unwrap();
    let invalid = sqlx::query(
        "UPDATE convention_variants
         SET inventory_policy = 'UNKNOWN'
         WHERE convention_id = $1",
    )
    .bind(id)
    .execute(&mut *connection)
    .await
    .unwrap_err();
    assert_eq!(
        invalid.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    sqlx::query("ROLLBACK TO SAVEPOINT invalid_policy")
        .execute(&mut *connection)
        .await
        .unwrap();
    input.preorder_enabled = true;
    let updated = logo::project(
        repositories::conventions::update(&mut connection, id, &input)
            .await
            .unwrap(),
        &fixture.client,
    )
    .await
    .unwrap();
    items[0].input.final_price = "13.99".into();
    let changed = variant_response(
        repositories::conventions::replace(&mut connection, id, &items)
            .await
            .unwrap()
            .remove(0),
        "THB",
    )
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
    let enabled = list(&pool, &fixture.client, 1, 100, Some(true))
        .await
        .unwrap();
    assert!(enabled.items.iter().any(|v| v.id == record.id));
    let disabled = list(&pool, &fixture.client, 1, 100, Some(false))
        .await
        .unwrap();
    assert!(disabled.items.iter().all(|v| v.id != record.id));
    sqlx::query("ROLLBACK").execute(&pool).await.unwrap();
    assert!(
        repositories::conventions::find(&pool, id)
            .await
            .unwrap()
            .is_none()
    );
    pool.close().await;
}

fn projected_convention(location: Option<String>) -> Convention {
    Convention {
        id: Uuid::new_v4(),
        slug: "fair".into(),
        name: "Fair".into(),
        logo_url: location,
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
async fn logo_projection_is_local_and_exposes_only_signed_preview_fields() {
    let key = "conventions/fair/550e8400-e29b-41d4-a716-446655440000.png".to_owned();
    let fixture = crate::s3_fixture::Fixture::new().await;
    let projected = logo::project(projected_convention(Some(key.clone())), &fixture.client)
        .await
        .unwrap();
    assert!(
        serde_json::to_value(&projected)
            .unwrap()
            .get("logo_url")
            .is_none()
    );
    let preview = reqwest::Url::parse(&projected.logo_preview_url.unwrap()).unwrap();
    assert_eq!(preview.path(), format!("/{key}"));
    assert!(
        preview
            .query_pairs()
            .any(|(name, _)| name == "X-Amz-Signature")
    );
    assert!(
        chrono::DateTime::parse_from_rfc3339(&projected.logo_preview_url_expires_at.unwrap())
            .is_ok()
    );
    assert_eq!(fixture.data.lock().unwrap().calls, Vec::<String>::new());
    let empty = logo::project(projected_convention(None), &fixture.client)
        .await
        .unwrap();
    assert!(empty.logo_preview_url.is_none() && empty.logo_preview_url_expires_at.is_none());
    let invalid = logo::project(
        projected_convention(Some("reports/private.csv".into())),
        &fixture.client,
    )
    .await
    .unwrap();
    assert!(invalid.logo_preview_url.is_none() && invalid.logo_preview_url_expires_at.is_none());
}

#[test]
fn options_include_every_assigned_country_in_sorted_order() {
    let result = options();
    assert_eq!(result.countries.len(), 249);
    assert!(result.countries.windows(2).all(|pair| pair[0] < pair[1]));
    for first in b'A'..=b'Z' {
        for second in b'A'..=b'Z' {
            let code = String::from_utf8(vec![first, second]).unwrap();
            assert_eq!(
                result.countries.binary_search(&code).is_ok(),
                !matches!(CountryCode::from(code.clone()), CountryCode::Other(_)),
                "{code}"
            );
        }
    }
    for code in ["TH", "US", "GB", "JP"] {
        assert!(result.countries.iter().any(|country| country == code));
    }
    for code in ["ZZ", "XX", "THA"] {
        assert!(!result.countries.iter().any(|country| country == code));
    }
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

#[tokio::test]
async fn convention_logo_urls_reuse_the_published_logo_url_column() {
    let pool = rollback_pool().await;
    let checked = validations::conventions::create(
        serde_json::from_value(serde_json::json!({
            "slug": format!("native-logo-{}", Uuid::new_v4().simple()),
            "name": "Legacy fair", "country_code": "TH",
            "event_starts_on": "2028-02-29", "event_ends_on": "2028-03-01",
            "description_markdown": "Legacy description", "currency_code": "THB",
            "payment_methods": ["stripe"]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut input = transformers::conventions::create(checked);
    let legacy_url = "https://legacy.example.test/convention.png";
    input.logo_url = Some(legacy_url.into());
    let inserted = repositories::conventions::insert(&pool, &input)
        .await
        .unwrap();
    let id = inserted.id;
    assert_eq!(inserted.logo_url.as_deref(), Some(legacy_url));
    let fixture = crate::s3_fixture::Fixture::new().await;
    let projected = logo::project(inserted, &fixture.client).await.unwrap();
    assert!(
        serde_json::to_value(&projected)
            .unwrap()
            .get("logo_url")
            .is_none()
    );
    assert!(
        projected.logo_preview_url.is_none() && projected.logo_preview_url_expires_at.is_none()
    );
    assert_eq!(fixture.data.lock().unwrap().calls, Vec::<String>::new());
    let mut connection = pool.acquire().await.unwrap();
    let locked = repositories::conventions::lock(&mut connection, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(locked.logo_url.as_deref(), Some(legacy_url));
    input.name = "Updated fair".into();
    let updated = repositories::conventions::update(&mut connection, id, &input)
        .await
        .unwrap();
    assert_eq!(updated.logo_url.as_deref(), Some(legacy_url));
    let retained: String = sqlx::query_scalar(
        "SELECT logo_url
         FROM conventions
         WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    assert_eq!(retained, legacy_url);
    let location = format!("conventions/{}/{}.png", input.slug, Uuid::new_v4());
    input.logo_url = Some(location.clone());
    let saved = repositories::conventions::update(&mut connection, id, &input)
        .await
        .unwrap();
    assert_eq!(saved.logo_url.as_deref(), Some(location.as_str()));
    let stored: String = sqlx::query_scalar(
        "SELECT logo_url
         FROM conventions
         WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    assert_eq!(stored, location);
    drop(connection);
    let found = repositories::conventions::find(&pool, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.logo_url.as_deref(), Some(location.as_str()));
    let listed = repositories::conventions::list(&pool, 1, u32::MAX, None)
        .await
        .unwrap();
    assert!(
        listed
            .iter()
            .any(|record| record.id == id && record.logo_url.as_deref() == Some(location.as_str()))
    );
    let signed = logo::project(found, &fixture.client).await.unwrap();
    assert!(
        signed
            .logo_preview_url
            .unwrap()
            .contains("X-Amz-Signature=")
    );
    assert!(signed.logo_preview_url_expires_at.is_some());
    assert_eq!(fixture.data.lock().unwrap().calls, Vec::<String>::new());
    sqlx::query("ROLLBACK").execute(&pool).await.unwrap();
    assert!(
        repositories::conventions::find(&pool, id)
            .await
            .unwrap()
            .is_none()
    );
    pool.close().await;
}

fn request(country: &str) -> domain::dto::CreateConvention {
    serde_json::from_value(serde_json::json!({"slug":"country-test","name":"Convention","country_code":country,"event_starts_on":"2028-02-29","event_ends_on":"2028-02-29","description_markdown":"Event","currency_code":"THB","payment_methods":["stripe"]})).unwrap()
}

#[test]
fn patch_preserves_omitted_legacy_logos_and_validates_supplied_locations() {
    let input =
        transformers::conventions::create(validations::conventions::create(request("TH")).unwrap());
    let legacy_url = "https://legacy.example.test/convention.png";
    let current = Convention {
        id: uuid::Uuid::new_v4(),
        slug: input.slug,
        name: input.name,
        logo_url: Some(legacy_url.into()),
        country_code: input.country_code,
        event_starts_on: input.event_starts_on,
        event_ends_on: input.event_ends_on,
        description_markdown: input.description_markdown,
        preorder_enabled: false,
        currency_code: input.currency_code,
        payment_methods: vec!["stripe".into()],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let patch = serde_json::from_value(json!({"name":"Updated convention"})).unwrap();
    let merged = merge(validations::conventions::patch(patch).unwrap(), &current).unwrap();
    assert_eq!(merged.name, "Updated convention");
    assert_eq!(merged.logo_url.as_deref(), Some(legacy_url));
    let patch = serde_json::from_value(json!({"logo_url":null})).unwrap();
    assert!(
        merge(validations::conventions::patch(patch).unwrap(), &current)
            .unwrap()
            .logo_url
            .is_none()
    );
    let location = "conventions/country-test/550e8400-e29b-41d4-a716-446655440000.png";
    let patch = serde_json::from_value(json!({"logo_url":location})).unwrap();
    assert_eq!(
        merge(validations::conventions::patch(patch).unwrap(), &current)
            .unwrap()
            .logo_url
            .as_deref(),
        Some(location)
    );
    let patch = serde_json::from_value(json!({"logo_url":legacy_url})).unwrap();
    assert!(validations::conventions::patch(patch).is_err());
    let mut new_convention = request("TH");
    new_convention.logo_url = Some(legacy_url.into());
    assert!(validations::conventions::create(new_convention).is_err());
}

#[test]
fn variant_projection_rejects_corrupt_saved_display_before_price() {
    let record = ConventionVariant {
        product_title: None,
        variant_title: None,
        sku: None,
        product_image_url: None,
        variant_image_url: None,
        inventory_policy: Some("UNKNOWN".into()),
        id: Uuid::new_v4(),
        convention_id: Uuid::new_v4(),
        shopify_product_id: "gid://shopify/Product/1".into(),
        shopify_variant_id: "gid://shopify/ProductVariant/2".into(),
        final_price: "12.99".into(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    assert!(variant_response(record, "THB").is_err());
}

async fn rollback_pool() -> sqlx::PgPool {
    let url = std::env::var("E2E_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    // A single connection keeps repository pool calls inside this rollback-only transaction.
    sqlx::query("BEGIN").execute(&pool).await.unwrap();
    pool
}
