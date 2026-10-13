//! Stateless credential crypto: hashing and verifying passwords with Argon2.
//! No database, no HTTP — plain functions over values.

use crate::error::AppError;
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};

#[derive(Clone)]
pub struct PasswordLimiter {
    gate: std::sync::Arc<tokio::sync::Semaphore>,
}
impl PasswordLimiter {
    #[must_use]
    pub fn new(concurrency: usize) -> Self {
        Self {
            gate: std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency)),
        }
    }
    async fn run<T: Send + 'static>(
        &self,
        job: impl FnOnce() -> Result<T, AppError> + Send + 'static,
    ) -> Result<T, AppError> {
        let permit = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            self.gate.clone().acquire_owned(),
        )
        .await
        .map_err(|_| AppError::Unavailable)?
        .map_err(|_| AppError::Unavailable)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            job()
        })
        .await
        .map_err(|error| AppError::Internal(error.into()))?
    }
}
/// Hash a plaintext password with Argon2. Returns the PHC string.
pub(crate) async fn hash(limiter: &PasswordLimiter, password: String) -> Result<String, AppError> {
    limiter
        .run(move || {
            let salt = SaltString::generate(&mut OsRng);
            Argon2::default()
                .hash_password(password.as_bytes(), &salt)
                .map(|hash| hash.to_string())
                .map_err(AppError::from)
        })
        .await
}

/// Verify a plaintext password against a PHC hash. `true` on match.
pub(crate) async fn verify(
    limiter: &PasswordLimiter,
    password: String,
    password_hash: String,
) -> Result<bool, AppError> {
    limiter
        .run(move || {
            let parsed_hash = PasswordHash::new(&password_hash)?;
            Ok(Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok())
        })
        .await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_produces_a_valid_verifiable_hash() {
        let hash = hash(&PasswordLimiter::new(2), "my-password".into())
            .await
            .unwrap();
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
        let hash = hash(&PasswordLimiter::new(2), "correct".into())
            .await
            .unwrap();
        assert!(
            verify(&PasswordLimiter::new(2), "correct".into(), hash)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn verify_returns_false_for_wrong_password() {
        let hash = hash(&PasswordLimiter::new(2), "correct".into())
            .await
            .unwrap();
        assert!(
            !verify(&PasswordLimiter::new(2), "wrong".into(), hash)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn verify_returns_error_for_malformed_hash() {
        let result = verify(
            &PasswordLimiter::new(2),
            "any".into(),
            "not-a-valid-hash".into(),
        )
        .await;
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_keeps_permit_until_blocking_job_finishes() {
        let limiter = PasswordLimiter::new(1);
        let worker = limiter.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            worker
                .run(move || {
                    started_tx.send(()).expect("started");
                    release_rx.recv().expect("release");
                    Ok(())
                })
                .await
        });
        started_rx.await.expect("work started");
        task.abort();
        let _ = task.await;
        let available = limiter.gate.available_permits();
        release_tx.send(()).expect("release work");
        assert_eq!(available, 0, "cancelled waiter released running job permit");
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while limiter.gate.available_permits() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("permit released on completion");
    }
    #[tokio::test]
    async fn saturated_admission_is_retryable() {
        let limiter = PasswordLimiter::new(1);
        let permit = limiter.gate.acquire().await.expect("permit");
        assert!(matches!(
            limiter.run(|| Ok(())).await,
            Err(AppError::Unavailable)
        ));
        drop(permit);
    }
}
