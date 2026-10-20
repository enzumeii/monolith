use super::*;
use futures::{FutureExt, channel::oneshot};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

type Reply = oneshot::Sender<api::ApiResult<AccountingReportResponse>>;
type ResponseQueue =
    Rc<RefCell<VecDeque<oneshot::Receiver<api::ApiResult<AccountingReportResponse>>>>>;
#[derive(Clone, Default)]
struct Harness {
    submitted: Rc<Cell<Option<Signal<Option<ReportRequest>>>>>,
    action: Rc<Cell<Option<AccountingAction>>>,
    replies: ResponseQueue,
    calls: Rc<Cell<usize>>,
}
fn app() -> Element {
    let h = use_context::<Harness>();
    let submitted = use_signal(|| None);
    h.submitted.set(Some(submitted));
    let replies = h.replies.clone();
    let calls = h.calls.clone();
    let action = use_accounting_report(submitted, move |_| {
        calls.set(calls.get() + 1);
        let reply = replies.borrow_mut().pop_front().expect("queued response");
        async move { reply.await.expect("response") }
    });
    h.action.set(Some(action));
    rsx! { div {} }
}
fn pump(dom: &mut VirtualDom) {
    for _ in 0..10 {
        let _ = dom.wait_for_work().now_or_never();
        dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
    }
}
fn viewer(id: i64, role: domain::AdminRole, expires_at: u64) -> session::Session {
    session::Session {
        admin_id: id,
        username: "fixture".into(),
        role,
        expires_at,
    }
}
fn mount() -> (VirtualDom, Harness) {
    let h = Harness::default();
    let mut dom = VirtualDom::new(app).with_root_context(h.clone());
    dom.in_scope(ScopeId::ROOT, || {
        *session::SESSION.write() = Some(viewer(1, domain::AdminRole::Admin, 100));
    });
    dom.rebuild_in_place();
    pump(&mut dom);
    assert_eq!(h.calls.get(), 0, "landing must not generate");
    (dom, h)
}
fn queue(h: &Harness) -> Reply {
    let (sender, receiver) = oneshot::channel();
    h.replies.borrow_mut().push_back(receiver);
    sender
}
fn submit(dom: &mut VirtualDom, h: &Harness) -> Reply {
    let sender = queue(h);
    dom.in_scope(ScopeId::ROOT, || {
        h.submitted.get().expect("submitted").set(Some(
            date_range("2026-10-01T00:00", "2026-10-31T23:59").expect("dates"),
        ));
    });
    pump(dom);
    assert_eq!(h.calls.get(), 1);
    sender
}
fn success() -> AccountingReportResponse {
    AccountingReportResponse {
        transactions: domain::dto::accounting::AccountingTransactions::default(),
        items: vec![],
    }
}
#[test]
fn principal_changes_clear_completed_success_and_error() {
    for response in [
        Ok(success()),
        Err(api::ApiError::new(
            reqwest::StatusCode::BAD_GATEWAY,
            "Provider failed",
        )),
    ] {
        let (mut dom, h) = mount();
        let reply = submit(&mut dom, &h);
        assert!(reply.send(response).is_ok());
        pump(&mut dom);
        assert!(dom.in_scope(ScopeId::ROOT, || {
            h.action.get().expect("action").value().is_some()
        }));
        dom.in_scope(ScopeId::ROOT, || {
            *session::SESSION.write() = Some(viewer(2, domain::AdminRole::Admin, 100));
        });
        pump(&mut dom);
        dom.in_scope(ScopeId::ROOT, || {
            assert!(h.submitted.get().expect("submitted").read().is_none());
            assert!(h.action.get().expect("action").value().is_none());
        });
    }
}
#[test]
fn pending_work_is_cancelled_for_logout_id_or_role_change() {
    for owner in [
        None,
        Some(viewer(2, domain::AdminRole::Admin, 100)),
        Some(viewer(1, domain::AdminRole::SuperAdmin, 100)),
    ] {
        let (mut dom, h) = mount();
        let old = submit(&mut dom, &h);
        dom.in_scope(ScopeId::ROOT, || *session::SESSION.write() = owner);
        pump(&mut dom);
        let _ = old.send(Ok(success()));
        pump(&mut dom);
        dom.in_scope(ScopeId::ROOT, || {
            assert!(h.submitted.get().expect("submitted").read().is_none());
            assert!(!h.action.get().expect("action").pending());
            assert!(h.action.get().expect("action").value().is_none());
        });
    }
}
#[test]
fn clearing_filters_discards_pending_response_and_allows_new_work() {
    let (mut dom, h) = mount();
    let old = submit(&mut dom, &h);
    dom.in_scope(ScopeId::ROOT, || {
        h.submitted.get().expect("submitted").set(None);
    });
    pump(&mut dom);
    let _ = old.send(Ok(success()));
    pump(&mut dom);
    assert!(dom.in_scope(ScopeId::ROOT, || {
        h.action.get().expect("action").value().is_none()
    }));
    let reply = queue(&h);
    dom.in_scope(ScopeId::ROOT, || {
        h.submitted.get().expect("submitted").set(Some(
            date_range("2026-10-02T00:00", "2026-10-03T00:00").expect("dates"),
        ));
    });
    pump(&mut dom);
    assert_eq!(h.calls.get(), 2);
    assert!(reply.send(Ok(success())).is_ok());
    pump(&mut dom);
    assert!(dom.in_scope(ScopeId::ROOT, || {
        h.action.get().expect("action").value().is_some()
    }));
}
#[test]
fn same_principal_refresh_does_not_replay_and_retry_can_repeat_request() {
    let (mut dom, h) = mount();
    let reply = submit(&mut dom, &h);
    dom.in_scope(ScopeId::ROOT, || {
        *session::SESSION.write() = Some(viewer(1, domain::AdminRole::Admin, 200));
    });
    pump(&mut dom);
    assert_eq!(h.calls.get(), 1);
    assert!(
        reply
            .send(Err(api::ApiError::new(
                reqwest::StatusCode::SERVICE_UNAVAILABLE,
                "Unavailable"
            )))
            .is_ok()
    );
    pump(&mut dom);
    let reply = queue(&h);
    dom.in_scope(ScopeId::ROOT, || {
        let request = h
            .submitted
            .get()
            .expect("submitted")
            .read()
            .clone()
            .expect("request");
        h.action.get().expect("action").call(request);
    });
    pump(&mut dom);
    assert_eq!(h.calls.get(), 2);
    assert!(reply.send(Ok(success())).is_ok());
    pump(&mut dom);
    dom.in_scope(ScopeId::ROOT, || {
        *session::SESSION.write() = Some(viewer(1, domain::AdminRole::Admin, 300));
    });
    pump(&mut dom);
    assert_eq!(h.calls.get(), 2);
}
