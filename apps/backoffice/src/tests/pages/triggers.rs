use super::*;
use domain::dto::InventoryEnableResponse;
use futures::{FutureExt, channel::oneshot};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

#[test]
fn reship_result_title_reflects_the_api_success_flag() {
    assert_eq!(
        reship_result_title(&domain::dto::orders::ReshipOrderResponse {
            success: true,
            email_id: "email-1".into(),
        }),
        "Reship email sent"
    );
    assert_eq!(
        reship_result_title(&domain::dto::orders::ReshipOrderResponse {
            success: false,
            email_id: String::new(),
        }),
        "Reship email failed"
    );
}

type ResponseReceiver = oneshot::Receiver<api::ApiResult<InventoryEnableResponse>>;

type Reply = oneshot::Sender<api::ApiResult<InventoryEnableResponse>>;

#[derive(Clone, Default)]
struct Harness {
    action: Rc<Cell<Option<TriggerAction>>>,
    location: Rc<Cell<Option<Signal<String>>>>,
    error: Rc<Cell<Option<Signal<Option<String>>>>>,
    pending: Rc<Cell<Option<Signal<bool>>>>,
    receivers: Rc<RefCell<VecDeque<ResponseReceiver>>>,
    calls: Rc<Cell<usize>>,
}

fn app() -> Element {
    let harness = use_context::<Harness>();
    let location = use_signal(String::new);
    let error = use_signal(|| None);
    let pending = use_signal(|| false);
    harness.location.set(Some(location));
    harness.error.set(Some(error));
    harness.pending.set(Some(pending));
    let receivers = harness.receivers.clone();
    let calls = harness.calls.clone();
    let action = use_trigger(location, error, pending, move |_| {
        calls.set(calls.get() + 1);
        let reply = receivers.borrow_mut().pop_front().expect("queued response");
        async move { reply.await.expect("test response") }
    });
    harness.action.set(Some(action));
    rsx! { div {} }
}

fn pump(dom: &mut VirtualDom) {
    for _ in 0..10 {
        let _ = dom.wait_for_work().now_or_never();
        dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
    }
}

fn viewer(id: i64, role: domain::AdminRole, expiry: u64) -> session::Session {
    session::Session {
        admin_id: id,
        username: "fixture".into(),
        role,
        expires_at: expiry,
    }
}

fn mount() -> (VirtualDom, Harness) {
    let harness = Harness::default();
    let mut dom = VirtualDom::new(app).with_root_context(harness.clone());
    dom.in_scope(ScopeId::ROOT, || {
        *session::SESSION.write() = Some(viewer(1, domain::AdminRole::Admin, 100));
    });
    dom.rebuild_in_place();
    pump(&mut dom);
    (dom, harness)
}

fn submit(dom: &mut VirtualDom, harness: &Harness) -> Reply {
    let (sender, receiver) = oneshot::channel();
    harness.receivers.borrow_mut().push_back(receiver);
    dom.in_scope(ScopeId::ROOT, || {
        harness.location.get().expect("location").set("123".into());
        harness.pending.get().expect("pending").set(true);
        harness.action.get().expect("action").call("123".into());
    });
    pump(dom);
    sender
}

fn success() -> InventoryEnableResponse {
    InventoryEnableResponse {
        total: 2,
        success: 2,
        failed: 0,
        errors: vec![],
    }
}

fn change_owner(dom: &mut VirtualDom, session: session::Session) {
    dom.in_scope(ScopeId::ROOT, || *session::SESSION.write() = Some(session));
    pump(dom);
}

fn assert_cleared(dom: &VirtualDom, harness: &Harness) {
    dom.in_scope(ScopeId::ROOT, || {
        assert!(harness.action.get().expect("action").value().is_none());
        assert!(harness.location.get().expect("location").read().is_empty());
        assert!(harness.error.get().expect("error").read().is_none());
        assert!(!*harness.pending.get().expect("pending").read());
    });
}

#[test]
fn completed_success_and_error_clear_on_admin_or_role_change() {
    for failed in [false, true] {
        for role_change in [false, true] {
            let (mut dom, harness) = mount();
            let sender = submit(&mut dom, &harness);
            let response = if failed {
                Err(api::ApiError::new(
                    reqwest::StatusCode::BAD_GATEWAY,
                    "Upstream request failed",
                ))
            } else {
                Ok(success())
            };
            sender.send(response).expect("receiver");
            pump(&mut dom);
            dom.in_scope(ScopeId::ROOT, || {
                assert!(harness.action.get().expect("action").value().is_some());
                harness
                    .error
                    .get()
                    .expect("error")
                    .set(Some("validation error".into()));
            });
            let owner = if role_change {
                viewer(1, domain::AdminRole::SuperAdmin, 200)
            } else {
                viewer(2, domain::AdminRole::Admin, 200)
            };
            change_owner(&mut dom, owner);
            assert_cleared(&dom, &harness);
        }
    }
}

#[test]
fn pending_request_is_cancelled_and_new_owner_can_submit() {
    let (mut dom, harness) = mount();
    let old = submit(&mut dom, &harness);
    change_owner(&mut dom, viewer(2, domain::AdminRole::Admin, 200));
    assert!(old.is_canceled(), "previous-owner future must be dropped");
    assert!(
        old.send(Ok(success())).is_err(),
        "late response cannot be accepted"
    );
    pump(&mut dom);
    assert_cleared(&dom, &harness);
    let new = submit(&mut dom, &harness);
    new.send(Ok(success())).expect("new owner's receiver");
    pump(&mut dom);
    dom.in_scope(ScopeId::ROOT, || {
        assert!(harness.action.get().expect("action").value().is_some());
    });
    assert_eq!(harness.calls.get(), 2);
}

#[test]
fn same_owner_refresh_preserves_pending_and_completed_work_without_replay() {
    let (mut dom, harness) = mount();
    let sender = submit(&mut dom, &harness);
    change_owner(&mut dom, viewer(1, domain::AdminRole::Admin, 200));
    assert!(!sender.is_canceled());
    dom.in_scope(ScopeId::ROOT, || {
        assert!(*harness.pending.get().expect("pending").read());
    });
    sender.send(Ok(success())).expect("same-owner receiver");
    pump(&mut dom);
    change_owner(&mut dom, viewer(1, domain::AdminRole::Admin, 300));
    dom.in_scope(ScopeId::ROOT, || {
        let result = harness
            .action
            .get()
            .expect("action")
            .value()
            .expect("completed")
            .expect("action result");
        assert_eq!(
            result.read().as_ref().expect("successful response").success,
            2
        );
        assert_eq!(
            harness.location.get().expect("location").read().as_str(),
            "123"
        );
        assert!(!*harness.pending.get().expect("pending").read());
    });
    assert_eq!(harness.calls.get(), 1);
}
