#![allow(clippy::unwrap_used)]
use crate::dto::{UploadFileQuery, UploadedFileDTO};
use serde_json::json;

#[test]
fn upload_requires_one_location_and_rejects_unknown_fields() {
    for value in [
        json!({}),
        json!({"location":null}),
        json!({"location":"conventions/fair","bucket":"private"}),
    ] {
        assert!(serde_json::from_value::<UploadFileQuery>(value).is_err());
    }
    assert_eq!(
        serde_json::from_value::<UploadFileQuery>(json!({"location":"conventions/fair"}))
            .unwrap()
            .location,
        "conventions/fair"
    );
    let value = json!({"location":"conventions/fair/asset.png", "url":"https://storage.test/signed", "expires_at":"2026-10-23T00:05:00Z"});
    let parsed: UploadedFileDTO = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}
