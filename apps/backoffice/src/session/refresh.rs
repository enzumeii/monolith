use std::cell::{Cell, RefCell};
use std::future::Future;

use dioxus::prelude::*;
use domain::dto::RefreshRequest;
use futures::future::{FutureExt, LocalBoxFuture, Shared};

use super::Session;
use crate::api;

type SharedRefresh = Shared<LocalBoxFuture<'static, ServerFnResult<Session>>>;

thread_local! {
    static GENERATION: Cell<u64> = const { Cell::new(0) };
    static REFRESH_IN_FLIGHT: RefCell<Option<SharedRefresh>> = const { RefCell::new(None) };
}

fn generation() -> u64 {
    GENERATION.with(Cell::get)
}

pub(super) fn invalidate() {
    GENERATION.with(|value| value.set(value.get() + 1));
    REFRESH_IN_FLIGHT.with(|slot| *slot.borrow_mut() = None);
}

fn now_unix() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        })
}

pub(super) fn expires_at(lifetime: i64) -> i64 {
    now_unix() + lifetime
}

fn is_expired(session: &Session) -> bool {
    now_unix() >= session.expires_at
}

pub async fn ensure_valid() -> ServerFnResult<Session> {
    let session = super::current().ok_or_else(|| ServerFnError::new("not signed in"))?;
    if !is_expired(&session) {
        return Ok(session);
    }
    refresh_once(move |version| rotate(session, version)).await
}

async fn rotate(mut session: Session, version: u64) -> ServerFnResult<Session> {
    let tokens = api::refresh(RefreshRequest {
        refresh_token: session.refresh_token.clone(),
    })
    .await?;
    if generation() != version {
        return Err(ServerFnError::new("session ended during refresh"));
    }
    session.access_token = tokens.access_token;
    session.refresh_token = tokens.refresh_token;
    session.expires_at = expires_at(tokens.expires_in);
    super::save(session.clone());
    Ok(session)
}

async fn refresh_once<F, Fut>(start: F) -> ServerFnResult<Session>
where
    F: FnOnce(u64) -> Fut + 'static,
    Fut: Future<Output = ServerFnResult<Session>> + 'static,
{
    if let Some(future) = REFRESH_IN_FLIGHT.with(|slot| slot.borrow().clone()) {
        return future.await;
    }
    let version = generation();
    let future = async move {
        let result = start(version).await;
        // Any waiter can finish the operation, even if its initiator was cancelled.
        if generation() == version {
            REFRESH_IN_FLIGHT.with(|slot| *slot.borrow_mut() = None);
        }
        result
    }
    .boxed_local()
    .shared();
    REFRESH_IN_FLIGHT.with(|slot| *slot.borrow_mut() = Some(future.clone()));
    future.await
}

#[cfg(test)]
mod tests;
