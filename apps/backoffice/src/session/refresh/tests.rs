#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::rc::Rc;

use domain::AdminRole;
use futures::channel::oneshot;
use futures::executor::block_on;
use futures::future::join;

use super::*;

fn reset_state() {
    GENERATION.with(|value| value.set(0));
    REFRESH_IN_FLIGHT.with(|slot| *slot.borrow_mut() = None);
}

fn session(access_token: &str) -> Session {
    Session {
        access_token: access_token.into(),
        refresh_token: "refresh".into(),
        expires_at: 0,
        admin_id: 1,
        username: "admin".into(),
        role: AdminRole::Admin,
    }
}

#[test]
fn expiry_includes_the_expiration_boundary() {
    let mut value = session("access");
    value.expires_at = now_unix() - 1;
    assert!(is_expired(&value));
    value.expires_at = now_unix();
    assert!(is_expired(&value));
    value.expires_at = now_unix() + 10_000;
    assert!(!is_expired(&value));
}

#[test]
fn concurrent_callers_share_one_successful_refresh() {
    reset_state();
    let calls = Rc::new(Cell::new(0));
    let (sender, receiver) = oneshot::channel::<()>();
    let starts = calls.clone();
    let mut first = Box::pin(refresh_once(move |_| {
        starts.set(starts.get() + 1);
        async move {
            receiver.await.unwrap();
            Ok(session("rotated"))
        }
    }));
    assert!(first.as_mut().now_or_never().is_none());
    let starts = calls.clone();
    let mut second = Box::pin(refresh_once(move |_| {
        starts.set(starts.get() + 1);
        async { Ok(session("duplicate")) }
    }));
    assert!(second.as_mut().now_or_never().is_none());
    sender.send(()).unwrap();
    let (first, second) = block_on(join(first, second));
    assert_eq!(first.unwrap().access_token, "rotated");
    assert_eq!(second.unwrap().access_token, "rotated");
    assert_eq!(calls.get(), 1);
}

#[test]
fn concurrent_callers_share_the_refresh_error() {
    reset_state();
    let (sender, receiver) = oneshot::channel::<()>();
    let mut first = Box::pin(refresh_once(move |_| async move {
        receiver.await.unwrap();
        Err(ServerFnError::new("upstream unavailable"))
    }));
    assert!(first.as_mut().now_or_never().is_none());
    let mut second = Box::pin(refresh_once(|_| async { Ok(session("duplicate")) }));
    assert!(second.as_mut().now_or_never().is_none());
    sender.send(()).unwrap();
    let (first, second) = block_on(join(first, second));
    assert!(
        first
            .unwrap_err()
            .to_string()
            .contains("upstream unavailable")
    );
    assert!(
        second
            .unwrap_err()
            .to_string()
            .contains("upstream unavailable")
    );
    assert_eq!(
        block_on(refresh_once(|_| async { Ok(session("retry")) }))
            .unwrap()
            .access_token,
        "retry"
    );
}

#[test]
fn cancelling_the_initiator_does_not_cache_a_completed_refresh() {
    reset_state();
    let (sender, receiver) = oneshot::channel::<()>();
    let mut initiator = Box::pin(refresh_once(move |_| async move {
        receiver.await.unwrap();
        Ok(session("first"))
    }));
    assert!(initiator.as_mut().now_or_never().is_none());
    drop(initiator);
    sender.send(()).unwrap();
    let joined = block_on(refresh_once(|_| async { Ok(session("unexpected")) }));
    assert_eq!(joined.unwrap().access_token, "first");
    let next = block_on(refresh_once(|_| async { Ok(session("next")) }));
    assert_eq!(next.unwrap().access_token, "next");
}

#[test]
fn cancelling_the_initiator_does_not_cache_a_failed_refresh() {
    reset_state();
    let (sender, receiver) = oneshot::channel::<()>();
    let mut initiator = Box::pin(refresh_once(move |_| async move {
        receiver.await.unwrap();
        Err(ServerFnError::new("upstream unavailable"))
    }));
    assert!(initiator.as_mut().now_or_never().is_none());
    drop(initiator);
    sender.send(()).unwrap();
    assert!(block_on(refresh_once(|_| async { Ok(session("unexpected")) })).is_err());
    let retry = block_on(refresh_once(|_| async { Ok(session("retry")) }));
    assert_eq!(retry.unwrap().access_token, "retry");
}

#[test]
fn invalidation_detaches_the_previous_sessions_refresh() {
    reset_state();
    let (_sender, receiver) = oneshot::channel::<()>();
    let mut old = Box::pin(refresh_once(move |_| async move {
        receiver.await.unwrap();
        Ok(session("old"))
    }));
    assert!(old.as_mut().now_or_never().is_none());
    let previous = generation();
    invalidate();
    assert_ne!(generation(), previous);
    let next = refresh_once(|_| async { Ok(session("new")) })
        .now_or_never()
        .expect("a new session must not wait for the old refresh")
        .unwrap();
    assert_eq!(next.access_token, "new");
}

#[test]
fn an_old_completion_does_not_discard_a_newer_refresh() {
    reset_state();
    let (old_sender, old_receiver) = oneshot::channel::<()>();
    let mut old = Box::pin(refresh_once(move |_| async move {
        old_receiver.await.unwrap();
        Ok(session("old"))
    }));
    assert!(old.as_mut().now_or_never().is_none());
    invalidate();
    let (new_sender, new_receiver) = oneshot::channel::<()>();
    let mut new = Box::pin(refresh_once(move |_| async move {
        new_receiver.await.unwrap();
        Ok(session("new"))
    }));
    assert!(new.as_mut().now_or_never().is_none());
    old_sender.send(()).unwrap();
    block_on(old).unwrap();
    let mut joined = Box::pin(refresh_once(|_| async { Ok(session("duplicate")) }));
    assert!(joined.as_mut().now_or_never().is_none());
    new_sender.send(()).unwrap();
    let (new, joined) = block_on(join(new, joined));
    assert_eq!(new.unwrap().access_token, "new");
    assert_eq!(joined.unwrap().access_token, "new");
}
