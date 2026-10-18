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
            .map_err(|error| AppError::from(error).at(concat!(module_path!(), "::hash")))
    })
    .await
    .map_err(|error| {
        AppError::from(anyhow::Error::from(error)).at(concat!(module_path!(), "::hash"))
    })?
}

/// Verify a plaintext password against a PHC hash. `true` on match.
pub(crate) async fn verify(password: String, password_hash: String) -> Result<bool, AppError> {
    tokio::task::spawn_blocking(move || {
        let parsed_hash = PasswordHash::new(&password_hash)
            .map_err(|error| AppError::from(error).at(concat!(module_path!(), "::verify")))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    })
    .await
    .map_err(|error| {
        AppError::from(anyhow::Error::from(error)).at(concat!(module_path!(), "::verify"))
    })?
}

#[cfg(test)]
#[path = "../tests/utils/password.rs"]
mod tests;
