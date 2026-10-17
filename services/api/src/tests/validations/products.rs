use super::*;
#[test]
fn rejects_blank_and_malformed_location_ids_and_keeps_literal_tags() {
    for id in ["", " ", "abc", "0", "gid://shopify/Order/1", "-1", "12 3"] {
        assert!(
            validate_report_query(ProductReportQuery {
                report_type: ProductReportType::Inventory,
                id: id.into(),
                page_size: None,
                cursor: None
            })
            .is_err()
        );
    }
    assert!(matches!(
        validate_report_query(ProductReportQuery {
            report_type: ProductReportType::Inventory,
            id: "1234".into(),
            page_size: None,
            cursor: None
        }),
        Ok(ValidatedReport {
            source: ReportSource::Inventory(_),
            ..
        })
    ));
    assert!(
        matches!(validate_report_query(ProductReportQuery {report_type:ProductReportType::OrderTag,id:"a:b".into(),page_size:None,cursor:None}),Ok(ValidatedReport {source:ReportSource::OrderTag(tag),..}) if tag=="a:b")
    );
}

#[test]
fn rejects_out_of_range_report_page_size() {
    for page_size in [0, 51] {
        let query = serde_json::from_value(
            serde_json::json!({"type":"INVENTORY","id":"1234","page_size":page_size}),
        )
        .expect("valid query fixture");
        assert!(validate_report_query(query).is_err());
    }
}

#[test]
fn report_pagination_defaults_and_bounds() {
    for (page_size, want) in [(None, 10), (Some(1), 1), (Some(50), 50)] {
        let input = validate_report_query(ProductReportQuery {
            report_type: ProductReportType::Inventory,
            id: "1234".into(),
            page_size,
            cursor: Some("opaque".into()),
        })
        .expect("valid query fixture");
        assert_eq!(input.page.first, want);
        assert_eq!(input.page.after.as_deref(), Some("opaque"));
    }
    let input: ProductReportQuery =
        serde_json::from_value(serde_json::json!({"type":"ORDER_TAG","id":"tag","cursor":" "}))
            .expect("valid query fixture");
    assert!(validate_report_query(input).is_err());
}
