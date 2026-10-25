use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Default)]
struct Harness {
    active: Rc<RefCell<Option<Signal<usize>>>>,
    draft: Rc<RefCell<Option<Signal<String>>>>,
    mounts: Rc<Cell<usize>>,
}

#[component]
fn Child() -> Element {
    let state = use_context::<LoadingState>();
    let harness = use_context::<Harness>();
    harness.active.replace(Some(state.0));
    let value = use_signal(|| {
        harness.mounts.set(harness.mounts.get() + 1);
        "initial draft".to_string()
    });
    harness.draft.replace(Some(value));
    rsx! { input { value: "{value}" } }
}
fn app() -> Element {
    rsx! { LoadingBoundary { Child {} } }
}
fn mount() -> (VirtualDom, Harness) {
    let harness = Harness::default();
    let mut dom = VirtualDom::new(app).with_root_context(harness.clone());
    dom.rebuild_in_place();
    (dom, harness)
}
fn pump(dom: &mut VirtualDom) {
    for _ in 0..5 {
        let mut work = std::pin::pin!(dom.wait_for_work());
        let _ = std::future::Future::poll(
            work.as_mut(),
            &mut std::task::Context::from_waker(std::task::Waker::noop()),
        );
    }
    dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
}
fn start(dom: &VirtualDom, harness: &Harness) -> LoadingGuard {
    dom.in_scope(ScopeId::ROOT, || {
        LoadingGuard::start(harness.active.borrow().expect("mounted loading owner"))
    })
}
#[test]
fn overlapping_operations_keep_page_inert_until_last_guard_releases_without_remounting() {
    let (mut dom, harness) = mount();
    dom.in_scope(ScopeId::ROOT, || {
        harness
            .draft
            .borrow()
            .expect("draft")
            .set("retained draft".into());
    });
    let first = start(&dom, &harness);
    let second = start(&dom, &harness);
    pump(&mut dom);
    let html = dioxus_ssr::render(&dom);
    assert!(
        html.contains("inert"),
        "count={:?} html={html}",
        harness.active.borrow().expect("owner").peek()
    );
    assert!(html.contains("Working… Please wait."));
    assert!(html.contains("retained draft"));
    drop(first);
    assert_eq!(*harness.active.borrow().expect("owner").peek(), 1);
    drop(second);
    assert_eq!(*harness.active.borrow().expect("owner").peek(), 0);
    pump(&mut dom);
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("retained draft"));
    assert!(!html.contains("page-loading"));
    assert!(!html.contains("inert="));
    assert_eq!(harness.mounts.get(), 1, "loading never remounts the editor");
}
#[test]
fn cancelled_work_releases_guard_and_disposed_owner_is_safe() {
    let (dom, harness) = mount();
    let guard = start(&dom, &harness);
    let mut work = Box::pin(async move {
        let _guard = guard;
        std::future::pending::<()>().await;
    });
    assert!(
        std::future::Future::poll(
            work.as_mut(),
            &mut std::task::Context::from_waker(std::task::Waker::noop())
        )
        .is_pending()
    );
    drop(work);
    assert_eq!(*harness.active.borrow().expect("owner").peek(), 0);
    let guard = start(&dom, &harness);
    drop(dom);
    drop(guard);
}
#[test]
fn isolated_consumers_without_boundary_do_not_require_loading_state() {
    drop(loading_guard());
    let dom = VirtualDom::new(|| rsx! {});
    dom.in_scope(ScopeId::ROOT, || drop(loading_guard()));
}
