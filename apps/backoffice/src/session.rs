mod refresh;
mod storage;

use dioxus::fullstack::AsStatusCode;
use dioxus::prelude::*;
use domain::AdminRole;
use domain::dto::LoginRequest;
use serde::{Deserialize, Serialize};

use crate::api;

pub use refresh::ensure_valid;

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

pub fn current() -> Option<Session> {
    SESSION.read().clone()
}

fn save(session: Session) {
    storage::save(&session);
    *SESSION.write() = Some(session);
}

pub fn restore() {
    if SESSION.peek().is_none()
        && let Some(session) = storage::load()
    {
        *SESSION.write() = Some(session);
    }
}

pub async fn sign_in(username: String, password: String) -> Result<(), ServerFnError> {
    let tokens = api::login(LoginRequest { username, password }).await?;
    let admin = api::get_admin(tokens.id, tokens.access_token.clone()).await?;
    refresh::invalidate();
    save(Session {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at: refresh::expires_at(tokens.expires_in),
        admin_id: tokens.id,
        username: admin.username,
        role: admin.role,
    });
    Ok(())
}

pub fn sign_out() {
    refresh::invalidate();
    storage::clear();
    let token = SESSION.write().take().map(|session| session.access_token);
    if let Some(token) = token {
        spawn(async move {
            let _ = api::logout(token).await;
        });
    }
}

pub async fn with_session<F, Fut, T>(call: F) -> Result<T, ServerFnError>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = ServerFnResult<T>>,
{
    let session = ensure_valid().await?;
    call(session.access_token).await
}

pub fn handle_api_error(error: &ServerFnError) {
    if error.as_status_code() == 401 {
        sign_out();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn session_serializes_round_trip_for_persistence() {
        let session = Session {
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            expires_at: 1_700_000_000,
            admin_id: 1,
            username: "admin".into(),
            role: AdminRole::Admin,
        };
        let json = serde_json::to_string(&session).unwrap();
        let restored: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, session);
    }
}
