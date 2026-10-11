use dioxus::fullstack::AsStatusCode;
use dioxus::prelude::*;
use domain::AdminRole;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub admin_id: i64,
    pub username: String,
    pub role: AdminRole,
}

/// The current signed-in session, if any.
pub static SESSION: GlobalSignal<Option<Session>> = GlobalSignal::new(|| None);

/// Sign in: store the session.
pub fn sign_in(session: Session) {
    *SESSION.write() = Some(session);
}

/// Sign out: clear the session (and any in-flight state returns to login).
pub fn sign_out() {
    *SESSION.write() = None;
}

/// Current session snapshot, if signed in.
pub fn current() -> Option<Session> {
    SESSION.read().clone()
}

/// Replace the stored session with a freshly-rotated one, but only if the
/// access token actually changed. Idempotent so reactive effects that read
/// `SESSION` don't re-trigger in a loop after a refresh.
pub fn adopt(new: Session) {
    let changed = match &*SESSION.peek() {
        Some(cur) => cur.access_token != new.access_token,
        None => true,
    };
    if changed {
        *SESSION.write() = Some(new);
    }
}

/// React to an API error: a 401 means the session is no longer valid (refresh
/// failed or the token was revoked server-side), so drop to the login screen.
pub fn handle_api_error(e: &ServerFnError) {
    if e.as_status_code() == 401 {
        sign_out();
    }
}
