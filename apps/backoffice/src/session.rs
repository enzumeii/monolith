mod sync;
use crate::api;
use dioxus::fullstack::AsStatusCode;
use dioxus::prelude::*;
use domain::{
    AdminRole,
    dto::{LoginRequest, ViewerResponse},
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub admin_id: i64,
    pub username: String,
    pub role: AdminRole,
    pub expires_at: u64,
}
impl From<ViewerResponse> for Session {
    fn from(viewer: ViewerResponse) -> Self {
        Self {
            admin_id: viewer.admin.id,
            username: viewer.admin.username,
            role: viewer.admin.role,
            expires_at: viewer.expires_at,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Authenticated(Session),
    NeedsRefresh,
    Anonymous,
}
pub static SESSION: GlobalSignal<Option<Session>> = GlobalSignal::new(|| None);
pub static READY: GlobalSignal<bool> = GlobalSignal::new(|| false);
pub static RESTORE_ERROR: GlobalSignal<Option<String>> = GlobalSignal::new(|| None);

#[derive(Clone, Copy, PartialEq, Eq)]
struct Ticket {
    epoch: u64,
    sequence: u64,
}
#[derive(Default)]
struct ResponseOwner {
    epoch: u64,
    sequence: u64,
}
impl ResponseOwner {
    fn request(&mut self) -> Ticket {
        self.sequence = self.sequence.wrapping_add(1);
        Ticket {
            epoch: self.epoch,
            sequence: self.sequence,
        }
    }
    fn transition(&mut self) -> Ticket {
        self.epoch = self.epoch.wrapping_add(1);
        self.request()
    }
    fn owns(&self, ticket: Ticket) -> bool {
        self.epoch == ticket.epoch && self.sequence == ticket.sequence
    }
}
thread_local! {static OWNER:RefCell<ResponseOwner>=RefCell::new(ResponseOwner::default());}
fn transition() -> Ticket {
    OWNER.with(|owner| owner.borrow_mut().transition())
}
fn request() -> Ticket {
    OWNER.with(|owner| owner.borrow_mut().request())
}
fn owns(ticket: Ticket) -> bool {
    OWNER.with(|owner| owner.borrow().owns(ticket))
}
fn epoch() -> u64 {
    OWNER.with(|owner| owner.borrow().epoch)
}
pub(crate) fn now_unix() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
fn apply(ticket: Ticket, session: Option<Session>) {
    if owns(ticket) {
        *SESSION.write() = session;
        *READY.write() = true;
        *RESTORE_ERROR.write() = None;
    }
}

async fn read_valid() -> ServerFnResult<Option<Session>> {
    match api::current_session().await? {
        SessionStatus::Authenticated(session) => Ok(Some(session)),
        SessionStatus::Anonymous => Ok(None),
        SessionStatus::NeedsRefresh => match api::refresh_session().await {
            Ok(viewer) => {
                sync::notify();
                Ok(Some(viewer.into()))
            }
            Err(error) if error.as_status_code() == 401 => {
                // A concurrent BFF request may already have updated the cookie.
                if let SessionStatus::Authenticated(session) = api::current_session().await? {
                    return Ok(Some(session));
                }
                api::clear_session_cookie().await?;
                Ok(None)
            }
            Err(error) => Err(error),
        },
    }
}

pub async fn restore() -> ServerFnResult<()> {
    let ticket = request();
    let result = async {
        let _lock = sync::lock().await?;
        read_valid().await
    }
    .await;
    match result {
        Ok(session) => {
            apply(ticket, session);
            Ok(())
        }
        Err(error) => {
            if owns(ticket) {
                *READY.write() = true;
                *RESTORE_ERROR.write() = Some(error.to_string());
            }
            Err(error)
        }
    }
}

pub async fn sign_in(username: String, password: String) -> ServerFnResult<()> {
    let ticket = transition();
    let _lock = sync::lock().await?;
    let viewer = api::login(LoginRequest { username, password }).await?;
    apply(ticket, Some(viewer.into()));
    sync::notify();
    Ok(())
}

pub async fn sign_out() -> ServerFnResult<()> {
    let ticket = transition();
    let _lock = sync::lock().await?;
    if read_valid().await?.is_some() {
        api::logout().await?;
    } else {
        api::clear_session_cookie().await?;
    }
    apply(ticket, None);
    sync::notify();
    Ok(())
}

pub async fn update_self(
    id: i64,
    req: domain::dto::UpdateAdminRequest,
) -> ServerFnResult<domain::dto::AdminDTO> {
    let ticket = transition();
    let _lock = sync::lock().await?;
    let current = read_valid()
        .await?
        .ok_or_else(|| ServerFnError::new("not signed in"))?;
    if current.admin_id != id {
        return Err(ServerFnError::new("Account changed; reload this form"));
    }
    let updated = api::update_admin(id, req).await?;
    api::clear_session_cookie()
        .await
        .map_err(|_| ServerFnError::new("Account saved; sign-out needs retry"))?;
    apply(ticket, None);
    sync::notify();
    Ok(updated)
}

async fn ensure_valid() -> ServerFnResult<()> {
    if SESSION
        .read()
        .as_ref()
        .is_some_and(|session| session.expires_at > now_unix() + 3)
    {
        return Ok(());
    }
    restore().await?;
    if SESSION.read().is_none() {
        return Err(ServerFnError::new("not signed in"));
    }
    Ok(())
}

pub async fn with_session<F, Fut, T>(call: F) -> ServerFnResult<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ServerFnResult<T>>,
{
    let version = epoch();
    ensure_valid().await?;
    let result = call().await;
    if epoch() != version {
        return Err(ServerFnError::new("session changed during request"));
    }
    if let Err(error) = &result
        && error.as_status_code() == 401
    {
        let _ = restore().await;
    }
    result
}

pub fn install_sync() {
    sync::install();
}
#[cfg(test)]
mod ownership_tests {
    #[test]
    fn old_restoration_cannot_restore_logged_out_or_new_account_state() {
        let mut owner = super::ResponseOwner::default();
        let old = owner.request();
        owner.transition();
        assert!(!owner.owns(old));
        let first = owner.request();
        let latest = owner.request();
        assert!(!owner.owns(first));
        assert!(owner.owns(latest));
        owner.transition();
        assert!(!owner.owns(latest));
    }
}
