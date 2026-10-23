use super::*;
use futures::FutureExt;
use std::{cell::Cell, rc::Rc};
#[derive(Clone, Default)]
struct Harness {
    request: Rc<Cell<Option<Signal<Option<ReportRequest>>>>>,
    runs: Rc<Cell<usize>>,
}
fn app() -> Element {
    let harness = use_context::<Harness>();
    let request = use_signal(|| None);
    harness.request.set(Some(request));
    let runs = harness.runs;
    let _report = use_report(request, move |_| {
        let runs = runs.clone();
        async move {
            let _session = session::SESSION.read().clone();
            runs.set(runs.get() + 1);
            Ok(ReportData {
                products: vec![],
                page_info: None,
            })
        }
    });
    rsx! { div {} }
}
fn pump(dom: &mut VirtualDom) {
    for _ in 0..10 {
        let _ = dom.wait_for_work().now_or_never();
        dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
    }
}
fn viewer(id: i64, expiry: u64) -> domain::dto::SessionResponse {
    domain::dto::SessionResponse {
        admin_id: id,
        username: "fixture".into(),
        role: domain::AdminRole::Admin,
        expires_at: expiry,
    }
}
#[test]
fn refreshing_same_viewer_does_not_regenerate_a_submitted_report() {
    let harness = Harness::default();
    let mut dom = VirtualDom::new(app).with_root_context(harness.clone());
    dom.in_scope(ScopeId::ROOT, || {
        *session::SESSION.write() = Some(viewer(1, 100));
    });
    dom.rebuild_in_place();
    pump(&mut dom);
    assert_eq!(harness.runs.get(), 0);
    dom.in_scope(ScopeId::ROOT, || {
        harness
            .request
            .get()
            .expect("request signal")
            .set(Some(ReportRequest {
                kind: ProductReportKind::Convention,
                parameter: "123".into(),
                page_size: None,
                cursor: None,
            }));
    });
    pump(&mut dom);
    assert_eq!(harness.runs.get(), 1);
    dom.in_scope(ScopeId::ROOT, || {
        *session::SESSION.write() = Some(viewer(1, 200));
    });
    pump(&mut dom);
    assert_eq!(
        harness.runs.get(),
        1,
        "same viewer refresh must not fetch reports"
    );
    dom.in_scope(ScopeId::ROOT, || {
        *session::SESSION.write() = Some(viewer(2, 200));
    });
    pump(&mut dom);
    assert_eq!(
        harness
            .request
            .get()
            .expect("request signal")
            .peek()
            .as_ref()
            .map(|r| r.parameter.as_str()),
        None,
        "changed viewer invalidates report"
    );
}

#[derive(Default)]
struct ReportRender {
    text: Vec<String>,
    attributes: Vec<(String, String)>,
}
impl ReportRender {
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

use dioxus::dioxus_core::{AttributeValue, DynamicNode, ElementId, VNode};
impl ReportRender {
    fn read(dom: &VirtualDom) -> Self {
        let mut rendered = Self::default();
        rendered.vnode(dom.base_scope().root_node(), dom);
        rendered
    }

    fn vnode(&mut self, vnode: &VNode, dom: &VirtualDom) {
        for root in vnode.template.roots {
            self.node(root);
        }
        for attribute in vnode.dynamic_attrs.iter().flatten() {
            if let AttributeValue::Text(value) = &attribute.value {
                self.attributes.push((attribute.name.into(), value.clone()));
            }
        }
        for (index, node) in vnode.dynamic_nodes.iter().enumerate() {
            match node {
                DynamicNode::Component(component) => {
                    if let Some(scope) = component.mounted_scope(index, vnode, dom) {
                        self.vnode(scope.root_node(), dom);
                    }
                }
                DynamicNode::Fragment(children) => {
                    for child in children {
                        self.vnode(child, dom);
                    }
                }
                DynamicNode::Text(text) => self.text.push(text.value.clone()),
                DynamicNode::Placeholder(_) => {}
            }
        }
    }
}

use crate::test_events::FormEventConverter;

fn change(dom: &VirtualDom, id: ElementId, value: &str) {
    let data = dioxus::html::PlatformEventData::new(Box::new(value.to_owned()));
    dom.runtime().handle_event(
        "change",
        Event::new(Rc::new(data) as Rc<dyn std::any::Any>, true),
        id,
    );
}

#[test]
fn selector_defaults_to_convention_and_offers_accounting() {
    let mut dom = VirtualDom::new(Reports);
    dom.rebuild_in_place();
    let rendered = ReportRender::read(&dom);
    for option in ["Convention report", "Picklist Report", "Accounting"] {
        assert!(
            rendered.text.iter().any(|text| text == option),
            "missing {option}"
        );
    }
    assert!(
        rendered
            .attributes
            .iter()
            .any(|(name, value)| name == "value" && value == "convention")
    );
    assert!(
        rendered
            .attributes
            .iter()
            .any(|(name, value)| name == "id" && value == "inventory-id")
    );
    assert!(
        !rendered
            .attributes
            .iter()
            .any(|(name, value)| name == "id" && value == "accounting-start")
    );
}

#[test]
fn selecting_accounting_mounts_dates_and_returning_requires_generation() {
    use dioxus::dioxus_core::Mutation;
    dioxus::html::set_event_converter(Box::new(FormEventConverter));
    let mut dom = VirtualDom::new(Reports);
    let initial = dom.rebuild_to_vec();
    let selector = initial
        .edits
        .iter()
        .find_map(|edit| match edit {
            Mutation::NewEventListener { name, id } if name == "change" => Some(*id),
            _ => None,
        })
        .expect("report selector");
    change(&dom, selector, "accounting");
    dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
    let accounting = ReportRender::read(&dom);
    for id in ["accounting-start", "accounting-end"] {
        assert!(
            accounting
                .attributes
                .iter()
                .any(|(name, value)| name == "id" && value == id),
            "missing {id}"
        );
    }
    assert!(
        accounting
            .text
            .iter()
            .any(|text| text.contains("UTC+07:00"))
    );
    for id in ["inventory-id", "report-tags", "report-page-size"] {
        assert!(
            !accounting
                .attributes
                .iter()
                .any(|(name, value)| name == "id" && value == id)
        );
    }
    assert_eq!(
        accounting
            .text
            .iter()
            .filter(|text| text.as_str() == "Reports")
            .count(),
        1
    );
    assert_eq!(
        accounting
            .text
            .iter()
            .filter(|text| text.as_str() == "Accounting")
            .count(),
        1
    );
    change(&dom, selector, "picklist");
    dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
    let picklist = ReportRender::read(&dom);
    assert!(
        picklist
            .attributes
            .iter()
            .any(|(name, value)| name == "id" && value == "report-tags")
    );
    change(&dom, selector, "accounting");
    dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
    let returned = ReportRender::read(&dom);
    assert!(
        returned
            .text
            .iter()
            .any(|text| text.contains("then generate a report"))
    );
}

fn routed_reports() -> Element {
    rsx! {
        dioxus::router::components::HistoryProvider {
            history: |()| Rc::new(dioxus::history::MemoryHistory::with_initial_path("/reports")) as Rc<dyn dioxus::history::History>,
            Router::<crate::route::Route> {}
        }
    }
}

#[test]
fn accounting_has_no_navigation_entry_and_former_route_is_not_found() {
    let mut dom = VirtualDom::new(routed_reports);
    dom.rebuild_in_place();
    let rendered = ReportRender::read(&dom);
    assert!(
        rendered
            .attributes
            .iter()
            .any(|(name, value)| name == "href" && value == "/reports")
    );
    assert!(
        !rendered
            .attributes
            .iter()
            .any(|(name, value)| name == "href" && value == "/accounting")
    );
    assert!(matches!(
        "/accounting".parse::<crate::route::Route>().expect("route"),
        crate::route::Route::NotFound { .. }
    ));
}

#[test]
fn returning_to_convention_restores_inventory_controls() {
    use dioxus::dioxus_core::Mutation;
    dioxus::html::set_event_converter(Box::new(FormEventConverter));
    let mut dom = VirtualDom::new(Reports);
    let initial = dom.rebuild_to_vec();
    let selector = initial
        .edits
        .iter()
        .find_map(|edit| match edit {
            Mutation::NewEventListener { name, id } if name == "change" => Some(*id),
            _ => None,
        })
        .expect("report selector");
    change(&dom, selector, "picklist");
    pump(&mut dom);
    change(&dom, selector, "convention");
    pump(&mut dom);
    let rendered = ReportRender::read(&dom);
    for id in ["inventory-id", "report-page-size"] {
        assert!(
            rendered
                .attributes
                .iter()
                .any(|(name, value)| name == "id" && value == id),
            "missing {id}"
        );
    }
    assert!(
        !rendered
            .attributes
            .iter()
            .any(|(name, value)| name == "id" && value == "report-tags")
    );
}

#[test]
fn invalid_inventory_submission_shows_recoverable_error() {
    use dioxus::dioxus_core::Mutation;
    dioxus::html::set_event_converter(Box::new(FormEventConverter));
    let mut dom = VirtualDom::new(Reports);
    dom.in_scope(ScopeId::ROOT, || {
        *session::SESSION.write() = Some(viewer(1, u64::MAX));
    });
    let initial = dom.rebuild_to_vec();
    let submit = initial
        .edits
        .iter()
        .find_map(|edit| match edit {
            Mutation::NewEventListener { name, id } if name == "submit" => Some(*id),
            _ => None,
        })
        .expect("report form");
    let data = dioxus::html::PlatformEventData::new(Box::new(String::new()));
    dom.runtime().handle_event(
        "submit",
        Event::new(Rc::new(data) as Rc<dyn std::any::Any>, true),
        submit,
    );
    pump(&mut dom);
    let rendered = ReportRender::read(&dom);
    assert!(
        rendered
            .text
            .iter()
            .any(|text| text.contains("Enter a positive numeric inventory ID"))
    );
    assert!(!rendered.text.iter().any(|text| text == "Retry"));
    assert!(
        !rendered
            .text
            .iter()
            .any(|text| text == "No products found.")
    );
}

#[cfg(not(target_arch = "wasm32"))]
mod interaction {
    use super::*;
    use dioxus::dioxus_core::{Mutation, Mutations};
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
    };

    fn mount() -> (VirtualDom, TcpListener, Mutations) {
        dioxus::html::set_event_converter(Box::new(FormEventConverter));
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let mut dom = VirtualDom::new(Reports).with_root_context(crate::config::Settings {
            conventions: crate::config::ConventionsConfig {
                catalogue_location_id: "65761018027".into(),
            },
            api: crate::config::ApiConfig {
                base_url: format!("http://{}/v1", listener.local_addr().expect("address")),
            },
        });
        dom.in_scope(ScopeId::ROOT, || {
            *session::SESSION.write() = Some(viewer(1, u64::MAX));
        });
        let initial = dom.rebuild_to_vec();
        (dom, listener, initial)
    }

    fn listeners(mutations: &Mutations, event: &str) -> Vec<ElementId> {
        mutations
            .edits
            .iter()
            .filter_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if *name == event => Some(*id),
                _ => None,
            })
            .collect()
    }

    fn event(dom: &VirtualDom, id: ElementId, name: &str, value: &str) {
        let data = dioxus::html::PlatformEventData::new(Box::new(value.to_owned()));
        dom.runtime().handle_event(
            name,
            Event::new(Rc::new(data) as Rc<dyn std::any::Any>, true),
            id,
        );
    }

    struct HttpRequest {
        stream: TcpStream,
        headers: String,
    }

    async fn request(dom: &mut VirtualDom, listener: &TcpListener) -> HttpRequest {
        for _ in 0..300 {
            pump(dom);
            if let Ok((mut stream, _)) = listener.accept() {
                stream.set_nonblocking(true).expect("nonblocking stream");
                let mut bytes = Vec::new();
                for _ in 0..300 {
                    let mut chunk = [0; 4096];
                    match stream.read(&mut chunk) {
                        Ok(count) if count > 0 => bytes.extend_from_slice(&chunk[..count]),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                        other => panic!("reading HTTP headers: {other:?}"),
                    }
                    if bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                        stream.set_nonblocking(false).expect("blocking response");
                        return HttpRequest {
                            stream,
                            headers: String::from_utf8(bytes).expect("HTTP headers"),
                        };
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    pump(dom);
                }
                panic!("incomplete HTTP headers");
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("report did not request data");
    }

    fn respond(request: HttpRequest, status: u16, body: &str) {
        let mut stream = request.stream;
        write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("response");
    }

    async fn visible(dom: &mut VirtualDom, expected: &str) {
        for _ in 0..300 {
            pump(dom);
            if ReportRender::read(dom)
                .text
                .iter()
                .any(|text| text == expected)
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("missing {expected}: {:?}", ReportRender::read(dom).text);
    }

    fn absent(dom: &VirtualDom, text: &str) {
        assert!(
            !ReportRender::read(dom)
                .text
                .iter()
                .any(|value| value == text),
            "unexpected {text}"
        );
    }

    fn product(title: &str) -> String {
        format!(
            r#"{{"category":"Books","collection":"Fair","sku":"BOOK-1","title":"{title}","variant":null,"cost":10,"cost_currency":"THB","weight":1,"unit":"kg","selling_price":20,"selling_price_currency":"THB","quantity":2,"total_selling_price":40,"total_weight":2}}"#
        )
    }

    fn inventory(title: &str, next: bool, cursor: &str) -> String {
        format!(
            r#"{{"products":[{}],"page_info":{{"has_next_page":{next},"end_cursor":"{cursor}"}}}}"#,
            product(title)
        )
    }

    fn inventory_page(request: HttpRequest, first: &str, second: &str, cursor: &str) {
        let is_second = request
            .headers
            .lines()
            .next()
            .expect("request line")
            .contains(&format!("cursor={cursor}"));
        let body = inventory(if is_second { second } else { first }, !is_second, cursor);
        respond(request, 200, &body);
    }

    #[tokio::test]
    async fn convention_navigation_replaces_rows_and_all_collects_every_page() {
        let (mut dom, listener, initial) = mount();
        let changes = listeners(&initial, "change");
        let clicks = listeners(&initial, "click");
        let input = listeners(&initial, "input")[0];
        event(&dom, input, "input", "123");
        pump(&mut dom);
        event(&dom, listeners(&initial, "submit")[0], "submit", "");
        let stream = request(&mut dom, &listener).await;
        absent(&dom, "No products found.");
        inventory_page(stream, "First page", "Second page", "second");
        visible(&mut dom, "First page").await;
        visible(&mut dom, "1 rows").await;
        visible(&mut dom, "Save report…").await;
        event(&dom, clicks[1], "click", "");
        let stream = request(&mut dom, &listener).await;
        inventory_page(stream, "First page", "Second page", "second");
        visible(&mut dom, "Second page").await;
        absent(&dom, "First page");
        event(&dom, clicks[0], "click", "");
        let stream = request(&mut dom, &listener).await;
        inventory_page(stream, "First page", "Second page", "second");
        visible(&mut dom, "First page").await;
        event(&dom, changes[1], "change", "all");
        let stream = request(&mut dom, &listener).await;
        inventory_page(stream, "All first", "All second", "more");
        let stream = request(&mut dom, &listener).await;
        inventory_page(stream, "All first", "All second", "more");
        visible(&mut dom, "2 rows").await;
        visible(&mut dom, "All first").await;
        visible(&mut dom, "All second").await;
        absent(&dom, "Previous");
        absent(&dom, "Next");
        event(&dom, input, "input", "456");
        pump(&mut dom);
        absent(&dom, "All first");
        absent(&dom, "Save report…");
    }

    #[tokio::test]
    async fn picklist_displays_products_and_editing_tags_clears_the_previous_report() {
        let (mut dom, listener, initial) = mount();
        change(&dom, listeners(&initial, "change")[0], "picklist");
        let picklist = dom.render_immediate_to_vec();
        let input = listeners(&picklist, "input")[0];
        let submit = listeners(&picklist, "submit")[0];
        event(&dom, input, "input", "fair");
        pump(&mut dom);
        event(&dom, submit, "submit", "");
        let stream = request(&mut dom, &listener).await;
        respond(
            stream,
            200,
            &format!(r#"{{"products":[{}]}}"#, product("Picklist book")),
        );
        visible(&mut dom, "Picklist book").await;
        visible(&mut dom, "1 rows").await;
        visible(&mut dom, "Save report…").await;
        absent(&dom, "Rows per page");
        event(&dom, input, "input", "empty");
        pump(&mut dom);
        absent(&dom, "Picklist book");
        absent(&dom, "Save report…");
        event(&dom, submit, "submit", "");
        let stream = request(&mut dom, &listener).await;
        respond(stream, 200, r#"{"products":[]}"#);
        visible(&mut dom, "No products found.").await;
        visible(&mut dom, "0 rows").await;
    }

    #[tokio::test]
    async fn retry_recovers_a_failed_report_and_input_clears_the_error() {
        let (mut dom, listener, initial) = mount();
        let input = listeners(&initial, "input")[0];
        event(&dom, input, "input", "123");
        pump(&mut dom);
        let submit = listeners(&initial, "submit")[0];
        event(&dom, submit, "submit", "");
        let stream = request(&mut dom, &listener).await;
        respond(
            stream,
            503,
            r#"{"error":"Inventory is temporarily unavailable"}"#,
        );
        // Capture the newly rendered Retry listener while waiting for the response.
        let mut retry = None;
        for _ in 0..300 {
            let _ = dom.wait_for_work().now_or_never();
            let mutations = dom.render_immediate_to_vec();
            retry = retry.or_else(|| listeners(&mutations, "click").first().copied());
            if retry.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        visible(&mut dom, "Inventory is temporarily unavailable").await;
        event(&dom, retry.expect("Retry button"), "click", "");
        let stream = request(&mut dom, &listener).await;
        respond(stream, 200, &inventory("Recovered book", false, "last"));
        visible(&mut dom, "Recovered book").await;
        absent(&dom, "Inventory is temporarily unavailable");
        absent(&dom, "Retry");
        event(&dom, input, "input", "");
        pump(&mut dom);
        event(&dom, submit, "submit", "");
        visible(&mut dom, "Enter a positive numeric inventory ID").await;
        event(&dom, input, "input", "456");
        pump(&mut dom);
        absent(&dom, "Enter a positive numeric inventory ID");
        absent(&dom, "Retry");
    }
}

#[test]
fn generate_tracks_active_required_report_parameter() {
    use crate::test_events::{disabled_state, form_event, listeners};
    dioxus::html::set_event_converter(Box::new(FormEventConverter));
    let mut dom = VirtualDom::new(Reports);
    let initial = dom.rebuild_to_vec();
    let input = listeners(&initial, "input")[0];
    let button = disabled_state(&initial)[0].0;
    assert_eq!(disabled_state(&initial)[0], (button, true));
    for (value, disabled) in [("123", false), ("0", true), (" 123 ", false), ("", true)] {
        form_event(&dom, input, "input", value);
        assert!(disabled_state(&dom.render_immediate_to_vec()).contains(&(button, disabled)));
    }
    form_event(&dom, listeners(&initial, "change")[0], "change", "picklist");
    let switched = dom.render_immediate_to_vec();
    let input = listeners(&switched, "input")[0];
    let button = disabled_state(&switched)[0].0;
    assert_eq!(disabled_state(&switched)[0], (button, true));
    for (value, disabled) in [
        ("fair", false),
        ("fair,", true),
        ("fair,books", false),
        ("   ", true),
    ] {
        form_event(&dom, input, "input", value);
        assert!(disabled_state(&dom.render_immediate_to_vec()).contains(&(button, disabled)));
    }
}
