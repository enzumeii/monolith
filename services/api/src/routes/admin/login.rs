use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{Json, extract::State, http::StatusCode};
use chrono::{Duration, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use models::{
    dto::{AuthResponse, Claims, LoginPayload},
    entities::admin::Admin,
};

use crate::state::AppState;

pub async fn login_handler(
    State(state): State<AppState>,
    Json(payload): Json<LoginPayload>,
) -> Result<Json<AuthResponse>, (StatusCode, &'static str)> {
    let admin = sqlx::query_as!(
        Admin,
        r#"
        SELECT id, username, password_hash, created_at, updated_at
        FROM admin
        WHERE username = $1
        "#,
        payload.username
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?
    .ok_or((StatusCode::UNAUTHORIZED, "Invalid username or password"))?;

    // 2. Verify password hash using argon2
    let parsed_hash = PasswordHash::new(&admin.password_hash).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Password verification error",
        )
    })?;
    Argon2::default()
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid username or password"))?;

    // 3. Issue JWT Token (valid for 24 hours)
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("valid timestamp")
        .timestamp() as usize;
    let claims = Claims {
        sub: admin.id.to_string(),
        username: admin.username,
        exp: expiration,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.settings.jwt.secret.as_bytes()),
    )
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Token generation failed"))?;

    Ok(Json(AuthResponse {
        token_type: "Bearer".to_string(),
        access_token: token,
    }))
}

// // Auth Guard Middleware
// pub async fn auth_middleware(
//     AuthBearer(token): AuthBearer,
//     mut req: Request,
//     next: Next,
// ) -> Result<Response, StatusCode> {
//     let token_data = decode::<Claims>(
//         &token,
//         &DecodingKey::from_secret(JWT_SECRET),
//         &Validation::default(),
//     )
//     .map_err(|_| StatusCode::UNAUTHORIZED)?;
//
//     req.extensions_mut().insert(token_data.claims);
//     Ok(next.run(req).await)
// }
