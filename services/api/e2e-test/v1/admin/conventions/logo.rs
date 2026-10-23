#![allow(clippy::too_many_lines)]
use super::*;
use crate::common::{ProviderConfig, TestClient, TestFixture, client_with_providers};
use crate::v1::admin::files::{image, stored};
use image::ImageFormat;
async fn read(c: &TestClient, f: &TestFixture, id: &str) -> Value {
    let response = c
        .send(authed(
            "GET",
            &format!("{URI}/{id}"),
            f.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    body_json(response).await
}
async fn location(f: &TestFixture, id: &str) -> Option<String> {
    sqlx::query_scalar(
        "SELECT logo_url
         FROM conventions
         WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(id).unwrap())
    .fetch_one(&f.pool())
    .await
    .unwrap()
}
#[tokio::test]
async fn logo_upload_before_create_signed_read_replace_preserve_and_clear() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    storage.data.lock().unwrap().content_type = Some("image/png".into());
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let uploaded = stored(&c, &f, f.prefix()).await;
    let mut value = input(f.prefix());
    value["logo_location"] = uploaded["location"].clone();
    let created = create(&c, &f, value).await;
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["logo_location"], uploaded["location"]);
    assert!(
        created["logo_url"]
            .as_str()
            .unwrap()
            .contains("X-Amz-Signature=")
    );
    assert!(created["logo_url_expires_at"].is_string());
    assert_eq!(
        location(&f, id).await.as_deref(),
        uploaded["location"].as_str()
    );
    let calls = storage.data.lock().unwrap().calls.len();
    let loaded = read(&c, &f, id).await;
    let listing = c
        .send(authed("GET", URI, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(listing.headers()["cache-control"], "no-store");
    assert!(
        body_json(listing).await["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"] == created["id"]
                && v["logo_location"] == uploaded["location"]
                && v["logo_url"].is_string())
    );
    assert_eq!(
        storage.data.lock().unwrap().calls.len(),
        calls,
        "projection must not HEAD/GET each logo"
    );
    let bytes = reqwest::get(loaded["logo_url"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(bytes.status(), 200);
    assert_eq!(bytes.headers()["content-type"], "image/png");
    assert_eq!(
        bytes.bytes().await.unwrap().as_ref(),
        image(ImageFormat::Png, 1, 1)
    );
    storage.data.lock().unwrap().calls.clear();
    let saved = mutate(
        &c,
        &f,
        "PATCH",
        &format!("{URI}/{id}"),
        json!({"name":"New name"}),
        200,
    )
    .await;
    assert_eq!(saved["logo_location"], uploaded["location"]);
    assert_eq!(storage.data.lock().unwrap().calls, Vec::<String>::new());
    let replacement = stored(&c, &f, f.prefix()).await;
    assert_ne!(uploaded["location"], replacement["location"]);
    mutate(
        &c,
        &f,
        "PATCH",
        &format!("{URI}/{id}"),
        json!({"logo_location":replacement["location"]}),
        200,
    )
    .await;
    // Previously issued signed URLs retain their documented lifetime after replacement.
    assert_eq!(
        reqwest::get(loaded["logo_url"].as_str().unwrap())
            .await
            .unwrap()
            .status(),
        200
    );
    let clear = mutate(
        &c,
        &f,
        "PATCH",
        &format!("{URI}/{id}"),
        json!({"logo_location":null}),
        200,
    )
    .await;
    for field in ["logo_location", "logo_url", "logo_url_expires_at"] {
        assert!(clear[field].is_null());
    }
    assert!(location(&f, id).await.is_none());
    assert_eq!(storage.data.lock().unwrap().puts.len(), 2);
}
#[tokio::test]
async fn existing_legacy_logo_survives_unrelated_patch_and_can_be_replaced_or_cleared() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    storage.data.lock().unwrap().content_type = Some("image/png".into());
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let created = create(&c, &f, input(f.prefix())).await;
    let id = created["id"].as_str().unwrap();
    let legacy_url = "https://legacy.example.test/convention.png";
    sqlx::query(
        "UPDATE conventions
         SET logo_url = $2
         WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(id).unwrap())
    .bind(legacy_url)
    .execute(&f.pool())
    .await
    .unwrap();
    let loaded = read(&c, &f, id).await;
    assert_eq!(loaded["logo_location"], legacy_url);
    assert!(loaded["logo_url"].is_null() && loaded["logo_url_expires_at"].is_null());
    let saved = mutate(
        &c,
        &f,
        "PATCH",
        &format!("{URI}/{id}"),
        json!({"name":"Updated legacy fair"}),
        200,
    )
    .await;
    assert_eq!(saved["name"], "Updated legacy fair");
    assert_eq!(saved["logo_location"], legacy_url);
    assert!(saved["logo_url"].is_null() && saved["logo_url_expires_at"].is_null());
    assert_eq!(location(&f, id).await.as_deref(), Some(legacy_url));
    mutate(
        &c,
        &f,
        "PATCH",
        &format!("{URI}/{id}"),
        json!({"logo_location":legacy_url}),
        400,
    )
    .await;
    assert_eq!(location(&f, id).await.as_deref(), Some(legacy_url));
    assert_eq!(storage.data.lock().unwrap().calls, Vec::<String>::new());
    let uploaded = stored(&c, &f, f.prefix()).await;
    let replaced = mutate(
        &c,
        &f,
        "PATCH",
        &format!("{URI}/{id}"),
        json!({"logo_location":uploaded["location"]}),
        200,
    )
    .await;
    assert_eq!(replaced["logo_location"], uploaded["location"]);
    assert!(replaced["logo_url"].is_string());
    assert_eq!(
        location(&f, id).await.as_deref(),
        uploaded["location"].as_str()
    );
    mutate(
        &c,
        &f,
        "PATCH",
        &format!("{URI}/{id}"),
        json!({"logo_location":null}),
        200,
    )
    .await;
    assert!(location(&f, id).await.is_none());
}
#[tokio::test]
async fn logo_attachment_location_validation_missing_and_unknown_legacy_url() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let created = create(&c, &f, input(f.prefix())).await;
    let id = created["id"].as_str().unwrap();
    for value in [
        "https://example.test/logo.png".to_owned(),
        "reports/a.csv".into(),
        format!("conventions/other/{}.png", uuid::Uuid::new_v4()),
        format!("conventions/{}/../{}.png", f.prefix(), uuid::Uuid::new_v4()),
        format!("conventions/{}/not-a-uuid.png", f.prefix()),
    ] {
        let mut data = input(&format!("{}-reject", f.prefix()));
        data["logo_location"] = json!(value);
        mutate(&c, &f, "POST", URI, data, 400).await;
        mutate(
            &c,
            &f,
            "PATCH",
            &format!("{URI}/{id}"),
            json!({"logo_location":value}),
            400,
        )
        .await;
    }
    assert_eq!(storage.data.lock().unwrap().calls, Vec::<String>::new());
    let missing = format!("conventions/{}/{}.png", f.prefix(), uuid::Uuid::new_v4());
    let response = mutate(
        &c,
        &f,
        "PATCH",
        &format!("{URI}/{id}"),
        json!({"logo_location":missing}),
        400,
    )
    .await;
    assert!(response["error"].is_string());
    assert!(location(&f, id).await.is_none());
    for value in [json!(null), json!("https://example.test/logo.png")] {
        let mut data = input(&format!("{}-legacy", f.prefix()));
        data["logo_url"] = value.clone();
        mutate(&c, &f, "POST", URI, data, 400).await;
        mutate(
            &c,
            &f,
            "PATCH",
            &format!("{URI}/{id}"),
            json!({"logo_url":value}),
            400,
        )
        .await;
    }
    assert_eq!(
        c.send(authed(
            "PUT",
            &format!("{URI}/{id}/logo"),
            f.root_id(),
            AdminRole::Admin
        ))
        .await
        .unwrap()
        .status(),
        404
    );
    assert_eq!(
        c.send(
            axum::http::Request::get(format!(
                "/api/v1/conventions/{id}/logo/{}",
                uuid::Uuid::new_v4()
            ))
            .body(axum::body::Body::empty())
            .unwrap()
        )
        .await
        .unwrap()
        .status(),
        404
    );
}
#[tokio::test]
async fn logo_attachment_provider_failures_and_invalid_objects_preserve_database() {
    let f = fixture().await;
    for (operation, fault, status) in [
        ("metadata", "http", 502),
        ("metadata", "timeout", 504),
        ("metadata", "malformed", 502),
        ("download", "http", 502),
        ("download", "timeout", 504),
        ("download", "malformed", 502),
    ] {
        let storage = crate::s3_fixture::Fixture::new().await;
        storage.data.lock().unwrap().content_type = Some("image/png".into());
        let c = client_with_providers(ProviderConfig {
            s3_url: Some(&storage.base),
            ..Default::default()
        })
        .await;
        let slug = format!("{}-{operation}-{fault}", f.prefix());
        let created = create(&c, &f, input(&slug)).await;
        let id = created["id"].as_str().unwrap();
        let uploaded = stored(&c, &f, &slug).await;
        storage.data.lock().unwrap().failure = Some((operation, fault));
        let response = mutate(
            &c,
            &f,
            "PATCH",
            &format!("{URI}/{id}"),
            json!({"logo_location":uploaded["location"]}),
            status,
        )
        .await;
        let expected = if operation == "download" && fault == "malformed" {
            "Invalid stored convention logo"
        } else {
            "Upstream request failed"
        };
        assert_eq!(response, json!({"error":expected}));
        assert!(location(&f, id).await.is_none());
        let calls = storage.data.lock().unwrap().calls.clone();
        assert_eq!(
            calls,
            if operation == "metadata" {
                vec!["upload", "metadata"]
            } else {
                vec!["upload", "metadata", "download"]
            }
        );
    }
    let storage = crate::s3_fixture::Fixture::new().await;
    storage.data.lock().unwrap().content_type = Some("image/png".into());
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let created = create(&c, &f, input(f.prefix())).await;
    let id = created["id"].as_str().unwrap();
    let uploaded = stored(&c, &f, f.prefix()).await;
    for (mime, size) in [
        ("text/html", None),
        ("image/jpeg", None),
        ("image/png", Some(0)),
        ("image/png", Some(2_097_153)),
    ] {
        storage.data.lock().unwrap().content_type = Some(mime.into());
        storage.data.lock().unwrap().content_length = size;
        mutate(
            &c,
            &f,
            "PATCH",
            &format!("{URI}/{id}"),
            json!({"logo_location":uploaded["location"]}),
            502,
        )
        .await;
        assert!(location(&f, id).await.is_none());
    }
    storage.data.lock().unwrap().content_type = Some("image/png".into());
    storage.data.lock().unwrap().content_length = None;
    for bytes in [b"corrupt".to_vec(), vec![0; 2_097_153]] {
        storage.data.lock().unwrap().download_override = Some(bytes);
        mutate(
            &c,
            &f,
            "PATCH",
            &format!("{URI}/{id}"),
            json!({"logo_location":uploaded["location"]}),
            502,
        )
        .await;
        assert!(location(&f, id).await.is_none());
    }
}
#[tokio::test]
async fn logo_optional_signer_degradation_and_attachment_database_failure_are_safe() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    storage.data.lock().unwrap().content_type = Some("image/png".into());
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let uploaded = stored(&c, &f, f.prefix()).await;
    let mut value = input(f.prefix());
    value["logo_location"] = uploaded["location"].clone();
    let created = create(&c, &f, value).await;
    let id = created["id"].as_str().unwrap();
    for (disabled, invalid) in [(true, false), (false, true)] {
        let unavailable = client_with_providers(ProviderConfig {
            s3_disabled: disabled,
            s3_invalid_credentials: invalid,
            ..Default::default()
        })
        .await;
        let no_logo = create(
            &unavailable,
            &f,
            input(&format!("{}-no-logo-{disabled}", f.prefix())),
        )
        .await;
        assert!(no_logo["logo_location"].is_null());
        assert!(no_logo["logo_url"].is_null());
        let loaded = read(&unavailable, &f, id).await;
        assert_eq!(loaded["logo_location"], uploaded["location"]);
        assert!(loaded["logo_url"].is_null());
        assert!(loaded["logo_url_expires_at"].is_null());
        let saved = mutate(
            &unavailable,
            &f,
            "PATCH",
            &format!("{URI}/{id}"),
            json!({"name":"Metadata survives"}),
            200,
        )
        .await;
        assert_eq!(saved["logo_location"], uploaded["location"]);
        assert!(saved["logo_url"].is_null());
        assert!(saved["logo_url_expires_at"].is_null());
        let listing = unavailable
            .send(authed("GET", URI, f.root_id(), AdminRole::Admin))
            .await
            .unwrap();
        let listing = body_json(listing).await;
        let listed = listing["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == created["id"])
            .unwrap();
        assert_eq!(listed["logo_location"], uploaded["location"]);
        assert!(listed["logo_url"].is_null());
        assert!(listed["logo_url_expires_at"].is_null());
        let clear = mutate(
            &unavailable,
            &f,
            "PATCH",
            &format!("{URI}/{id}"),
            json!({"logo_location":null}),
            200,
        )
        .await;
        assert!(clear["logo_location"].is_null());
        mutate(
            &c,
            &f,
            "PATCH",
            &format!("{URI}/{id}"),
            json!({"logo_location":uploaded["location"]}),
            200,
        )
        .await;
        mutate(
            &unavailable,
            &f,
            "PATCH",
            &format!("{URI}/{id}"),
            json!({"logo_location":uploaded["location"]}),
            503,
        )
        .await;
    }
    let prefix = f.prefix();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {prefix}_logo_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.id='{id}'::uuid THEN RAISE EXCEPTION 'owned logo write fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER {prefix}_logo_fault BEFORE UPDATE ON conventions FOR EACH ROW EXECUTE FUNCTION {prefix}_logo_fault();"))).execute(&f.pool()).await.unwrap();
    let response = c
        .send(authed_json(
            "PATCH",
            &format!("{URI}/{id}"),
            f.root_id(),
            AdminRole::Admin,
            &json!({"logo_location":uploaded["location"]}),
        ))
        .await
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {prefix}_logo_fault ON conventions; DROP FUNCTION {prefix}_logo_fault();"
    )))
    .execute(&f.pool())
    .await
    .unwrap();
    assert_eq!(response.status(), 500);
    assert_eq!(
        body_json(response).await,
        json!({"error":"Internal server error"})
    );
    assert_eq!(
        location(&f, id).await.as_deref(),
        uploaded["location"].as_str()
    );
    storage.data.lock().unwrap().objects.clear();
    let loaded = read(&c, &f, id).await;
    assert!(loaded["logo_url"].is_string());
    assert_eq!(
        reqwest::get(loaded["logo_url"].as_str().unwrap())
            .await
            .unwrap()
            .status(),
        404
    );
}
#[tokio::test]
async fn options_intrinsic_policy_authentication_and_unexpected_body() {
    let f = fixture().await;
    let c = client_with_providers(ProviderConfig::default()).await;
    let path = format!("{URI}/options");
    let response = c
        .send(authed("GET", &path, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let result = body_json(response).await;
    let currencies = result["currencies"].as_array().unwrap();
    let find = |code: &str| {
        currencies
            .iter()
            .find(|currency| currency["code"] == code)
            .unwrap()
    };
    assert_eq!(
        find("THB")["payment_methods"],
        json!(["stripe", "paypal", "promptpay"])
    );
    assert_eq!(find("USD")["payment_methods"], json!(["stripe", "paypal"]));
    assert_eq!(find("KWD")["payment_methods"], json!(["stripe"]));
    assert!(currencies.iter().all(|currency| currency["code"] != "ZZZ"));
    let unknown_query = format!("{path}?unused=true");
    assert_eq!(
        c.send(authed("GET", &unknown_query, f.root_id(), AdminRole::Admin))
            .await
            .unwrap()
            .status(),
        400
    );
    let mut request = authed("GET", &path, f.root_id(), AdminRole::Admin);
    *request.body_mut() = axum::body::Body::from("{}");
    assert_eq!(c.send(request).await.unwrap().status(), 400);
    assert_eq!(
        c.send(
            axum::http::Request::get(&path)
                .body(axum::body::Body::empty())
                .unwrap()
        )
        .await
        .unwrap()
        .status(),
        401
    );
}
#[tokio::test]
async fn logo_create_dependency_failure_never_inserts_record() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    storage.data.lock().unwrap().content_type = Some("image/png".into());
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let uploaded = stored(&c, &f, f.prefix()).await;
    let mut value = input(f.prefix());
    value["logo_location"] = uploaded["location"].clone();
    for (operation, fault, status) in [
        ("metadata", "forbidden", 502),
        ("metadata", "timeout", 504),
        ("download", "http", 502),
        ("download", "timeout", 504),
    ] {
        storage.data.lock().unwrap().failure = Some((operation, fault));
        mutate(&c, &f, "POST", URI, value.clone(), status).await;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conventions WHERE slug=$1")
            .bind(f.prefix())
            .fetch_one(&f.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
