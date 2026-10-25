#![allow(clippy::unwrap_used)]
use super::*;
fn claims(admin_id: i64, role: AdminRole) -> AuthenticatedAdmin {
    AuthenticatedAdmin { id: admin_id, role }
}

#[test]
fn authorize_update_passes_when_ids_match() {
    assert!(authorize_update(&claims(1, AdminRole::Admin), 1).is_ok());
}

#[test]
fn authorize_update_returns_forbidden_when_ids_mismatch() {
    assert!(matches!(
        authorize_update(&claims(1, AdminRole::Admin), 2)
            .map_err(|error| axum::response::IntoResponse::into_response(error).status()),
        Err(axum::http::StatusCode::FORBIDDEN)
    ));
}

#[test]
fn authorize_update_super_admin_passes_for_self() {
    assert!(authorize_update(&claims(1, AdminRole::SuperAdmin), 1).is_ok());
}

#[test]
fn authorize_update_super_admin_passes_for_other_account() {
    assert!(authorize_update(&claims(1, AdminRole::SuperAdmin), 99).is_ok());
}

#[test]
fn authorize_delete_rejects_self() {
    assert!(matches!(
        authorize_delete(&claims(1, AdminRole::SuperAdmin), 1)
            .map_err(|error| axum::response::IntoResponse::into_response(error).status()),
        Err(axum::http::StatusCode::FORBIDDEN)
    ));
}

#[test]
fn authorize_delete_passes_for_other_account() {
    assert!(authorize_delete(&claims(1, AdminRole::SuperAdmin), 2).is_ok());
}
