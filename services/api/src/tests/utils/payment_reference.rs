#![allow(clippy::unwrap_used)]
use super::tag;

#[test]
fn payment_references_fit_shopify_tags_without_losing_kind_or_id_binding() {
    let boundary = "s".repeat(17);
    assert_eq!(
        tag("stripe:session", &boundary).unwrap(),
        format!("payment:stripe:session:{boundary}")
    );
    let session_id = format!("cs_test_{}", "s".repeat(90));
    let reference = tag("stripe:session", &session_id).unwrap();
    assert_eq!(reference, "payment:XvGzmVJpPMv7bbJ-aojDbsffsYUoKToE");
    assert_eq!(reference.len(), 40);
    assert_eq!(tag("stripe:session", &session_id).unwrap(), reference);
    assert_ne!(tag("stripe:intent", &session_id).unwrap(), reference);
    assert_ne!(
        tag("stripe:session", &format!("{session_id}x")).unwrap(),
        reference
    );
    for id in [
        String::new(),
        "bad id".into(),
        "bad:id".into(),
        "bad,id".into(),
        "bad\nid".into(),
        "s".repeat(234),
    ] {
        assert!(tag("stripe:session", &id).is_err());
    }
}
