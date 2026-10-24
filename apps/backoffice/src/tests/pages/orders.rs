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
        conventions: crate::config::ConventionsConfig {
            catalogue_location_id: "65761018027".into(),
        },
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

pub(super) fn best_label() -> domain::dto::orders::BestLabel {
    domain::dto::orders::BestLabel {
        name: "ผู้รับ Best".into(),
        phone: "00123".into(),
        zip_code: "00100".into(),
        address: "Best address".into(),
        category: "Goods".into(),
        cod_amount: "12.50".into(),
        insured_amount: 15,
        waybill_number: "WB001".into(),
        note: "=1+1".into(),
        parcel_id: "00042".into(),
        weight: 321,
    }
}
pub(super) fn teleport_label() -> domain::dto::orders::TeleportLabel {
    domain::dto::orders::TeleportLabel {
        name: "ผู้รับ Teleport".into(),
        email: "buyer@example.test".into(),
        phone: "00456".into(),
        address: "Teleport address".into(),
        state: "Bangkok".into(),
        city: "City".into(),
        postal_code: "00101".into(),
        weight: "0.321".parse().expect("weight"),
        is_multi_piece: "Multi Piece Shippment".into(),
        quantity: 3,
        commodity_value: "12.50".parse().expect("value"),
        package: "Polymailer".into(),
        length: 13,
        width: 15,
        height: 4,
        commodity: "Goods".into(),
        commodity_description: "=SUM(1)".into(),
        commodity_currency: "THB".into(),
        delivery_instructions: "Call first".into(),
        tracking_number: "000123".into(),
        reference: "ref-42".into(),
        destination_geo: "13.75,100.5".into(),
    }
}

#[derive(Default)]
struct LabelRender {
    text: Vec<String>,
    attributes: Vec<(String, String)>,
}
impl LabelRender {
    fn node(&mut self, node: &dioxus::dioxus_core::TemplateNode) {
        use dioxus::dioxus_core::{TemplateAttribute, TemplateNode};
        match node {
            TemplateNode::Element {
                attrs, children, ..
            } => {
                for attr in *attrs {
                    if let TemplateAttribute::Static { name, value, .. } = attr {
                        self.attributes.push(((*name).into(), (*value).into()));
                    }
                }
                for child in *children {
                    self.node(child);
                }
            }
            TemplateNode::Text { text } => self.text.push((*text).into()),
            TemplateNode::Dynamic { .. } => {}
        }
    }
}

use dioxus::dioxus_core::{AttributeValue, ElementId, Template};
impl dioxus::dioxus_core::WriteMutations for LabelRender {
    fn append_children(&mut self, _: ElementId, _: usize) {}

    fn assign_node_id(&mut self, _: &'static [u8], _: ElementId) {}

    fn create_placeholder(&mut self, _: ElementId) {}

    fn create_text_node(&mut self, value: &str, _: ElementId) {
        self.text.push(value.into());
    }

    fn load_template(&mut self, template: Template, index: usize, _: ElementId) {
        self.node(&template.roots[index]);
    }

    fn replace_node_with(&mut self, _: ElementId, _: usize) {}

    fn replace_placeholder_with_nodes(&mut self, _: &'static [u8], _: usize) {}

    fn insert_nodes_after(&mut self, _: ElementId, _: usize) {}

    fn insert_nodes_before(&mut self, _: ElementId, _: usize) {}

    fn set_attribute(
        &mut self,
        name: &'static str,
        _: Option<&'static str>,
        value: &AttributeValue,
        _: ElementId,
    ) {
        if let AttributeValue::Text(value) = value {
            self.attributes.push((name.into(), value.clone()));
        }
    }

    fn set_node_text(&mut self, _: &str, _: ElementId) {}

    fn create_event_listener(&mut self, _: &'static str, _: ElementId) {}

    fn remove_event_listener(&mut self, _: &'static str, _: ElementId) {}

    fn remove_node(&mut self, _: ElementId) {}

    fn push_root(&mut self, _: ElementId) {}
}

fn label_app() -> Element {
    let labels = use_context::<ShippingLabels>();
    rsx! { ShippingLabelTables { labels } }
}
#[test]
fn empty_courier_sections_are_hidden_and_one_empty_message_is_shown() {
    let mut dom = VirtualDom::new(label_app).with_root_context(ShippingLabels::default());
    let mut rendered = LabelRender::default();
    dom.rebuild(&mut rendered);
    for label in [
        "Fastship labels",
        "ThaiPost labels",
        "Best labels",
        "Teleport labels",
    ] {
        assert!(
            !rendered
                .attributes
                .iter()
                .any(|(name, value)| name == "aria-label" && value == label),
            "empty {label} must be hidden"
        );
    }
    assert_eq!(
        rendered
            .text
            .iter()
            .filter(|value| value.as_str() == "No shipping labels found.")
            .count(),
        1
    );
}
#[test]
fn only_nonempty_best_and_teleport_sections_render_tables_and_downloads() {
    for (best, teleport) in [(true, false), (false, true), (true, true)] {
        let labels = ShippingLabels {
            best: if best { vec![best_label()] } else { vec![] },
            teleport: if teleport {
                vec![teleport_label()]
            } else {
                vec![]
            },
            ..ShippingLabels::default()
        };
        let mut dom = VirtualDom::new(label_app).with_root_context(labels);
        let mut rendered = LabelRender::default();
        dom.rebuild(&mut rendered);
        for (label, present) in [
            ("Best labels", best),
            ("Teleport labels", teleport),
            ("Fastship labels", false),
            ("ThaiPost labels", false),
        ] {
            assert_eq!(
                rendered
                    .attributes
                    .iter()
                    .any(|(name, value)| name == "aria-label" && value == label),
                present,
                "section {label}"
            );
        }
        for (text, present) in [("Save Best XLSX", best), ("Save Teleport XLSX", teleport)] {
            assert_eq!(
                rendered.text.iter().any(|value| value == text),
                present,
                "button {text}"
            );
        }
        assert!(
            !rendered
                .text
                .iter()
                .any(|text| text == "No shipping labels found.")
        );
    }
}

#[test]
fn consolidated_download_is_disabled_only_when_every_courier_is_empty() {
    for populated in [false, true] {
        let labels = ShippingLabels {
            best: if populated {
                vec![best_label()]
            } else {
                vec![]
            },
            ..ShippingLabels::default()
        };
        let mut dom = VirtualDom::new(label_app).with_root_context(labels);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        let button = html
            .split("<button")
            .find(|tag| {
                tag.split('>')
                    .next()
                    .is_some_and(|attributes| attributes.contains("shipping-labels-save"))
            })
            .expect("consolidated button");
        let (attributes, content) = button.split_once('>').expect("button tag");
        assert_eq!(attributes.contains("disabled"), !populated);
        assert!(content.starts_with("Download consolidated ZIP</button>"));
    }
}
