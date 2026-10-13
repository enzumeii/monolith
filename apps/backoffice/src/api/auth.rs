use crate::session::{Session, SessionStatus};
#[cfg(feature = "server")]
use dioxus::fullstack::AsStatusCode;
use dioxus::prelude::*;
use domain::dto::LoginRequest;
#[cfg(feature = "server")]
use domain::dto::TokenResponse;

#[server(endpoint = "auth/login")]
pub async fn login(req: LoginRequest) -> ServerFnResult<Session> {
    let state = crate::state::app_state();
    let response = state
        .http
        .post(state.url("/auth/login"))
        .json(&req)
        .send()
        .await
        .map_err(|_| super::client::unavailable())?;
    let tokens: TokenResponse = super::client::decode(response, "login").await?;
    let viewer = viewer_for(&state, &tokens.access_token).await?;
    crate::session::cookies::save(&state, &tokens)?;
    Ok(viewer)
}

#[server(endpoint = "auth/current")]
pub async fn current_session() -> ServerFnResult<SessionStatus> {
    let state = crate::state::app_state();
    let Some(credentials) = crate::session::cookies::read(&state).await? else {
        return Ok(SessionStatus::Anonymous);
    };
    match viewer_for(&state, &credentials.access).await {
        Ok(viewer) if viewer.expires_at > crate::session::now_unix() => {
            Ok(SessionStatus::Authenticated(viewer))
        }
        Ok(_) => Ok(SessionStatus::NeedsRefresh),
        Err(error) if error.as_status_code() == 401 => Ok(SessionStatus::NeedsRefresh),
        Err(error) if error.as_status_code() == 404 => Ok(SessionStatus::Anonymous),
        Err(error) => Err(error),
    }
}

#[server(endpoint = "auth/refresh")]
pub async fn refresh_session() -> ServerFnResult<Session> {
    let state = crate::state::app_state();
    let credentials = crate::session::cookies::read(&state)
        .await?
        .ok_or_else(|| super::client::status_error(401))?;
    let tokens = state
        .refresh
        .rotate(
            state.http.clone(),
            state.url("/auth/refresh"),
            credentials.refresh,
        )
        .await?;
    let viewer = viewer_for(&state, &tokens.access_token).await?;
    crate::session::cookies::save(&state, &tokens)?;
    Ok(viewer)
}

#[server(endpoint = "auth/logout")]
pub async fn logout() -> ServerFnResult<()> {
    let state = crate::state::app_state();
    let Some(credentials) = crate::session::cookies::read(&state).await? else {
        return Ok(());
    };
    let response = state
        .http
        .post(state.url("/auth/logout"))
        .bearer_auth(credentials.access)
        .json(&domain::dto::RefreshRequest {
            refresh_token: credentials.refresh,
        })
        .send()
        .await
        .map_err(|_| super::client::unavailable())?;
    if !response.status().is_success() {
        return Err(super::client::status_error(response.status().as_u16()));
    }
    crate::session::cookies::clear(&state)
}

// Server RPC signatures stay async on both platforms.
#[allow(clippy::unused_async)]
#[server(endpoint = "auth/clear")]
pub async fn clear_session_cookie() -> ServerFnResult<()> {
    crate::session::cookies::clear(&crate::state::app_state())
}

#[cfg(feature = "server")]
async fn viewer_for(state: &crate::state::AppState, token: &str) -> ServerFnResult<Session> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    #[derive(serde::Deserialize)]
    struct Identity {
        sub: i64,
        exp: u64,
    }
    // Parsing is not authentication: the API validates the bearer before returning this profile.
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| super::client::status_error(401))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| super::client::status_error(401))?;
    let identity: Identity =
        serde_json::from_slice(&bytes).map_err(|_| super::client::status_error(401))?;
    let response = state
        .http
        .get(state.url(&format!("/admins/{}", identity.sub)))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| super::client::unavailable())?;
    let admin: domain::dto::AdminDTO = super::client::decode(response, "profile").await?;
    if admin.id != identity.sub {
        return Err(ServerFnError::new("Invalid profile response"));
    }
    Ok(Session {
        admin_id: admin.id,
        username: admin.username,
        role: admin.role,
        expires_at: identity.exp,
    })
}
