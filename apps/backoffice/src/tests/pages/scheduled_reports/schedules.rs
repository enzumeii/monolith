use super::*;

#[test]
fn row_mutations_wait_for_editor_dismissal_and_reject_duplicate_actions() {
    let dom = VirtualDom::new(|| rsx! { div {} });
    dom.in_scope(ScopeId::ROOT, || {
        let mut editor = Signal::new(Some(EditorSelection::Existing(
            "existing-schedule".to_owned(),
        )));
        let recovery = Signal::new(super::super::state::SaveRecovery::default());
        let mut mutating = Signal::new(false);
        assert!(
            !begin_row_mutation(editor, mutating, recovery),
            "editing a record blocks its stale row toggle/delete"
        );
        assert!(
            !mutating(),
            "blocked row actions never acquire pending state"
        );
        assert_eq!(
            editor(),
            Some(EditorSelection::Existing("existing-schedule".to_owned())),
            "blocked actions preserve the editor"
        );
        editor.set(Some(EditorSelection::New));
        assert!(
            !begin_row_mutation(editor, mutating, recovery),
            "creating a record also blocks row mutations"
        );
        editor.set(None);
        assert!(
            begin_row_mutation(editor, mutating, recovery),
            "dismissal permits one row action"
        );
        assert!(mutating());
        assert!(
            !begin_row_mutation(editor, mutating, recovery),
            "pending action blocks duplicate dispatch"
        );
        mutating.set(false);
        assert!(
            begin_row_mutation(editor, mutating, recovery),
            "completion permits the next action"
        );
    });
}

#[test]
fn row_conflict_requires_successful_new_list_response_before_any_mutation_retry() {
    let dom = VirtualDom::new(|| rsx! { div {} });
    dom.in_scope(ScopeId::ROOT, || {
        let editor = Signal::new(None);
        let pending = Signal::new(false);
        let mut recovery = Signal::new(super::super::state::SaveRecovery::default());
        let blocked_refresh = Signal::new(None);
        record_row_failure(
            recovery,
            blocked_refresh,
            3,
            api::ApiError::new(reqwest::StatusCode::CONFLICT, "Changed; refresh"),
        );
        assert!(!begin_row_mutation(editor, pending, recovery));
        recovery.write().edited();
        assert!(
            recovery.read().reload_required,
            "clearing transient text on Refresh must retain recovery"
        );
        let failed = Err(api::ApiError::new(
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            "List failed",
        ));
        complete_list_reload(recovery, blocked_refresh, 4, &failed);
        assert!(
            !begin_row_mutation(editor, pending, recovery),
            "failed reload cannot permit retry"
        );
        let success = Ok(domain::dto::CronConfigurationListResponse {
            items: vec![],
            page: 1,
            per_page: 25,
            has_next: false,
        });
        complete_list_reload(recovery, blocked_refresh, 3, &success);
        assert!(
            !begin_row_mutation(editor, pending, recovery),
            "pre-conflict successful data cannot unlock mutations"
        );
        complete_list_reload(recovery, blocked_refresh, 5, &success);
        assert!(!recovery.read().reload_required);
        assert!(
            begin_row_mutation(editor, pending, recovery),
            "successful fresh reload permits one action"
        );
        assert!(
            !begin_row_mutation(editor, pending, recovery),
            "duplicate dispatch remains blocked"
        );
    });
}
