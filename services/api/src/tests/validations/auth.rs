use super::*;

#[test]
fn login_preserves_the_password_unicode_character_bound() {
    for (length, valid) in [(0, true), (128, true), (129, false)] {
        let result = login(&LoginRequest {
            username: "admin".into(),
            password: "é".repeat(length),
        });
        assert_eq!(result.is_ok(), valid);
        if let Err(error) = result {
            assert_eq!(
                axum::response::IntoResponse::into_response(error).status(),
                StatusCode::UNAUTHORIZED
            );
        }
    }
}
