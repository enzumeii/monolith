use super::*;

#[test]
fn dirty_departures_preserve_source_until_cancel_or_single_confirmation() {
    let owner = Some((1, domain::AdminRole::Admin));
    let source = Route::ScheduledReports {
        query: "page=2".into(),
    };
    let mut guard = DraftNavigation::default();
    guard.observe(owner, source.clone(), true);
    assert_eq!(
        guard.navigate(owner, Route::Orders {}),
        Some(source.clone())
    );
    assert_eq!(guard.pending, Some(Route::Orders {}));
    guard.cancel();
    assert!(guard.pending.is_none());
    assert_eq!(
        guard.navigate(owner, Route::Orders {}),
        Some(source.clone()),
        "cancel keeps the unsaved editor guarded"
    );
    assert_eq!(guard.confirm(), Some(Route::Orders {}));
    assert!(
        guard.confirm().is_none(),
        "a confirmed departure is consumed once"
    );
    assert!(guard.navigate(owner, Route::Orders {}).is_none());
}

#[test]
fn query_changes_keep_editor_and_guard_the_updated_url_while_clean_and_account_changes_pass() {
    let owner = Some((1, domain::AdminRole::Admin));
    let source = Route::ScheduledReports {
        query: String::new(),
    };
    let next_query = Route::ScheduledReports {
        query: "page=3&enabled=false".into(),
    };
    let mut guard = DraftNavigation::default();
    guard.observe(owner, source.clone(), false);
    assert!(
        guard.navigate(owner, Route::Orders {}).is_none(),
        "clean departure passes"
    );
    guard.observe(owner, source, true);
    assert!(
        guard.navigate(owner, next_query.clone()).is_none(),
        "same feature filters preserve the editor"
    );
    assert_eq!(
        guard.navigate(owner, Route::Reports {}),
        Some(next_query.clone())
    );
    assert!(
        guard.navigate(None, Route::Home {}).is_none(),
        "logout clears stale ownership"
    );
    assert!(guard.pending.is_none());
    guard.observe(owner, next_query, true);
    assert!(
        guard
            .navigate(Some((2, domain::AdminRole::Admin)), Route::Home {})
            .is_none()
    );
    assert!(guard.pending.is_none());
}

#[test]
fn native_destination_keys_are_owner_scoped_and_confirmation_does_not_become_push_navigation() {
    let owner = Some((1, domain::AdminRole::Admin));
    let source = Route::ScheduledReports {
        query: "page=2".into(),
    };
    let mut guard = DraftNavigation::default();
    guard.observe(owner, source.clone(), true);
    guard.native_departure(owner, "existing-history-entry".into(), Route::Orders {});
    assert_eq!(guard.source, Some(source.clone()));
    assert_eq!(guard.native_key.as_deref(), Some("existing-history-entry"));
    guard.cancel();
    assert!(guard.native_key.is_none());
    assert!(guard.dirty);
    guard.native_departure(
        Some((2, domain::AdminRole::Admin)),
        "stale-owner-entry".into(),
        Route::Home {},
    );
    assert!(guard.pending.is_none());
    guard.native_departure(owner, "existing-history-entry".into(), Route::Orders {});
    assert_eq!(guard.confirm(), Some(Route::Orders {}));
    assert!(guard.native_key.is_none());
    assert!(!guard.dirty);
}

#[test]
fn restoration_failure_retains_original_source_and_blocks_even_the_current_native_destination() {
    let owner = Some((1, domain::AdminRole::Admin));
    let source = Route::ScheduledReports {
        query: "page=2".into(),
    };
    let wrong_location = Route::Orders {};
    let mut guard = DraftNavigation::default();
    guard.observe(owner, source.clone(), true);
    guard.restoration_notice(owner, true);
    guard.observe(owner, wrong_location.clone(), true);
    assert_eq!(
        guard.source,
        Some(source.clone()),
        "an error rerender cannot adopt the failed native destination"
    );
    assert_eq!(
        guard.retained_source(owner, &wrong_location),
        Some(source.clone())
    );
    assert_eq!(
        guard.navigate(owner, wrong_location.clone()),
        Some(source.clone()),
        "clicking that destination still cannot discard the draft"
    );
    assert_eq!(
        guard.navigate(
            owner,
            Route::ScheduledReports {
                query: "page=3".into()
            }
        ),
        Some(source.clone()),
        "query changes wait for pointer restoration too"
    );
    assert!(
        guard.confirm().is_none(),
        "restoration recovery precedes another departure"
    );
    guard.restoration_notice(owner, false);
    guard.native_departure(owner, "native-target".into(), wrong_location.clone());
    assert_eq!(guard.retained_source(owner, &wrong_location), Some(source));
    guard.native_confirming = true;
    assert!(
        guard.retained_source(owner, &wrong_location).is_none(),
        "only explicit confirmed native traversal releases route reads"
    );
}

#[test]
fn history_route_reads_keep_editor_mounted_during_source_restoration_rejection() {
    use dioxus::history::MemoryHistory;
    let dom = VirtualDom::new(|| rsx! { div {} });
    dom.in_scope(ScopeId::ROOT, || {
        let owner = Some((1, domain::AdminRole::Admin));
        *session::SESSION.write() = Some(session::Session {
            admin_id: 1,
            username: "fixture".into(),
            role: domain::AdminRole::Admin,
            expires_at: u64::MAX,
        });
        let source = Route::ScheduledReports {
            query: "page=2".into(),
        };
        let inner = Rc::new(MemoryHistory::with_initial_path(source.to_string()));
        inner.push(Route::Orders {}.to_string());
        let history = DraftHistory {
            inner: inner.clone(),
        };
        NAVIGATION.write().observe(owner, source.clone(), true);
        NAVIGATION.write().restoration_notice(owner, true);
        assert_eq!(history.current_route(), source.to_string());
        assert_eq!(
            inner.current_route(),
            Route::Orders {}.to_string(),
            "guard shadows route reads without rewriting the native entry"
        );
        history.push(Route::Orders {}.to_string());
        assert_eq!(NAVIGATION.peek().source, Some(source.clone()));
        inner.go_back();
        assert_eq!(
            inner.current_route(),
            source.to_string(),
            "retry restores the original pointer"
        );
        NAVIGATION.write().restoration_notice(owner, false);
        NAVIGATION
            .write()
            .native_departure(owner, "native-target".into(), Route::Orders {});
        NAVIGATION.write().native_confirming = true;
        inner.go_forward();
        assert_eq!(history.current_route(), Route::Orders {}.to_string());
        *NAVIGATION.write() = DraftNavigation::default();
    });
}

#[test]
fn confirmed_in_app_departure_releases_the_guard_before_history_reads() {
    use dioxus::history::MemoryHistory;
    let dom = VirtualDom::new(|| rsx! { div {} });
    dom.in_scope(ScopeId::ROOT, || {
        let owner = Some((1, domain::AdminRole::Admin));
        *session::SESSION.write() = Some(session::Session {
            admin_id: 1,
            username: "fixture".into(),
            role: domain::AdminRole::Admin,
            expires_at: u64::MAX,
        });
        let source = Route::ScheduledReports {
            query: String::new(),
        };
        let inner = Rc::new(MemoryHistory::with_initial_path(source.to_string()));
        let history = DraftHistory {
            inner: inner.clone(),
        };
        NAVIGATION.write().observe(owner, source, true);
        history.push(Route::Profile {}.to_string());
        confirm_in_app_departure(|target| history.push(target.to_string()));
        assert_eq!(inner.current_route(), Route::Profile {}.to_string());
        assert!(!NAVIGATION.peek().dirty);
    });
}
