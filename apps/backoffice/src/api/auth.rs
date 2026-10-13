use crate::session::SessionStatus;
#[cfg(feature = "server")]
use dioxus::fullstack::AsStatusCode;
use dioxus::prelude::*;
#[cfg(feature = "server")]
use domain::dto::TokenResponse;
use domain::dto::{LoginRequest, ViewerResponse};

#[server(endpoint = "auth/login")]
pub async fn login(req: LoginRequest) -> ServerFnResult<ViewerResponse> {
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
    super::cookies::save(&state, &tokens)?;
    Ok(viewer)
}

#[server(endpoint = "auth/current")]
pub async fn current_session() -> ServerFnResult<SessionStatus> {
    let state = crate::state::app_state();
    let Some(credentials) = super::cookies::read(&state).await? else {
        return Ok(SessionStatus::Anonymous);
    };
    match viewer_for(&state, &credentials.access).await {
        Ok(viewer) if viewer.expires_at > crate::session::now_unix() => {
            Ok(SessionStatus::Authenticated(viewer.into()))
        }
        Ok(_) => Ok(SessionStatus::NeedsRefresh),
        Err(error) if error.as_status_code() == 401 => Ok(SessionStatus::NeedsRefresh),
        Err(error) if error.as_status_code() == 404 => Ok(SessionStatus::Anonymous),
        Err(error) => Err(error),
    }
}

#[server(endpoint = "auth/refresh")]
pub async fn refresh_session() -> ServerFnResult<ViewerResponse> {
    let state = crate::state::app_state();
    let credentials = super::cookies::read(&state)
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
    super::cookies::save(&state, &tokens)?;
    Ok(viewer)
}

#[server(endpoint = "auth/logout")]
pub async fn logout() -> ServerFnResult<()> {
    let state = crate::state::app_state();
    let Some(credentials) = super::cookies::read(&state).await? else {
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
    super::cookies::clear(&state)
}

// Server RPC signatures stay async on both platforms.
#[allow(clippy::unused_async)]
#[server(endpoint = "auth/clear")]
pub async fn clear_session_cookie() -> ServerFnResult<()> {
    super::cookies::clear(&crate::state::app_state())
}

#[cfg(feature = "server")]
async fn viewer_for(state: &crate::state::AppState, token: &str) -> ServerFnResult<ViewerResponse> {
    let response = state
        .http
        .get(state.url("/auth/me"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| super::client::unavailable())?;
    super::client::decode(response, "viewer").await
}
