use crate::{
    auth::Claims,
    error::AppError,
    models::Admin,
    repositories::{admin, auth},
    utils::password,
};
use chrono::{Duration, Utc};
use domain::dto::{LoginRequest, RefreshRequest, TokenResponse};
use jsonwebtoken::{EncodingKey, Header, encode};
use rand::RngExt;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};

pub(crate) async fn verified_account(pool: &PgPool, req: LoginRequest) -> Result<Admin, AppError> {
    if req.password.chars().count() > 128 {
        return Err(AppError::InvalidCredentials);
    }
    let account = admin::find_by_username(pool, &req.username)
        .await?
        .ok_or(AppError::InvalidCredentials)?;
    if !password::verify(req.password, account.password_hash.clone()).await? {
        return Err(AppError::InvalidCredentials);
    }
    Ok(account)
}

pub(crate) async fn lock_verified(
    connection: &mut PgConnection,
    observed: &Admin,
) -> Result<Admin, AppError> {
    let current = admin::lock(connection, observed.id)
        .await?
        .ok_or(AppError::InvalidCredentials)?;
    if current.password_hash != observed.password_hash {
        return Err(AppError::InvalidCredentials);
    }
    Ok(current)
}

#[tracing::instrument(skip_all)]
pub async fn login(
    pool: &PgPool,
    jwt_secret: &[u8],
    req: LoginRequest,
) -> Result<TokenResponse, AppError> {
    let observed = verified_account(pool, req).await?;
    let mut transaction = pool.begin().await?;
    let account = lock_verified(&mut transaction, &observed).await?;
    let tokens = issue_token_pair(&mut transaction, jwt_secret, &account).await?;
    transaction.commit().await?;
    Ok(tokens)
}

#[tracing::instrument(skip_all)]
pub async fn refresh(
    pool: &PgPool,
    jwt_secret: &[u8],
    req: RefreshRequest,
) -> Result<TokenResponse, AppError> {
    let hash = hash_token(&req.refresh_token);
    let id = auth::refresh_owner(pool, &hash)
        .await?
        .ok_or(AppError::InvalidToken)?;
    let mut transaction = pool.begin().await?;
    let account = admin::lock(&mut transaction, id)
        .await?
        .ok_or(AppError::InvalidToken)?;
    if !auth::consume_refresh_token(&mut transaction, &hash, id).await? {
        return Err(AppError::InvalidToken);
    }
    let tokens = issue_token_pair(&mut transaction, jwt_secret, &account).await?;
    transaction.commit().await?;
    Ok(tokens)
}

pub async fn logout(
    pool: &PgPool,
    actor: &crate::auth::AuthenticatedAdmin,
) -> Result<(), AppError> {
    let mut transaction = pool.begin().await?;
    let _account = admin::lock(&mut transaction, actor.id)
        .await?
        .ok_or(AppError::InvalidToken)?;
    auth::revoke_account(&mut transaction, actor.id).await?;
    transaction.commit().await?;
    Ok(())
}

async fn issue_token_pair(
    connection: &mut PgConnection,
    jwt_secret: &[u8],
    account: &Admin,
) -> Result<TokenResponse, AppError> {
    let expiration = u64::try_from((Utc::now() + Duration::minutes(10)).timestamp())
        .map_err(|_| anyhow::anyhow!("timestamp overflow"))?;
    let claims = Claims {
        admin_id: account.id,
        username: account.username.clone(),
        role: account.role.into(),
        expiration,
    };
    let access_token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret),
    )?;
    let refresh_token = new_token();
    auth::create_refresh_token(
        connection,
        account.id,
        &hash_token(&refresh_token),
        Utc::now() + Duration::days(7),
    )
    .await?;
    Ok(TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".into(),
        expires_in: 600,
        id: account.id,
    })
}

pub(crate) fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    hex::encode(bytes)
}
pub(crate) fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hash_token_is_deterministic_and_different_from_input() {
        assert_eq!(hash_token("test-token"), hash_token("test-token"));
        assert_ne!(hash_token("test-token"), "test-token");
    }
}
