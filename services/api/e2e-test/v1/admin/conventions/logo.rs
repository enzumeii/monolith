use super::*;
use crate::common::{ProviderConfig, TestClient, TestFixture, client_with_providers};
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use image::{DynamicImage, ImageFormat};
fn image() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    DynamicImage::new_rgb8(1, 1)
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
fn upload(f: &TestFixture, id: &str, bytes: Vec<u8>, mime: &str) -> Request<Body> {
    let mut request = authed(
        "PUT",
        &format!("{URI}/{id}/logo"),
        f.root_id(),
        AdminRole::Admin,
    );
    request
        .headers_mut()
        .insert("content-type", mime.parse().unwrap());
    *request.body_mut() = Body::from(bytes);
    request
}
async fn logo(c: &TestClient, f: &TestFixture, id: &str) -> String {
    let response = c.send(upload(f, id, image(), "image/png")).await.unwrap();
    assert_eq!(response.status(), 200, "{}", body_json(response).await);
    body_json(response).await["logo_url"]
        .as_str()
        .unwrap()
        .to_owned()
}
#[tokio::test]
async fn logo_upload_reload_replace_clear_and_metadata_patch_preserve_managed_reference() {
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
    let first = logo(&c, &f, id).await;
    let response = c
        .send(Request::get(&first).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "image/png");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert!(response.headers().get("content-disposition").is_none());
    assert_eq!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .as_ref(),
        image().as_slice()
    );
    let saved = mutate(
        &c,
        &f,
        "PATCH",
        &format!("{URI}/{id}"),
        json!({"name":"New name","preorder_enabled":false}),
        200,
    )
    .await;
    assert_eq!(saved["logo_url"], first);
    mutate(
        &c,
        &f,
        "PATCH",
        &format!("{URI}/{id}"),
        json!({"logo_url":first}),
        400,
    )
    .await;
    let second = logo(&c, &f, id).await;
    assert_ne!(first, second);
    assert_eq!(
        c.send(Request::get(&first).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        404
    );
    mutate(
        &c,
        &f,
        "PATCH",
        &format!("{URI}/{id}"),
        json!({"logo_url":null}),
        200,
    )
    .await;
    assert_eq!(
        c.send(Request::get(&second).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(storage.data.lock().unwrap().puts.len(), 2);
}
#[tokio::test]
async fn logo_validation_auth_and_options_boundaries_precede_storage_calls() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let created = create(&c, &f, input(f.prefix())).await;
    let id = created["id"].as_str().unwrap();
    for (bytes, mime, status) in [
        (vec![], "image/png", 400),
        (vec![0; 2_097_153], "image/png", 413),
        (image(), "image/jpeg", 400),
        (b"<svg/>".to_vec(), "image/svg+xml", 400),
        (b"broken".to_vec(), "image/png", 400),
    ] {
        assert_eq!(
            c.send(upload(&f, id, bytes, mime)).await.unwrap().status(),
            status
        );
    }
    let mut request = upload(&f, id, image(), "image/png");
    request.headers_mut().remove("content-type");
    assert_eq!(c.send(request).await.unwrap().status(), 400);
    for value in ["invalid".to_owned(), uuid::Uuid::new_v4().to_string()] {
        assert_eq!(
            c.send(upload(&f, &value, image(), "image/png"))
                .await
                .unwrap()
                .status(),
            if value == "invalid" { 400 } else { 404 }
        );
    }
    let mut request = upload(&f, id, image(), "image/png");
    request.headers_mut().remove("authorization");
    assert_eq!(c.send(request).await.unwrap().status(), 401);
    let mut request = upload(&f, id, image(), "image/png");
    *request.uri_mut() = format!("{URI}/{id}/logo?unknown=x").parse().unwrap();
    assert_eq!(c.send(request).await.unwrap().status(), 400);
    for (query, status) in [("", 200), ("?unknown=x", 400)] {
        let r = c
            .send(authed(
                "GET",
                &format!("{URI}/options{query}"),
                f.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(r.status(), status);
        if status == 200 {
            assert_eq!(body_json(r).await["currencies"][0]["code"], "THB");
        }
    }
    assert_eq!(storage.data.lock().unwrap().calls, [] as [String; 0]);
}
#[tokio::test]
async fn logo_s3_failures_are_safe_and_do_not_update_previous_database_reference() {
    let f = fixture().await;
    for (operation, fault, status) in [
        ("upload", "http", 502),
        ("upload", "timeout", 504),
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
        let mut value = input(&format!("{}-{}-{}", f.prefix(), operation, fault));
        value["logo_url"] = json!("https://example.test/previous.png");
        let created = create(&c, &f, value).await;
        let id = created["id"].as_str().unwrap();
        let managed = if operation == "upload" {
            None
        } else {
            Some(logo(&c, &f, id).await)
        };
        storage.data.lock().unwrap().failure = Some((operation, fault));
        let response = if let Some(path) = managed {
            c.send(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap()
        } else {
            c.send(upload(&f, id, image(), "image/png")).await.unwrap()
        };
        assert_eq!(response.status(), status, "{operation}/{fault}");
        let error = body_json(response).await;
        let expected = if operation == "download" && fault == "malformed" {
            "Invalid stored convention logo"
        } else {
            "Upstream request failed"
        };
        assert_eq!(error, json!({"error": expected}), "{operation}/{fault}");
        let calls = storage.data.lock().unwrap().calls.clone();
        if operation == "upload" {
            assert_eq!(calls, ["upload"]);
        }
        if operation == "metadata" {
            assert_eq!(calls, ["upload", "metadata"]);
        }
        if operation == "download" {
            assert_eq!(calls, ["upload", "metadata", "download"]);
        }
        if operation == "upload" {
            let r = c
                .send(authed(
                    "GET",
                    &format!("{URI}/{id}"),
                    f.root_id(),
                    AdminRole::Admin,
                ))
                .await
                .unwrap();
            assert_eq!(
                body_json(r).await["logo_url"],
                "https://example.test/previous.png"
            );
        }
    }
}
#[tokio::test]
async fn logo_missing_client_connection_and_database_failures_preserve_saved_value() {
    let f = fixture().await;
    let initial = client().await;
    let created = create(&initial, &f, input(f.prefix())).await;
    let id = created["id"].as_str().unwrap();
    for (disabled, invalid) in [(true, false), (false, true)] {
        let c = client_with_providers(ProviderConfig {
            s3_disabled: disabled,
            s3_invalid_credentials: invalid,
            ..Default::default()
        })
        .await;
        assert_eq!(
            c.send(upload(&f, id, image(), "image/png"))
                .await
                .unwrap()
                .status(),
            503
        );
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&endpoint),
        ..Default::default()
    })
    .await;
    assert_eq!(
        c.send(upload(&f, id, image(), "image/png"))
            .await
            .unwrap()
            .status(),
        502
    );
    let storage = crate::s3_fixture::Fixture::new().await;
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let prefix = f.prefix();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {prefix}_logo_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.id='{id}'::uuid THEN RAISE EXCEPTION 'owned logo write fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER {prefix}_logo_fault BEFORE UPDATE ON conventions FOR EACH ROW EXECUTE FUNCTION {prefix}_logo_fault();"))).execute(&f.pool()).await.unwrap();
    let status = c
        .send(upload(&f, id, image(), "image/png"))
        .await
        .unwrap()
        .status();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {prefix}_logo_fault ON conventions; DROP FUNCTION {prefix}_logo_fault();"
    )))
    .execute(&f.pool())
    .await
    .unwrap();
    assert_eq!(status, 500);
    assert_eq!(storage.data.lock().unwrap().puts.len(), 1);
    let saved = body_json(
        initial
            .send(authed(
                "GET",
                &format!("{URI}/{id}"),
                f.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(saved["logo_url"].is_null());
}
#[tokio::test]
async fn logo_public_reads_validate_current_reference_and_bounded_stored_bytes() {
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
    let path = logo(&c, &f, id).await;
    for (path, status) in [
        (format!("{path}?unknown=1"), 400),
        (
            format!("/api/v1/conventions/invalid/logo/{}", uuid::Uuid::new_v4()),
            400,
        ),
        (format!("/api/v1/conventions/{id}/logo/invalid"), 400),
        (
            format!("/api/v1/conventions/{id}/logo/{}", uuid::Uuid::new_v4()),
            404,
        ),
        (
            format!(
                "/api/v1/conventions/{}/logo/{}",
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4()
            ),
            404,
        ),
    ] {
        assert_eq!(
            c.send(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            status
        );
    }
    for mime in ["text/html", "image/svg+xml"] {
        storage.data.lock().unwrap().content_type = Some(mime.into());
        assert_eq!(
            c.send(Request::get(&path).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            502
        );
    }
    storage.data.lock().unwrap().content_type = Some("image/png".into());
    storage.data.lock().unwrap().calls.clear();
    let response = c
        .send(Request::get(&path).body(Body::from("{}")).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(storage.data.lock().unwrap().calls, [] as [String; 0]);
    let mut corrupt = image();
    corrupt[0] ^= 0xff;
    storage.data.lock().unwrap().download_override = Some(corrupt);
    assert_eq!(
        c.send(Request::get(&path).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        502
    );
    assert_eq!(storage.data.lock().unwrap().calls, ["metadata", "download"]);
    storage.data.lock().unwrap().download_override = None;
    for length in [0, 2_097_153] {
        storage.data.lock().unwrap().content_length = Some(length);
        let before = storage.data.lock().unwrap().calls.len();
        assert_eq!(
            c.send(Request::get(&path).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            502
        );
        assert_eq!(&storage.data.lock().unwrap().calls[before..], &["metadata"]);
    }
    storage.data.lock().unwrap().content_length = None;
    storage.data.lock().unwrap().download_override = Some(b"corrupt".to_vec());
    assert_eq!(
        c.send(Request::get(&path).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        502
    );
    storage.data.lock().unwrap().download_override = Some(vec![0; 2_097_153]);
    assert_eq!(
        c.send(Request::get(&path).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        502
    );
    storage.data.lock().unwrap().objects.clear();
    assert_eq!(
        c.send(Request::get(&path).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        404
    );
}
#[tokio::test]
async fn logo_cookie_upload_requires_origin_and_put_preflight_supports_browser() {
    let f = fixture().await;
    let c = client().await;
    let created = create(&c, &f, input(f.prefix())).await;
    let id = created["id"].as_str().unwrap();
    let jwt = crate::common::jwt_for(f.root_id(), AdminRole::Admin);
    let cookie = format!("backoffice_auth={jwt}~{}", "a".repeat(64));
    for origin in [
        None,
        Some("https://untrusted.example.test"),
        Some("http://127.0.0.1:8084"),
    ] {
        let mut request = upload(&f, id, image(), "image/png");
        request.headers_mut().remove("authorization");
        request
            .headers_mut()
            .insert("x-session-mode", "cookie".parse().unwrap());
        request
            .headers_mut()
            .insert("cookie", cookie.parse().unwrap());
        if let Some(origin) = origin {
            request
                .headers_mut()
                .insert("origin", origin.parse().unwrap());
        }
        assert_eq!(
            c.send(request).await.unwrap().status(),
            if origin == Some("http://127.0.0.1:8084") {
                200
            } else {
                403
            }
        );
    }
    let response = c
        .send(
            Request::builder()
                .method("OPTIONS")
                .uri(format!("{URI}/{id}/logo"))
                .header("origin", "http://127.0.0.1:8084")
                .header("access-control-request-method", "PUT")
                .header(
                    "access-control-request-headers",
                    "content-type,x-session-mode",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "http://127.0.0.1:8084"
    );
    assert!(
        response.headers()["access-control-allow-methods"]
            .to_str()
            .unwrap()
            .contains("PUT")
    );
}
#[tokio::test]
async fn options_empty_config_authentication_and_unexpected_body() {
    let f = fixture().await;
    let c = client_with_providers(ProviderConfig {
        convention_currencies: Some("[]"),
        ..Default::default()
    })
    .await;
    let path = format!("{URI}/options");
    let response = c
        .send(authed("GET", &path, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(body_json(response).await, json!({"currencies":[]}));
    let mut request = authed("GET", &path, f.root_id(), AdminRole::Admin);
    *request.body_mut() = Body::from("{}");
    assert_eq!(c.send(request).await.unwrap().status(), 400);
    assert_eq!(
        c.send(Request::get(&path).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        401
    );
}
#[tokio::test]
async fn logo_database_read_errors_precede_s3_and_public_unavailable_client_is_safe() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    storage.data.lock().unwrap().content_type = Some("image/png".into());
    let normal = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let created = create(&normal, &f, input(f.prefix())).await;
    let id = created["id"].as_str().unwrap();
    let path = logo(&normal, &f, id).await;
    storage.data.lock().unwrap().calls.clear();
    for (disabled, invalid) in [(true, false), (false, true)] {
        let c = client_with_providers(ProviderConfig {
            s3_disabled: disabled,
            s3_invalid_credentials: invalid,
            ..Default::default()
        })
        .await;
        assert_eq!(
            c.send(Request::get(&path).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            503
        );
    }
    let role = format!("{}_logo", f.prefix());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN PASSWORD 'synthetic-logo-password'; GRANT USAGE ON SCHEMA public TO {role}; GRANT SELECT ON _sqlx_migrations TO {role};"))).execute(&f.pool()).await.unwrap();
    let mut db = reqwest::Url::parse(&crate::common::database_url()).unwrap();
    db.set_username(&role).unwrap();
    db.set_password(Some("synthetic-logo-password")).unwrap();
    let c = client_with_providers(ProviderConfig {
        database_url: Some(db.as_str()),
        s3_url: Some(&storage.base),
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await;
    let read = c
        .send(Request::get(&path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let write = c.send(upload(&f, id, image(), "image/png")).await.unwrap();
    drop(c);
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("REVOKE SELECT ON _sqlx_migrations FROM {role}; REVOKE USAGE ON SCHEMA public FROM {role}; DROP ROLE {role};"))).execute(&f.pool()).await.unwrap();
    assert_eq!(read.status(), 500);
    assert_eq!(write.status(), 500);
    assert_eq!(storage.data.lock().unwrap().calls, [] as [String; 0]);
}
#[tokio::test]
async fn logo_header_path_dimensions_and_credentials_rejections_are_complete() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let created = create(&c, &f, input(f.prefix())).await;
    let id = created["id"].as_str().unwrap();
    for path in [format!("{URI}/options"), format!("{URI}/{id}/logo")] {
        for token in [
            "Bearer invalid".to_owned(),
            format!(
                "Bearer {}",
                jsonwebtoken::encode(
                    &jsonwebtoken::Header::default(),
                    &json!({"sub":f.root_id(),"username":"fixture","role":"ADMIN","exp":1}),
                    &jsonwebtoken::EncodingKey::from_secret(
                        &hex::decode(crate::common::JWT_SECRET).unwrap()
                    )
                )
                .unwrap()
            ),
        ] {
            let mut request = if path.ends_with("options") {
                authed("GET", &path, f.root_id(), AdminRole::Admin)
            } else {
                upload(&f, id, image(), "image/png")
            };
            request
                .headers_mut()
                .insert("authorization", token.parse().unwrap());
            assert_eq!(c.send(request).await.unwrap().status(), 401);
        }
    }
    let mut request = upload(&f, id, image(), "image/png");
    request
        .headers_mut()
        .insert("x-session-mode", "invalid".parse().unwrap());
    assert_eq!(c.send(request).await.unwrap().status(), 400);
    let mut bytes = std::io::Cursor::new(Vec::new());
    DynamicImage::new_rgb8(4097, 1)
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    assert_eq!(
        c.send(upload(&f, id, bytes.into_inner(), "image/png"))
            .await
            .unwrap()
            .status(),
        400
    );
    let mut bytes = image();
    bytes.truncate(bytes.len() / 2);
    assert_eq!(
        c.send(upload(&f, id, bytes, "image/png"))
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(storage.data.lock().unwrap().calls, [] as [String; 0]);
}
