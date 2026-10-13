#[test]
fn old_restoration_cannot_restore_logged_out_or_new_account_state() {
    let mut owner = super::ResponseOwner::default();
    let old = owner.request();
    owner.transition();
    assert!(!owner.owns(old));
    let first = owner.request();
    let latest = owner.request();
    assert!(!owner.owns(first));
    assert!(owner.owns(latest));
    owner.transition();
    assert!(!owner.owns(latest));
}
