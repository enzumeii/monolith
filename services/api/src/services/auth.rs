use crate::{
    error::AppError,
    models::Admin,
    models::Claims,
    repositories::{admin, auth},
    utils::password,
};
use chrono::{Duration, Utc};
use domain::dto::{LoginRequest, RefreshRequest, TokenResponse};
use jsonwebtoken::{EncodingKey, Header, encode};
use rand::RngExt;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};

#[tracing::instrument(skip_all)]
pub async fn login(
    pool: &PgPool,
    jwt_secret: &[u8],
    req: LoginRequest,
) -> Result<TokenResponse, AppError> {
    if req.password.chars().count() > 128 {
        return Err(AppError::InvalidCredentials.at(concat!(module_path!(), "::login")));
    }
    let account = admin::find_by_username(pool, &req.username)
        .await?
        .ok_or_else(|| AppError::InvalidCredentials.at(concat!(module_path!(), "::login")))?;
    if !password::verify(req.password, account.password_hash.clone()).await? {
        return Err(AppError::InvalidCredentials.at(concat!(module_path!(), "::login")));
    }
    let mut transaction = pool
        .begin()
        .await
        .map_err(|error| AppError::from(error).at(concat!(module_path!(), "::login")))?;
    let tokens = issue_token_pair(&mut transaction, jwt_secret, &account, None).await?;
    transaction
        .commit()
        .await
        .map_err(|error| AppError::from(error).at(concat!(module_path!(), "::login")))?;
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
        .ok_or_else(|| AppError::InvalidToken.at(concat!(module_path!(), "::refresh")))?;
    let account = admin::find_by_id(pool, id)
        .await?
        .ok_or_else(|| AppError::InvalidToken.at(concat!(module_path!(), "::refresh")))?;
    let mut transaction = pool
        .begin()
        .await
        .map_err(|error| AppError::from(error).at(concat!(module_path!(), "::refresh")))?;
    let family_id = auth::consume_refresh_token(&mut transaction, &hash, id)
        .await?
        .ok_or_else(|| AppError::InvalidToken.at(concat!(module_path!(), "::refresh")))?;
    let tokens = issue_token_pair(&mut transaction, jwt_secret, &account, Some(family_id)).await?;
    transaction
        .commit()
        .await
        .map_err(|error| AppError::from(error).at(concat!(module_path!(), "::refresh")))?;
    Ok(tokens)
}

pub async fn logout(
    pool: &PgPool,
    actor: &crate::models::AuthenticatedAdmin,
    req: RefreshRequest,
) -> Result<(), AppError> {
    let mut connection = pool
        .acquire()
        .await
        .map_err(|error| AppError::from(error).at(concat!(module_path!(), "::logout")))?;
    auth::delete_refresh_family(&mut connection, actor.id, &hash_token(&req.refresh_token)).await?;
    Ok(())
}

pub(crate) fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

async fn issue_token_pair(
    connection: &mut PgConnection,
    jwt_secret: &[u8],
    account: &Admin,
    family_id: Option<uuid::Uuid>,
) -> Result<TokenResponse, AppError> {
    let expiration =
        u64::try_from((Utc::now() + Duration::minutes(10)).timestamp()).map_err(|_| {
            AppError::from(anyhow::anyhow!("timestamp overflow"))
                .at(concat!(module_path!(), "::issue_token_pair"))
        })?;
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
    )
    .map_err(|error| AppError::from(error).at(concat!(module_path!(), "::issue_token_pair")))?;
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    let refresh_token = hex::encode(bytes);
    auth::create_refresh_token(
        connection,
        account.id,
        &hash_token(&refresh_token),
        Utc::now() + Duration::days(7),
        family_id,
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

#[cfg(test)]
#[path = "../tests/services/auth.rs"]
mod tests;
