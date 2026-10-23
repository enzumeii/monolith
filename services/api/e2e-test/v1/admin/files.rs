#![allow(clippy::too_many_lines)]
use crate::common::{
    ProviderConfig, TestClient, TestFixture, authed, body_json, client_with_providers, fixture,
};
use axum::{body::Body, http::Request};
use domain::dto::AdminRole;
use image::{DynamicImage, ImageFormat};
use serde_json::Value;
const URI: &str = "/api/v1/admin/files";
pub(super) fn image(format: ImageFormat, width: u32, height: u32) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    DynamicImage::new_rgb8(width, height)
        .write_to(&mut bytes, format)
        .unwrap();
    bytes.into_inner()
}
pub(super) fn upload(f: &TestFixture, location: &str, bytes: Vec<u8>, mime: &str) -> Request<Body> {
    let mut destination = reqwest::Url::parse("http://localhost/").unwrap();
    destination
        .query_pairs_mut()
        .append_pair("location", location);
    let query = destination.query().unwrap();
    let mut request = authed(
        "POST",
        &format!("{URI}?{query}"),
        f.root_id(),
        AdminRole::Admin,
    );
    request
        .headers_mut()
        .insert("content-type", mime.parse().unwrap());
    *request.body_mut() = Body::from(bytes);
    request
}
pub(super) async fn stored(c: &TestClient, f: &TestFixture, slug: &str) -> Value {
    let response = c
        .send(upload(
            f,
            &format!("conventions/{slug}"),
            image(ImageFormat::Png, 1, 1),
            "image/png",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 201, "{}", body_json(response).await);
    assert_eq!(response.headers()["cache-control"], "no-store");
    body_json(response).await
}
#[tokio::test]
async fn files_png_jpeg_admin_roles_unique_keys_and_unsigned_image_reads() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let mut keys = std::collections::HashSet::new();
    for (format, mime, extension, role) in [
        (ImageFormat::Png, "image/png", "png", AdminRole::Admin),
        (
            ImageFormat::Jpeg,
            "image/jpeg",
            "jpg",
            AdminRole::SuperAdmin,
        ),
    ] {
        storage.data.lock().unwrap().content_type = Some(mime.into());
        let bytes = image(format, 1, 1);
        let mut request = upload(
            &f,
            &format!("conventions/{}", f.prefix()),
            bytes.clone(),
            mime,
        );
        request.headers_mut().insert(
            "authorization",
            authed("POST", URI, f.root_id(), role).headers()["authorization"].clone(),
        );
        let response = c.send(request).await.unwrap();
        assert_eq!(response.status(), 201, "{}", body_json(response).await);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let value = body_json(response).await;
        let key = value["location"].as_str().unwrap();
        assert!(key.starts_with(&format!("conventions/{}/", f.prefix())));
        assert!(key.ends_with(&format!(".{extension}")));
        let file = key.rsplit('/').next().unwrap().split('.').next().unwrap();
        assert_eq!(uuid::Uuid::parse_str(file).unwrap().get_version_num(), 4);
        assert!(keys.insert(key.to_owned()));
        let expiry =
            chrono::DateTime::parse_from_rfc3339(value["expires_at"].as_str().unwrap()).unwrap();
        assert!(expiry.timestamp() > chrono::Utc::now().timestamp());
        let url = reqwest::Url::parse(value["url"].as_str().unwrap()).unwrap();
        assert!(url.query_pairs().any(|(k, _)| k == "X-Amz-Signature"));
        assert_eq!(url.path(), format!("/{key}"));
        assert!(!url.path().starts_with("/reports/"));
        let downloaded = reqwest::get(url).await.unwrap();
        assert_eq!(downloaded.status(), 200);
        assert_eq!(downloaded.headers()["content-type"], mime);
        assert_eq!(downloaded.bytes().await.unwrap().as_ref(), bytes);
        let (_, object_key) = key.split_once('/').unwrap();
        let data = storage.data.lock().unwrap();
        assert_eq!(data.destinations.len(), keys.len() * 2);
        assert!(data.destinations.iter().rev().take(2).all(|destination| {
            destination == &("conventions".to_owned(), object_key.to_owned())
        }));
    }
    assert_eq!(storage.data.lock().unwrap().puts.len(), 2);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conventions WHERE slug=$1")
        .bind(f.prefix())
        .fetch_one(&f.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn files_all_query_path_content_and_image_rejections_precede_s3() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    for path in [
        "",
        "conventions",
        "conventions/",
        "/conventions/event",
        "reports/event",
        "https://example.test/event",
        "conventions/../event",
        "conventions/a/b",
        "conventions/Event",
        "conventions/a--b",
        "conventions/-a",
        "conventions/a-",
        "conventions/api",
        "conventions/a.png",
        "conventions/xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "conventions/a\\b",
        "conventions/a\0b",
    ] {
        let response = c
            .send(upload(&f, path, image(ImageFormat::Png, 1, 1), "image/png"))
            .await
            .unwrap();
        assert_eq!(response.status(), 400, "{path:?}");
    }
    for query in [
        "",
        "?unknown=x",
        "?location=conventions/a&unknown=x",
        "?location=conventions/a&location=conventions/b",
        "?location=conventions%2Fa%2F..%2Fb",
        "?location=conventions%2Fa%00",
    ] {
        let mut request = upload(
            &f,
            "conventions/event",
            image(ImageFormat::Png, 1, 1),
            "image/png",
        );
        *request.uri_mut() = format!("{URI}{query}").parse().unwrap();
        assert_eq!(c.send(request).await.unwrap().status(), 400, "{query}");
    }
    let png = image(ImageFormat::Png, 1, 1);
    let mut truncated = png.clone();
    truncated.truncate(truncated.len() / 2);
    for (bytes, mime, status) in [
        (vec![], "image/png", 400),
        (vec![0; 2_097_153], "image/png", 413),
        (png.clone(), "image/jpeg", 400),
        (png.clone(), "text/plain", 400),
        (png.clone(), "image/png; charset=utf-8", 400),
        (b"broken".to_vec(), "image/png", 400),
        (truncated, "image/png", 400),
        (image(ImageFormat::Png, 4097, 1), "image/png", 400),
        (image(ImageFormat::Png, 1, 4097), "image/png", 400),
    ] {
        let response = c
            .send(upload(&f, "conventions/event", bytes, mime))
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{mime} {status}");
    }
    let mut request = upload(&f, "conventions/event", png, "image/png");
    request.headers_mut().remove("content-type");
    assert_eq!(c.send(request).await.unwrap().status(), 400);
    assert_eq!(storage.data.lock().unwrap().calls, Vec::<String>::new());
}
#[tokio::test]
async fn files_authentication_cookie_origin_and_post_preflight() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    for auth in [None, Some("Bearer invalid")] {
        let mut request = upload(
            &f,
            "conventions/event",
            image(ImageFormat::Png, 1, 1),
            "image/png",
        );
        request.headers_mut().remove("authorization");
        if let Some(auth) = auth {
            request
                .headers_mut()
                .insert("authorization", auth.parse().unwrap());
        }
        let response = c.send(request).await.unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
    }
    let jwt = crate::common::jwt_for(f.root_id(), AdminRole::Admin);
    let cookie = format!("backoffice_auth={jwt}~{}", "a".repeat(64));
    for origin in [
        None,
        Some("https://untrusted.example.test"),
        Some("http://127.0.0.1:8084"),
    ] {
        let mut request = upload(
            &f,
            "conventions/event",
            image(ImageFormat::Png, 1, 1),
            "image/png",
        );
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
                201
            } else {
                403
            }
        );
    }
    let response = c
        .send(
            Request::builder()
                .method("OPTIONS")
                .uri(URI)
                .header("origin", "http://127.0.0.1:8084")
                .header("access-control-request-method", "POST")
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
            .contains("POST")
    );
    assert_eq!(storage.data.lock().unwrap().puts.len(), 1);
}
#[tokio::test]
async fn files_dependency_failures_never_create_conventions_or_claim_success() {
    let f = fixture().await;
    for (disabled, invalid) in [(true, false), (false, true)] {
        let c = client_with_providers(ProviderConfig {
            s3_disabled: disabled,
            s3_invalid_credentials: invalid,
            ..Default::default()
        })
        .await;
        assert_eq!(
            c.send(upload(
                &f,
                "conventions/event",
                image(ImageFormat::Png, 1, 1),
                "image/png"
            ))
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
        c.send(upload(
            &f,
            "conventions/event",
            image(ImageFormat::Png, 1, 1),
            "image/png"
        ))
        .await
        .unwrap()
        .status(),
        502
    );
    for (fault, status) in [("http", 502), ("forbidden", 502), ("timeout", 504)] {
        let storage = crate::s3_fixture::Fixture::new().await;
        storage.data.lock().unwrap().failure = Some(("upload", fault));
        let c = client_with_providers(ProviderConfig {
            s3_url: Some(&storage.base),
            ..Default::default()
        })
        .await;
        let response = c
            .send(upload(
                &f,
                "conventions/event",
                image(ImageFormat::Png, 1, 1),
                "image/png",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(
            body_json(response).await,
            serde_json::json!({"error":"Upstream request failed"})
        );
        assert_eq!(storage.data.lock().unwrap().calls, ["upload"]);
    }
    let storage = crate::s3_fixture::Fixture::new().await;
    storage.data.lock().unwrap().lost_response = true;
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    assert_eq!(
        c.send(upload(
            &f,
            "conventions/event",
            image(ImageFormat::Png, 1, 1),
            "image/png"
        ))
        .await
        .unwrap()
        .status(),
        504
    );
    assert_eq!(storage.data.lock().unwrap().puts.len(), 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conventions WHERE slug=$1")
        .bind(f.prefix())
        .fetch_one(&f.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn files_real_s3_signed_url_is_usable_without_authentication_and_tamper_rejected() {
    let f = fixture().await;
    let c = crate::common::client().await;
    let value = stored(&c, &f, f.prefix()).await;
    let url = value["url"].as_str().unwrap();
    let response = reqwest::get(url).await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "image/png");
    assert_eq!(
        response.bytes().await.unwrap().as_ref(),
        image(ImageFormat::Png, 1, 1)
    );
    let mut tampered = reqwest::Url::parse(url).unwrap();
    tampered.query_pairs_mut().append_pair("changed", "true");
    assert_eq!(reqwest::get(tampered).await.unwrap().status(), 403);
}
#[tokio::test]
async fn files_accept_maximum_slug_dimensions_and_exact_byte_budget() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let slug = "x".repeat(63);
    assert_eq!(
        c.send(upload(
            &f,
            &format!("conventions/{slug}"),
            image(ImageFormat::Png, 4096, 1),
            "image/png"
        ))
        .await
        .unwrap()
        .status(),
        201
    );
    assert_eq!(
        c.send(upload(
            &f,
            "conventions/event",
            image(ImageFormat::Png, 1, 4096),
            "image/png"
        ))
        .await
        .unwrap()
        .status(),
        201
    );
    let mut jpeg = image(ImageFormat::Jpeg, 1, 1);
    jpeg.truncate(jpeg.len() - 2);
    // JPEG comments pad an otherwise complete image to the exact accepted byte boundary.
    while jpeg.len() + 2 < 2_097_152 {
        let remaining = 2_097_152 - jpeg.len() - 2;
        let mut segment = remaining.min(65_537);
        if remaining - segment > 0 && remaining - segment < 4 {
            segment -= 4;
        }
        jpeg.extend_from_slice(&[0xff, 0xfe]);
        jpeg.extend_from_slice(&u16::try_from(segment - 2).unwrap().to_be_bytes());
        jpeg.resize(jpeg.len() + segment - 4, b'x');
    }
    jpeg.extend_from_slice(&[0xff, 0xd9]);
    assert_eq!(jpeg.len(), 2_097_152);
    let response = c
        .send(upload(&f, "conventions/event", jpeg, "image/jpeg"))
        .await
        .unwrap();
    assert_eq!(response.status(), 201, "{}", body_json(response).await);
    assert_eq!(storage.data.lock().unwrap().puts.len(), 3);
}
#[tokio::test]
async fn files_expired_bearer_and_malformed_cookie_headers_are_rejected() {
    let f = fixture().await;
    let storage = crate::s3_fixture::Fixture::new().await;
    let c = client_with_providers(ProviderConfig {
        s3_url: Some(&storage.base),
        ..Default::default()
    })
    .await;
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &serde_json::json!({"sub":f.root_id(),"username":"fixture","role":"ADMIN","exp":1}),
        &jsonwebtoken::EncodingKey::from_secret(&hex::decode(crate::common::JWT_SECRET).unwrap()),
    )
    .unwrap();
    let mut request = upload(
        &f,
        "conventions/event",
        image(ImageFormat::Png, 1, 1),
        "image/png",
    );
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    assert_eq!(c.send(request).await.unwrap().status(), 401);
    for cookie in [
        "backoffice_auth=invalid",
        "backoffice_auth=invalid; backoffice_auth=duplicate",
    ] {
        let mut request = upload(
            &f,
            "conventions/event",
            image(ImageFormat::Png, 1, 1),
            "image/png",
        );
        request.headers_mut().remove("authorization");
        request
            .headers_mut()
            .insert("x-session-mode", "cookie".parse().unwrap());
        request
            .headers_mut()
            .insert("origin", "http://127.0.0.1:8084".parse().unwrap());
        request
            .headers_mut()
            .insert("cookie", cookie.parse().unwrap());
        assert_eq!(c.send(request).await.unwrap().status(), 401);
    }
    assert_eq!(storage.data.lock().unwrap().calls, Vec::<String>::new());
}
