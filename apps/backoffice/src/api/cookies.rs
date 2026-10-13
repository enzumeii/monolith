#![cfg(feature = "server")]
use crate::state::AppState;
use dioxus::fullstack::FullstackContext;
use dioxus::prelude::*;
use dioxus_server::axum::http::{
    HeaderMap, HeaderValue,
    header::{COOKIE, SET_COOKIE},
};
use domain::dto::TokenResponse;

pub(crate) struct Credentials {
    pub access: String,
    pub refresh: String,
}
fn name(state: &AppState) -> &'static str {
    if state.settings.session.insecure_local {
        "backoffice_auth"
    } else {
        "__Host-backoffice_auth"
    }
}
fn attributes(state: &AppState) -> &'static str {
    if state.settings.session.insecure_local {
        "HttpOnly; SameSite=Lax; Path=/"
    } else {
        "HttpOnly; Secure; SameSite=Lax; Path=/"
    }
}

pub(crate) async fn read(state: &AppState) -> ServerFnResult<Option<Credentials>> {
    let headers = FullstackContext::extract::<HeaderMap, _>().await?;
    let Some(cookie) = headers.get(COOKIE).and_then(|h| h.to_str().ok()) else {
        return Ok(None);
    };
    let key = name(state);
    let values: Vec<_> = cookie
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .filter(|(k, _)| *k == key)
        .collect();
    if values.len() != 1 {
        return Ok(None);
    }
    let Some((access, refresh)) = values[0].1.split_once('~') else {
        return Ok(None);
    };
    if access.len() > 3500
        || access.is_empty()
        || !access
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        || refresh.len() != 64
        || !refresh.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Ok(None);
    }
    Ok(Some(Credentials {
        access: access.into(),
        refresh: refresh.into(),
    }))
}
pub(crate) fn save(state: &AppState, tokens: &TokenResponse) -> ServerFnResult<()> {
    set(&format!(
        "{}={}~{}; {}; Max-Age={}",
        name(state),
        tokens.access_token,
        tokens.refresh_token,
        attributes(state),
        604_800
    ))
}
pub(crate) fn clear(state: &AppState) -> ServerFnResult<()> {
    set(&format!(
        "{}=; {}; Max-Age=0",
        name(state),
        attributes(state)
    ))
}
fn set(value: &str) -> ServerFnResult<()> {
    let context = FullstackContext::current()
        .ok_or_else(|| ServerFnError::new("missing response context"))?;
    let header = HeaderValue::from_str(value)
        .map_err(|_| ServerFnError::new("invalid authentication response"))?;
    context.add_response_header(SET_COOKIE, header);
    Ok(())
}
