use super::*;

#[test]
fn filters_round_trip_and_reset_page_when_filter_changes() {
    let filters = Filters::parse("report_type=INVENTORY&enabled=false&page=3&per_page=50");
    assert_eq!(filters.page, 3);
    assert_eq!(filters.enabled, Some(false));
    assert_eq!(Filters::parse(&filters.query()), filters);
    assert_eq!(
        Filters::parse("page=0&enabled=garbage&per_page=999"),
        Filters::default()
    );
    let changed = filters.with_type("TRANSACTION");
    assert_eq!(changed.page, 1);
    assert_eq!(changed.enabled, Some(false));
    assert_eq!(changed.per_page, 50);
}

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
fn ownership_refresh_preserves_pending_work_but_admin_and_role_changes_restart_it() {
    use dioxus::prelude::*;
    use futures::FutureExt;
    use std::{cell::Cell, rc::Rc};
    #[derive(Clone, Default)]
    struct Harness(Rc<Cell<usize>>);
    fn app() -> Element {
        let harness = use_context::<Harness>();
        let owner = use_schedule_owner();
        let _pending = use_resource(move || {
            let _owner = owner();
            harness.0.set(harness.0.get() + 1);
            std::future::pending::<()>()
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
        *crate::session::SESSION.write() = Some(viewer(1, domain::AdminRole::Admin, 100));
    });
    dom.rebuild_in_place();
    pump(&mut dom);
    assert_eq!(harness.0.get(), 1);
    dom.in_scope(ScopeId::ROOT, || {
        *crate::session::SESSION.write() = Some(viewer(1, domain::AdminRole::Admin, 200));
    });
    pump(&mut dom);
    assert_eq!(harness.0.get(), 1, "same owner keeps pending work");
    dom.in_scope(ScopeId::ROOT, || {
        *crate::session::SESSION.write() = Some(viewer(2, domain::AdminRole::Admin, 200));
    });
    pump(&mut dom);
    assert_eq!(harness.0.get(), 2);
    dom.in_scope(ScopeId::ROOT, || {
        *crate::session::SESSION.write() = Some(viewer(2, domain::AdminRole::SuperAdmin, 200));
    });
    pump(&mut dom);
    assert_eq!(harness.0.get(), 3);
}

#[test]
fn conflict_requires_successful_reload_even_after_edits_or_failed_retries() {
    let mut recovery = SaveRecovery::default();
    recovery.save_failed(crate::api::ApiError::new(
        reqwest::StatusCode::CONFLICT,
        "Changed; reload",
    ));
    assert!(recovery.reload_required);
    recovery.edited();
    assert!(recovery.failure.is_none());
    assert!(
        recovery.reload_required,
        "editing cannot bypass the conflict recovery"
    );
    recovery.transient_failed(crate::api::ApiError::new(
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        "Reload failed",
    ));
    assert!(
        recovery.reload_required,
        "failed reload does not permit saving"
    );
    recovery.loaded();
    assert!(!recovery.reload_required);
    assert!(recovery.failure.is_none());
}
