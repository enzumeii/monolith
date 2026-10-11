//! Stateless credential crypto: hashing and verifying passwords with Argon2.
//! No database, no HTTP — plain functions over values.

use crate::error::AppError;
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};

/// Hash a plaintext password with Argon2. Returns the PHC string.
pub(crate) async fn hash(password: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(AppError::from)
    })
    .await
    .map_err(|error| AppError::Internal(anyhow::Error::msg(error.to_string())))?
}

/// Verify a plaintext password against a PHC hash. `true` on match.
pub(crate) async fn verify(password: String, password_hash: String) -> Result<bool, AppError> {
    tokio::task::spawn_blocking(move || {
        let parsed_hash = PasswordHash::new(&password_hash)?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    })
    .await
    .map_err(|error| AppError::Internal(anyhow::Error::msg(error.to_string())))?
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
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
}
