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

struct FormEventConverter;

struct TestFormValue(String);
impl dioxus::html::HasFileData for TestFormValue {
    fn files(&self) -> Vec<dioxus::html::FileData> {
        vec![]
    }
}
impl dioxus::html::HasFormData for TestFormValue {
    fn value(&self) -> String {
        self.0.clone()
    }
    fn valid(&self) -> bool {
        true
    }
    fn values(&self) -> Vec<(String, dioxus::html::FormValue)> {
        vec![]
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
macro_rules! form_event_converter {
    (enum Event { $(#[convert = $converter:ident] #[events = [$($(#[$attr:meta])* $name:ident => $raw:ident,)*]] $(#[raw = [$($raw_only:ident),* $(,)?]])? $group:ident($data:ident),)* }) => {
        impl dioxus::html::HtmlEventConverter for FormEventConverter {
            $(fn $converter(&self, _event: &dioxus::html::PlatformEventData) -> dioxus::html::$data {
                form_event_converter!(@data $data, _event)
            })*
        }
    };
    (@data FormData, $event:ident) => { dioxus::html::FormData::new(TestFormValue($event.downcast::<String>().expect("form value").clone())) };
    (@data $other:ident, $event:ident) => { panic!("unexpected non-form event: {}", stringify!($other)) };
}
dioxus::html::with_html_event_groups!(form_event_converter);

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
