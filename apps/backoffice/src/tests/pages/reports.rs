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
                kind: ReportKind::Convention,
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
