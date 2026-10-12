//! Login session: identity, token storage, expiry, and refresh coordination.
//! Held only in browser memory — reloading the page signs you out.

use std::cell::{Cell, RefCell};

use dioxus::fullstack::AsStatusCode;
use dioxus::prelude::*;
use domain::AdminRole;
use domain::dto::{LoginRequest, RefreshRequest};
use futures::future::{FutureExt, LocalBoxFuture, Shared};
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use gloo_storage::Storage;

use crate::api;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub admin_id: i64,
    pub username: String,
    pub role: AdminRole,
}

pub static SESSION: GlobalSignal<Option<Session>> = GlobalSignal::new(|| None);

#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "backoffice.session";

#[cfg(target_arch = "wasm32")]
fn persist_save(session: &Session) {
    let _ = gloo_storage::LocalStorage::set(STORAGE_KEY, session);
}

#[cfg(not(target_arch = "wasm32"))]
fn persist_save(_session: &Session) {}

#[cfg(target_arch = "wasm32")]
fn persist_load() -> Option<Session> {
    gloo_storage::LocalStorage::get(STORAGE_KEY).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn persist_load() -> Option<Session> {
    None
}

#[cfg(target_arch = "wasm32")]
fn persist_clear() {
    gloo_storage::LocalStorage::delete(STORAGE_KEY);
}

#[cfg(not(target_arch = "wasm32"))]
fn persist_clear() {}

type SharedRefresh = Shared<LocalBoxFuture<'static, Result<Session, ServerFnError>>>;

thread_local! {
    static GENERATION: Cell<u64> = const { Cell::new(0) };
    static REFRESH_IN_FLIGHT: RefCell<Option<SharedRefresh>> = const { RefCell::new(None) };
}

fn generation() -> u64 {
    GENERATION.with(Cell::get)
}

fn bump_generation() {
    GENERATION.with(|g| g.set(g.get() + 1));
}

fn now_unix() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn is_expired(session: &Session) -> bool {
    now_unix() >= session.expires_at
}

pub fn current() -> Option<Session> {
    SESSION.read().clone()
}

/// Restore a persisted session from browser storage (no-op off-wasm). Call once
/// on mount; token expiry is handled lazily by `ensure_valid`.
pub fn restore() {
    if SESSION.peek().is_none()
        && let Some(session) = persist_load()
    {
        *SESSION.write() = Some(session);
    }
}

pub async fn sign_in(username: String, password: String) -> Result<(), ServerFnError> {
    let tokens = api::login(LoginRequest { username, password }).await?;
    let admin = api::get_admin(tokens.id, tokens.access_token.clone()).await?;
    bump_generation();
    let session = Session {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at: now_unix() + tokens.expires_in,
        admin_id: tokens.id,
        username: admin.username,
        role: admin.role,
    };
    persist_save(&session);
    *SESSION.write() = Some(session);
    Ok(())
}

pub fn sign_out() {
    bump_generation();
    persist_clear();
    let token = SESSION.write().take().map(|s| s.access_token);
    if let Some(token) = token {
        spawn(async move {
            let _ = api::logout(token).await;
        });
    }
}

/// Return a session whose access token is valid, refreshing once if expired.
/// Concurrent callers share the single in-flight refresh, and a session signed
/// out mid-refresh is not resurrected.
pub async fn ensure_valid() -> Result<Session, ServerFnError> {
    let cur = current().ok_or_else(|| ServerFnError::new("not signed in"))?;
    if !is_expired(&cur) {
        return Ok(cur);
    }

    if let Some(fut) = REFRESH_IN_FLIGHT.with(|c| c.borrow().clone()) {
        return fut.await;
    }

    let generation_before = generation();
    let refresh_token = cur.refresh_token.clone();
    let prev = cur;
    let fut = async move {
        let tokens = api::refresh(RefreshRequest { refresh_token }).await?;
        if generation() != generation_before {
            return Err(ServerFnError::new("session ended during refresh"));
        }
        let session = Session {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at: now_unix() + tokens.expires_in,
            admin_id: prev.admin_id,
            username: prev.username,
            role: prev.role,
        };
        persist_save(&session);
        *SESSION.write() = Some(session.clone());
        Ok(session)
    }
    .boxed_local()
    .shared();

    REFRESH_IN_FLIGHT.with(|c| *c.borrow_mut() = Some(fut.clone()));
    let result = fut.await;
    REFRESH_IN_FLIGHT.with(|c| *c.borrow_mut() = None);
    result
}

/// Run an upstream call with a valid bearer token, refreshing first if needed.
pub async fn with_session<F, Fut, T>(call: F) -> Result<T, ServerFnError>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = ServerFnResult<T>>,
{
    let session = ensure_valid().await?;
    call(session.access_token).await
}

/// A 401 means the session is no longer valid — drop to the login screen.
pub fn handle_api_error(e: &ServerFnError) {
    if e.as_status_code() == 401 {
        sign_out();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn session(expires_at: i64) -> Session {
        Session {
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            expires_at,
            admin_id: 1,
            username: "admin".into(),
            role: AdminRole::Admin,
        }
    }

    #[test]
    fn expiry_is_true_at_or_past_expires_at_and_false_before() {
        assert!(is_expired(&session(now_unix() - 1)));
        assert!(
            is_expired(&session(now_unix())),
            "expires_at == now is expired"
        );
        assert!(!is_expired(&session(now_unix() + 10_000)));
    }

    #[test]
    fn session_serializes_round_trip_for_persistence() {
        let session = session(1_700_000_000);
        let json = serde_json::to_string(&session).unwrap();
        let restored: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, session);
    }
}
