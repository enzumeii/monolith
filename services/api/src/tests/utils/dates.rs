#![allow(clippy::unwrap_used)]
use super::*;
#[test]
fn offsets_and_timestamp_fallback_are_preserved() {
    assert_eq!(
        parse_rfc3339("2026-10-01T07:00:00+07:00").unwrap(),
        parse_rfc3339("2026-10-01T00:00:00Z").unwrap()
    );
    assert!(parse_rfc3339("bad").is_err());
    assert_eq!(iso_utc(i64::MAX), "1970-01-01T00:00:00+00:00");
    assert_eq!(iso_utc(0), "1970-01-01T00:00:00+00:00");
}
