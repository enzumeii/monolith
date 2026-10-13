#![allow(clippy::unwrap_used)]
use super::*;

#[tokio::test]
async fn hash_produces_a_valid_verifiable_hash() {
    let hash = hash("my-password".into()).await.unwrap();
    assert_ne!(hash, "my-password");
    let parsed = PasswordHash::new(&hash).unwrap();
    assert!(
        Argon2::default()
            .verify_password(b"my-password", &parsed)
            .is_ok()
    );
    assert!(
        Argon2::default()
            .verify_password(b"wrong-password", &parsed)
            .is_err()
    );
}

#[tokio::test]
async fn verify_returns_true_for_correct_password() {
    let hash = hash("correct".into()).await.unwrap();
    assert!(verify("correct".into(), hash).await.unwrap());
}

#[tokio::test]
async fn verify_returns_false_for_wrong_password() {
    let hash = hash("correct".into()).await.unwrap();
    assert!(!verify("wrong".into(), hash).await.unwrap());
}

#[tokio::test]
async fn verify_returns_error_for_malformed_hash() {
    let result = verify("any".into(), "not-a-valid-hash".into()).await;
    assert!(result.is_err());
}
