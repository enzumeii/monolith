#[test]
fn old_restoration_cannot_restore_logged_out_or_new_account_state() {
    let mut owner = super::ResponseOwner::default();
    let old = owner.request();
    owner.transition();
    assert!(!owner.owns(old));
    let first = owner.request();
    let latest = owner.request();
    assert!(!owner.owns(first));
    assert!(owner.owns(latest));
    owner.transition();
    assert!(!owner.owns(latest));
}

#[cfg(not(target_arch = "wasm32"))]
use crate::api::tests::metadata_responses;
#[cfg(not(target_arch = "wasm32"))]
use dioxus::prelude::ReadableExt;

#[cfg(not(target_arch = "wasm32"))]
const CURRENT: &str =
    r#"{"admin_id":7,"username":"session-admin","role":"ADMIN","expires_at":4102444800}"#;
#[cfg(not(target_arch = "wasm32"))]
const RENEWED: &str =
    r#"{"admin_id":7,"username":"session-admin","role":"ADMIN","expires_at":4102445800}"#;

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn healthy_restoration_reads_session_without_refreshing() {
    let (result, requests) = metadata_responses(&[(200, CURRENT)], restore_snapshot()).await;
    let session = result.expect("restored").expect("signed in");
    assert_eq!(session.admin_id, 7);
    assert_eq!(session.username, "session-admin");
    assert_eq!(session.expires_at, 4_102_444_800);
    assert_requests(&requests, &["GET /v1/auth/session HTTP/1.1"]);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn repeated_healthy_restoration_never_posts_refresh() {
    let (result, requests) = metadata_responses(&[(200, CURRENT), (200, CURRENT)], async {
        let first = restore_snapshot().await?;
        let second = restore_snapshot().await?;
        Ok::<_, crate::api::ApiError>((first, second))
    })
    .await;
    let (first, second) = result.expect("both restored");
    assert!(first.is_some());
    assert_eq!(first, second);
    assert_requests(
        &requests,
        &[
            "GET /v1/auth/session HTTP/1.1",
            "GET /v1/auth/session HTTP/1.1",
        ],
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn unauthorized_access_renews_session() {
    let (result, requests) = metadata_responses(
        &[(401, r#"{"error":"unauthorized"}"#), (200, RENEWED)],
        restore_snapshot(),
    )
    .await;
    assert_eq!(
        result.expect("restored").expect("signed in").expires_at,
        4_102_445_800
    );
    assert_requests(
        &requests,
        &[
            "GET /v1/auth/session HTTP/1.1",
            "POST /v1/auth/refresh HTTP/1.1",
        ],
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn nearly_expired_access_renews_session() {
    let near_expiry = format!(
        r#"{{"admin_id":7,"username":"session-admin","role":"ADMIN","expires_at":{}}}"#,
        super::now_unix() + 3,
    );
    let (result, requests) =
        metadata_responses(&[(200, &near_expiry), (200, RENEWED)], restore_snapshot()).await;
    assert_eq!(
        result.expect("restored").expect("signed in").expires_at,
        4_102_445_800
    );
    assert_requests(
        &requests,
        &[
            "GET /v1/auth/session HTTP/1.1",
            "POST /v1/auth/refresh HTTP/1.1",
        ],
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn unauthorized_access_and_refresh_restore_signed_out_state() {
    let (result, requests) = metadata_responses(
        &[
            (401, r#"{"error":"unauthorized"}"#),
            (401, r#"{"error":"unauthorized"}"#),
        ],
        restore_snapshot(),
    )
    .await;
    assert_eq!(result.expect("restored"), None);
    assert_requests(
        &requests,
        &[
            "GET /v1/auth/session HTTP/1.1",
            "POST /v1/auth/refresh HTTP/1.1",
        ],
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn unavailable_session_probe_preserves_error_without_refreshing() {
    let (result, requests) = metadata_responses(
        &[(503, r#"{"error":"session service unavailable"}"#)],
        restore_snapshot(),
    )
    .await;
    let error = result.expect_err("probe failed");
    assert_eq!(error.status, 503);
    assert_eq!(error.to_string(), "session service unavailable");
    assert_requests(&requests, &["GET /v1/auth/session HTTP/1.1"]);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn malformed_session_probe_preserves_error_without_refreshing() {
    let (result, requests) = metadata_responses(&[(200, "not JSON")], restore_snapshot()).await;
    let error = result.expect_err("invalid metadata");
    assert_eq!(error.status, 502);
    assert_eq!(error.to_string(), "Invalid API response");
    assert_requests(&requests, &["GET /v1/auth/session HTTP/1.1"]);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn unavailable_refresh_preserves_error_after_unauthorized_probe() {
    let (result, requests) = metadata_responses(
        &[
            (401, r#"{"error":"unauthorized"}"#),
            (503, r#"{"error":"refresh service unavailable"}"#),
        ],
        restore_snapshot(),
    )
    .await;
    let error = result.expect_err("refresh failed");
    assert_eq!(error.status, 503);
    assert_eq!(error.to_string(), "refresh service unavailable");
    assert_requests(
        &requests,
        &[
            "GET /v1/auth/session HTTP/1.1",
            "POST /v1/auth/refresh HTTP/1.1",
        ],
    );
}

#[cfg(not(target_arch = "wasm32"))]
async fn restore_snapshot() -> crate::api::ApiResult<Option<super::Session>> {
    super::restore().await?;
    assert!(*super::READY.read());
    assert!(super::RESTORE_ERROR.read().is_none());
    Ok(super::SESSION.read().clone())
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_requests(requests: &[String], expected: &[&str]) {
    let request_lines: Vec<_> = requests
        .iter()
        .map(|request| request.lines().next().expect("request line"))
        .collect();
    assert_eq!(request_lines, expected);
    for request in requests {
        assert!(
            request
                .to_ascii_lowercase()
                .contains("x-session-mode: cookie\r\n")
        );
    }
}
