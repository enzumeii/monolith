use super::*;
use crate::test_events::{
    FormEventConverter, disabled_state, element_id, form_event, render_changes,
};
use futures::FutureExt;

#[test]
fn profile_requires_username_and_optional_password_confirmation() {
    dioxus::html::set_event_converter(Box::new(FormEventConverter));
    let mut dom = VirtualDom::new(Profile);
    dom.in_scope(ScopeId::ROOT, || {
        *SESSION.write() = Some(session::Session {
            admin_id: 1,
            username: "admin".into(),
            role: domain::AdminRole::Admin,
            expires_at: u64::MAX,
        });
    });
    dom.rebuild_in_place();
    let inputs = [
        element_id(&dom, "profile-username"),
        element_id(&dom, "profile-password"),
        element_id(&dom, "profile-confirm"),
    ];
    for _ in 0..5 {
        let _ = dom.wait_for_work().now_or_never();
        dom.render_immediate_to_vec();
    }
    form_event(&dom, inputs[0], "input", "");
    assert!(
        disabled_state(&render_changes(&mut dom))
            .iter()
            .any(|(_, disabled)| *disabled)
    );
    form_event(&dom, inputs[0], "input", "renamed");
    assert!(
        disabled_state(&render_changes(&mut dom))
            .iter()
            .any(|(_, disabled)| !*disabled),
        "empty optional password permits username-only change"
    );
    form_event(&dom, inputs[1], "input", "new password");
    assert!(
        disabled_state(&render_changes(&mut dom))
            .iter()
            .any(|(_, disabled)| *disabled)
    );
    form_event(&dom, inputs[2], "input", "new password");
    assert!(
        disabled_state(&render_changes(&mut dom))
            .iter()
            .any(|(_, disabled)| !*disabled)
    );
    form_event(&dom, inputs[2], "input", "");
    assert!(
        disabled_state(&render_changes(&mut dom))
            .iter()
            .any(|(_, disabled)| *disabled)
    );
}
