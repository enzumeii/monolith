use super::*;
use futures::{FutureExt, channel::oneshot};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

type OrderAction = Action<(OrderView,), (OrderView, ApiResult<OrderData>)>;
type FulfillAction = Action<(FulfillmentRequest,), ApiResult<Vec<FulfillmentResult>>>;
type SignalSlot<T> = Rc<Cell<Option<Signal<T>>>>;
type ResponseQueue<T> = Rc<RefCell<VecDeque<oneshot::Receiver<ApiResult<T>>>>>;

type ReportReply = oneshot::Sender<ApiResult<OrderData>>;
type FulfillReply = oneshot::Sender<ApiResult<Vec<FulfillmentResult>>>;

#[derive(Clone, Default)]
struct Harness {
    request: Rc<Cell<Option<Signal<Option<OrderView>>>>>,
    report: Rc<Cell<Option<OrderAction>>>,
    pending: Rc<Cell<Option<Signal<bool>>>>,
    #[cfg(not(target_arch = "wasm32"))]
    show: Rc<Cell<Option<Signal<bool>>>>,
    fulfill: Rc<Cell<Option<FulfillAction>>>,
    tracking: SignalSlot<HashMap<u64, String>>,
    reports: ResponseQueue<OrderData>,
    fulfillments: ResponseQueue<Vec<FulfillmentResult>>,
    report_calls: Rc<Cell<usize>>,
    fulfillment_calls: Rc<Cell<usize>>,
}
fn viewer(id: i64) -> session::Session {
    session::Session {
        admin_id: id,
        username: format!("admin-{id}"),
        role: domain::AdminRole::Admin,
        expires_at: 4_102_444_800,
    }
}
fn pump(dom: &mut VirtualDom) {
    for _ in 0..10 {
        let _ = dom.wait_for_work().now_or_never();
        dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
    }
}
fn customer_data() -> OrderData {
    OrderData::Fulfillment(vec![FulfillmentOrder {
        order: domain::dto::orders::OrderSummary {
            id: 42,
            number: 1085,
            full_name: Some("Original owner's customer".into()),
            email: Some("buyer@example.test".into()),
            shipping_country_code: None,
        },
        courier: domain::dto::orders::Courier::Fastship,
    }])
}
fn owner_app() -> Element {
    let h = use_context::<Harness>();
    let request = use_signal(|| None);
    let tracking = use_signal(HashMap::new);
    let mut pending = use_signal(|| false);
    let replies = h.fulfillments.clone();
    let calls = h.fulfillment_calls.clone();
    let fulfill = use_action(move |_: FulfillmentRequest| {
        calls.set(calls.get() + 1);
        let reply = replies
            .borrow_mut()
            .pop_front()
            .expect("queued fulfillment");
        async move {
            let result = reply.await.expect("fulfillment reply");
            pending.set(false);
            Ok::<_, ApiError>(result)
        }
    });
    let replies = h.reports.clone();
    let calls = h.report_calls.clone();
    let report = use_order_report(request, tracking, fulfill, pending, move |_| {
        calls.set(calls.get() + 1);
        let reply = replies.borrow_mut().pop_front().expect("queued report");
        async move { reply.await.expect("report reply") }
    });
    h.request.set(Some(request));
    h.report.set(Some(report));
    h.tracking.set(Some(tracking));
    h.pending.set(Some(pending));
    h.fulfill.set(Some(fulfill));
    rsx! {div{}}
}
fn mount() -> (VirtualDom, Harness) {
    let h = Harness::default();
    let mut dom = VirtualDom::new(owner_app).with_root_context(h.clone());
    dom.in_scope(ScopeId::ROOT, || {
        *session::SESSION.write() = Some(viewer(1));
    });
    dom.rebuild_in_place();
    pump(&mut dom);
    (dom, h)
}
fn submit(dom: &mut VirtualDom, h: &Harness) -> (ReportReply, FulfillReply) {
    let (report_tx, report_rx) = oneshot::channel();
    h.reports.borrow_mut().push_back(report_rx);
    let (fulfill_tx, fulfill_rx) = oneshot::channel();
    h.fulfillments.borrow_mut().push_back(fulfill_rx);
    dom.in_scope(ScopeId::ROOT, || {
        h.request
            .get()
            .expect("initialized test fixture")
            .set(Some(OrderView::Fulfillment));
        h.tracking
            .get()
            .expect("initialized test fixture")
            .write()
            .insert(42, "TRACK123".into());
        h.pending.get().expect("initialized test fixture").set(true);
        h.fulfill
            .get()
            .expect("initialized test fixture")
            .call(FulfillmentRequest {
                orders: vec![],
                tracking_numbers: HashMap::new(),
            });
    });
    pump(dom);
    (report_tx, fulfill_tx)
}
fn change_owner(dom: &mut VirtualDom, owner: Option<session::Session>) {
    dom.in_scope(ScopeId::ROOT, || *session::SESSION.write() = owner);
    pump(dom);
}
fn owners() -> [Option<session::Session>; 3] {
    let mut changed_role = viewer(1);
    changed_role.role = domain::AdminRole::SuperAdmin;
    [Some(viewer(2)), Some(changed_role), None]
}
fn assert_cleared(dom: &VirtualDom, h: &Harness) {
    dom.in_scope(ScopeId::ROOT, || {
        assert!(
            h.request
                .get()
                .expect("initialized test fixture")
                .read()
                .is_none()
        );
        assert!(
            h.report
                .get()
                .expect("initialized test fixture")
                .value()
                .is_none()
        );
        assert!(
            h.fulfill
                .get()
                .expect("initialized test fixture")
                .value()
                .is_none()
        );
        assert!(!h.report.get().expect("initialized test fixture").pending());
        assert!(!h.fulfill.get().expect("initialized test fixture").pending());
        assert!(
            h.tracking
                .get()
                .expect("initialized test fixture")
                .read()
                .is_empty()
        );
        assert!(!*h.pending.get().expect("initialized test fixture").read());
    });
}
#[test]
fn completed_customer_data_tracking_and_results_clear_on_principal_change() {
    for owner in owners() {
        let (mut dom, h) = mount();
        let (report, fulfill) = submit(&mut dom, &h);
        assert!(report.send(Ok(customer_data())).is_ok());
        assert!(fulfill.send(Ok(vec![])).is_ok());
        pump(&mut dom);
        dom.in_scope(ScopeId::ROOT, || {
            assert!(
                h.report
                    .get()
                    .expect("initialized test fixture")
                    .value()
                    .is_some()
            );
            assert!(
                h.fulfill
                    .get()
                    .expect("initialized test fixture")
                    .value()
                    .is_some()
            );
        });
        change_owner(&mut dom, owner);
        assert_cleared(&dom, &h);
    }
}
#[test]
fn principal_change_cancels_pending_actions_rejects_late_results_and_allows_new_work() {
    for owner in owners() {
        let (mut dom, h) = mount();
        let (report, fulfill) = submit(&mut dom, &h);
        change_owner(&mut dom, owner);
        assert_cleared(&dom, &h);
        assert!(report.is_canceled());
        assert!(fulfill.is_canceled());
        assert!(report.send(Ok(customer_data())).is_err());
        assert!(fulfill.send(Ok(vec![])).is_err());
        change_owner(&mut dom, Some(viewer(3)));
        let (report, fulfill) = submit(&mut dom, &h);
        assert!(report.send(Ok(customer_data())).is_ok());
        assert!(fulfill.send(Ok(vec![])).is_ok());
        pump(&mut dom);
        dom.in_scope(ScopeId::ROOT, || {
            assert!(
                h.report
                    .get()
                    .expect("initialized test fixture")
                    .value()
                    .is_some()
            );
        });
        assert_eq!(h.report_calls.get(), 2);
        assert_eq!(h.fulfillment_calls.get(), 2);
    }
}
#[test]
fn same_principal_refresh_preserves_pending_and_completed_work_without_replay() {
    let (mut dom, h) = mount();
    let (report, fulfill) = submit(&mut dom, &h);
    let mut refreshed = viewer(1);
    refreshed.expires_at += 100;
    refreshed.username = "renamed".into();
    change_owner(&mut dom, Some(refreshed.clone()));
    assert!(!report.is_canceled());
    assert!(!fulfill.is_canceled());
    dom.in_scope(ScopeId::ROOT, || {
        assert!(*h.pending.get().expect("initialized test fixture").read());
    });
    assert!(report.send(Ok(customer_data())).is_ok());
    assert!(fulfill.send(Ok(vec![])).is_ok());
    pump(&mut dom);
    refreshed.expires_at += 100;
    change_owner(&mut dom, Some(refreshed));
    dom.in_scope(ScopeId::ROOT, || {
        assert!(
            h.report
                .get()
                .expect("initialized test fixture")
                .value()
                .is_some()
        );
        assert!(
            h.fulfill
                .get()
                .expect("initialized test fixture")
                .value()
                .is_some()
        );
        assert!(
            h.request
                .get()
                .expect("initialized test fixture")
                .read()
                .is_some()
        );
        assert_eq!(
            h.tracking
                .get()
                .expect("initialized test fixture")
                .read()
                .get(&42)
                .expect("initialized test fixture"),
            "TRACK123"
        );
    });
    assert_eq!(h.report_calls.get(), 1);
    assert_eq!(h.fulfillment_calls.get(), 1);
}
#[cfg(not(target_arch = "wasm32"))]
fn overlay_app() -> Element {
    let h = use_context::<Harness>();
    let pending = use_signal(|| false);
    let show = use_signal(|| true);
    h.pending.set(Some(pending));
    h.show.set(Some(show));
    rsx! {if show(){PendingOrders{}}}
}
#[cfg(not(target_arch = "wasm32"))]
#[component]
fn PendingOrders() -> Element {
    let h = use_context::<Harness>();
    let action = use_fulfillment_action(h.pending.get().expect("initialized test fixture"));
    h.fulfill.set(Some(action));
    rsx! {div{}}
}
#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn unmounting_pending_fulfillment_must_release_parent_overlay() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("initialized test fixture");
    listener
        .set_nonblocking(true)
        .expect("initialized test fixture");
    let address = listener.local_addr().expect("initialized test fixture");
    let (seen_tx, seen_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        for _ in 0..300 {
            if let Ok((mut stream, _)) = listener.accept() {
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .expect("initialized test fixture");
                let mut bytes = [0; 4096];
                let _ = stream.read(&mut bytes);
                let _ = seen_tx.send(());
                let _ = release_rx.recv_timeout(std::time::Duration::from_secs(5));
                let body = r#"{"success":true,"tracking_url":"https://track.test/manual"}"#;
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });
    let h = Harness::default();
    let settings = crate::config::Settings {
        api: crate::config::ApiConfig {
            base_url: format!("http://{address}/v1"),
        },
    };
    let mut dom = VirtualDom::new(overlay_app)
        .with_root_context(h.clone())
        .with_root_context(settings);
    dom.in_scope(ScopeId::ROOT, || {
        *session::SESSION.write() = Some(viewer(1));
    });
    dom.rebuild_in_place();
    pump(&mut dom);
    dom.in_scope(ScopeId::ROOT, || {
        h.pending.get().expect("initialized test fixture").set(true);
        h.fulfill
            .get()
            .expect("initialized test fixture")
            .call(FulfillmentRequest {
                orders: vec![FulfillmentOrder {
                    order: domain::dto::orders::OrderSummary {
                        id: 42,
                        number: 1085,
                        full_name: None,
                        email: None,
                        shipping_country_code: None,
                    },
                    courier: domain::dto::orders::Courier::Fastship,
                }],
                tracking_numbers: HashMap::from([(42, "TRACK123".into())]),
            });
    });
    let mut started = false;
    for _ in 0..100 {
        pump(&mut dom);
        if seen_rx.try_recv().is_ok() {
            started = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        started,
        "real fulfillment request reached the controlled TCP server"
    );
    dom.in_scope(ScopeId::ROOT, || {
        h.show.get().expect("initialized test fixture").set(false);
    });
    pump(&mut dom);
    let overlay_still_active = dom.in_scope(ScopeId::ROOT, || {
        *h.pending.get().expect("initialized test fixture").read()
    });
    let _ = release_tx.send(());
    server.join().expect("initialized test fixture");
    drop(dom);
    assert!(
        !overlay_still_active,
        "parent loading overlay remains true after the Orders scope cancels its pending action"
    );
}
