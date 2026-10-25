mod sync;
use crate::api;
use dioxus::prelude::*;
use domain::dto::LoginRequest;
use reqwest::StatusCode;
use std::cell::RefCell;

pub use domain::dto::SessionResponse as Session;
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

thread_local! {
    static OWNER: RefCell<ResponseOwner> = RefCell::new(ResponseOwner::default());
}
pub(crate) fn now_unix() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
pub async fn restore() -> api::ApiResult<()> {
    let _loading = ui::loading_guard();
    let ticket = request();
    let result = async {
        let _lock = sync::lock().await?;
        restore_session_from_cookie().await
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
pub async fn sign_in(username: String, password: String) -> api::ApiResult<()> {
    let _loading = ui::loading_guard();
    let ticket = transition();
    let _lock = sync::lock().await?;
    let viewer = api::login(LoginRequest { username, password }).await?;
    apply(ticket, Some(viewer));
    sync::notify();
    Ok(())
}
pub async fn sign_out() -> api::ApiResult<()> {
    let _loading = ui::loading_guard();
    let ticket = transition();
    let _lock = sync::lock().await?;
    if restore_session_from_cookie().await?.is_some() {
        api::logout().await?;
    }
    apply(ticket, None);
    sync::notify();
    Ok(())
}
pub async fn update_self(
    id: i64,
    req: domain::dto::UpdateAdminRequest,
) -> api::ApiResult<domain::dto::AdminDTO> {
    let _loading = ui::loading_guard();
    let ticket = transition();
    let _lock = sync::lock().await?;
    let current = restore_session_from_cookie()
        .await?
        .ok_or_else(|| api::ApiError::new(StatusCode::UNAUTHORIZED, "not signed in"))?;
    if current.admin_id != id {
        return Err(api::ApiError::new(
            StatusCode::CONFLICT,
            "Account changed; reload this form",
        ));
    }
    let updated = api::update_admin(id, req).await?;
    apply(ticket, None);
    sync::notify();
    Ok(updated)
}
pub async fn with_session<F, Fut, T>(call: F) -> api::ApiResult<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = api::ApiResult<T>>,
{
    let _loading = ui::loading_guard();
    let version = epoch();
    ensure_valid().await?;
    let result = call().await;
    if epoch() != version {
        return Err(api::ApiError::new(
            StatusCode::CONFLICT,
            "session changed during request",
        ));
    }
    if let Err(error) = &result
        && error.status == 401
    {
        let _ = restore().await;
    }
    result
}

pub fn install_sync() {
    sync::install();
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

fn apply(ticket: Ticket, session: Option<Session>) {
    if owns(ticket) {
        *SESSION.write() = session;
        *READY.write() = true;
        *RESTORE_ERROR.write() = None;
    }
}

async fn restore_session_from_cookie() -> api::ApiResult<Option<Session>> {
    match api::current_session().await {
        Ok(session) if session.expires_at > now_unix() + 3 => return Ok(Some(session)),
        Ok(_) => {}
        Err(error) if error.status == 401 => {}
        Err(error) => return Err(error),
    }
    match api::refresh_session().await {
        Ok(session) => Ok(Some(session)),
        Err(error) if error.status == 401 => Ok(None),
        Err(error) => Err(error),
    }
}

async fn ensure_valid() -> api::ApiResult<()> {
    if SESSION
        .peek()
        .as_ref()
        .is_some_and(|session| session.expires_at > now_unix() + 3)
    {
        return Ok(());
    }
    restore().await?;
    if SESSION.peek().is_none() {
        return Err(api::ApiError::new(
            StatusCode::UNAUTHORIZED,
            "not signed in",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/session/mod.rs"]
mod tests;
