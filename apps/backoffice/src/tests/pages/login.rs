use super::*;
use crate::test_events::{
    FormEventConverter, disabled_state, element_id, form_event, listeners, render_changes,
};

#[test]
fn sign_in_requires_both_credentials_and_tracks_cleared_inputs() {
    dioxus::html::set_event_converter(Box::new(FormEventConverter));
    let mut dom = VirtualDom::new(Login);
    let initial = dom.rebuild_to_vec();
    let inputs = [
        element_id(&initial, "login-username"),
        element_id(&initial, "login-password"),
    ];
    let button = disabled_state(&initial)[0].0;
    assert_eq!(disabled_state(&initial), vec![(button, true)]);
    form_event(&dom, inputs[0], "input", "admin");
    assert_eq!(disabled_state(&render_changes(&mut dom)), vec![]);
    form_event(&dom, inputs[1], "input", " ");
    assert_eq!(
        disabled_state(&render_changes(&mut dom)),
        vec![(button, false)],
        "password bytes are not trimmed"
    );
    form_event(&dom, inputs[0], "input", "   ");
    assert_eq!(
        disabled_state(&render_changes(&mut dom)),
        vec![(button, true)]
    );
    form_event(&dom, listeners(&initial, "submit")[0], "submit", "");
    assert!(
        disabled_state(&render_changes(&mut dom)).is_empty(),
        "forced incomplete submit never enters signing-in state"
    );
    form_event(&dom, inputs[0], "input", "admin");
    assert_eq!(
        disabled_state(&render_changes(&mut dom)),
        vec![(button, false)]
    );
    form_event(&dom, inputs[1], "input", "");
    assert_eq!(
        disabled_state(&render_changes(&mut dom)),
        vec![(button, true)]
    );
}
