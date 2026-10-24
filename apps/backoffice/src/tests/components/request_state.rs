use super::*;

#[test]
fn input_revision_invalidates_preview_and_duplicate_action_is_rejected() {
    let mut state = RequestState::default();
    let first = state.begin().expect("valid input");
    assert!(state.begin().is_none());
    state.edited();
    assert!(!state.accepts(first));
    assert!(!state.finish(first));
    let latest = state.begin().expect("valid input");
    assert!(state.accepts(latest));
    assert!(state.finish(latest));
    assert!(!state.finish(latest));
}

#[test]
fn authenticated_resource_preserves_pending_work_on_refresh_but_changes_owner() {
    use dioxus::prelude::*;
    use futures::FutureExt;
    use std::{cell::Cell, rc::Rc};
    #[derive(Clone, Default)]
    struct Harness(Rc<Cell<usize>>);
    fn app() -> Element {
        let harness = use_context::<Harness>();
        let owner = use_owner();
        let _pending = use_resource(move || {
            let _owner = owner();
            harness.0.set(harness.0.get() + 1);
            async move {
                crate::session::with_session(std::future::pending::<crate::api::ApiResult<()>>)
                    .await
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
    let harness = Harness::default();
    let mut dom = VirtualDom::new(app).with_root_context(harness.clone());
    let viewer = |id, role, expires_at| crate::session::Session {
        admin_id: id,
        username: "fixture".into(),
        role,
        expires_at,
    };
    dom.in_scope(ScopeId::ROOT, || {
        *crate::session::SESSION.write() =
            Some(viewer(1, domain::AdminRole::Admin, u64::MAX - 100));
    });
    dom.rebuild_in_place();
    pump(&mut dom);
    assert_eq!(harness.0.get(), 1);
    dom.in_scope(ScopeId::ROOT, || {
        *crate::session::SESSION.write() = Some(viewer(1, domain::AdminRole::Admin, u64::MAX - 50));
    });
    pump(&mut dom);
    assert_eq!(harness.0.get(), 1, "same owner keeps pending work");
    dom.in_scope(ScopeId::ROOT, || {
        *crate::session::SESSION.write() = Some(viewer(2, domain::AdminRole::Admin, u64::MAX - 50));
    });
    pump(&mut dom);
    assert_eq!(harness.0.get(), 2);
    dom.in_scope(ScopeId::ROOT, || {
        *crate::session::SESSION.write() =
            Some(viewer(2, domain::AdminRole::SuperAdmin, u64::MAX - 50));
    });
    pump(&mut dom);
    assert_eq!(harness.0.get(), 3);
}
