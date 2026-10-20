use super::*;
use crate::config::{ApiConfig, Settings};
use dioxus::prelude::*;
#[test]
fn report_uses_versioned_base_cookie_transport_and_encoded_offsets() {
    let dom = VirtualDom::new(|| rsx! {}).with_root_context(Settings {
        api: ApiConfig {
            base_url: "https://example.test/v1/".into(),
        },
    });
    let request = dom.in_scope(ScopeId::ROOT, || {
        report_request(&AccountingReportQuery {
            date_from: "2026-10-01T00:00:00+07:00".into(),
            date_to: "2026-10-31T23:59:59+07:00".into(),
        })
        .expect("request")
        .build()
        .expect("built")
    });
    assert_eq!(request.url().path(), "/v1/admin/transactions");
    assert_eq!(request.method(), reqwest::Method::GET);
    assert_eq!(request.headers()["x-session-mode"], "cookie");
    assert!(request.url().as_str().contains("%2B07%3A00"));
    assert_eq!(
        request.url().query_pairs().collect::<Vec<_>>(),
        [
            ("date_from".into(), "2026-10-01T00:00:00+07:00".into()),
            ("date_to".into(), "2026-10-31T23:59:59+07:00".into())
        ]
    );
}
