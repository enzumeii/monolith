#![allow(clippy::unwrap_used)]
use super::*;
use domain::dto::AdminRole;

fn query(page: Option<u32>, per_page: Option<u32>) -> ListAdminsQuery {
    ListAdminsQuery { page, per_page }
}

fn claims(admin_id: i64, role: AdminRole) -> AuthenticatedAdmin {
    AuthenticatedAdmin { id: admin_id, role }
}

#[test]
fn validate_page_applies_defaults_when_omitted() {
    let page = validate_page(&query(None, None)).unwrap();
    assert_eq!(page.page, 1);
    assert_eq!(page.per_page, 25);
}

#[test]
fn validate_page_rejects_zero_page() {
    assert!(matches!(
        validate_page(&query(Some(0), Some(25)))
            .map_err(|error| axum::response::IntoResponse::into_response(error).status()),
        Err(axum::http::StatusCode::BAD_REQUEST)
    ));
}

#[test]
fn validate_page_rejects_zero_per_page() {
    assert!(matches!(
        validate_page(&query(Some(1), Some(0)))
            .map_err(|error| axum::response::IntoResponse::into_response(error).status()),
        Err(axum::http::StatusCode::BAD_REQUEST)
    ));
}

#[test]
fn validate_page_rejects_per_page_over_100() {
    assert!(matches!(
        validate_page(&query(Some(1), Some(101)))
            .map_err(|error| axum::response::IntoResponse::into_response(error).status()),
        Err(axum::http::StatusCode::BAD_REQUEST)
    ));
}

#[test]
fn validate_page_accepts_boundary_values() {
    assert!(validate_page(&query(Some(1), Some(1))).is_ok());
    assert!(validate_page(&query(Some(1), Some(100))).is_ok());
}

#[test]
fn validate_update_passes_when_ids_match() {
    assert!(validate_update(&claims(1, AdminRole::Admin), 1).is_ok());
}

#[test]
fn validate_update_returns_forbidden_when_ids_mismatch() {
    assert!(matches!(
        validate_update(&claims(1, AdminRole::Admin), 2)
            .map_err(|error| axum::response::IntoResponse::into_response(error).status()),
        Err(axum::http::StatusCode::FORBIDDEN)
    ));
}

#[test]
fn validate_update_super_admin_passes_for_self() {
    assert!(validate_update(&claims(1, AdminRole::SuperAdmin), 1).is_ok());
}

#[test]
fn validate_update_super_admin_passes_for_other_account() {
    assert!(validate_update(&claims(1, AdminRole::SuperAdmin), 99).is_ok());
}

#[test]
fn validate_delete_rejects_self() {
    assert!(matches!(
        validate_delete(&claims(1, AdminRole::SuperAdmin), 1)
            .map_err(|error| axum::response::IntoResponse::into_response(error).status()),
        Err(axum::http::StatusCode::FORBIDDEN)
    ));
}

#[test]
fn validate_delete_passes_for_other_account() {
    assert!(validate_delete(&claims(1, AdminRole::SuperAdmin), 2).is_ok());
}

mod credential_tests {
    use super::*;
    #[test]
    fn credential_boundaries_count_unicode_characters() {
        for (username, valid) in [
            (String::new(), false),
            ("a".to_string(), true),
            ("é".repeat(20), true),
            ("é".repeat(21), false),
            (" ".to_string(), false),
            (" user".to_string(), false),
            ("user ".to_string(), false),
        ] {
            assert_eq!(validate_username(&username).is_ok(), valid);
        }
        for (length, valid) in [
            (0, false),
            (11, false),
            (12, true),
            (128, true),
            (129, false),
        ] {
            assert_eq!(validate_password(&"é".repeat(length)).is_ok(), valid);
        }
        assert!(validate_update_input(&UpdateAdminRequest::default()).is_err());
    }
}
