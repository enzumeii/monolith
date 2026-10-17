use super::*;
#[test]
fn rejects_blank_and_malformed_location_ids_and_keeps_literal_tags() {
    for id in ["", " ", "abc", "0", "gid://shopify/Order/1", "-1", "12 3"] {
        assert!(
            validate_report_query(ProductReportQuery {
                report_type: ProductReportType::Inventory,
                id: id.into()
            })
            .is_err()
        );
    }
    assert!(matches!(
        validate_report_query(ProductReportQuery {
            report_type: ProductReportType::Inventory,
            id: "1234".into()
        }),
        Ok(ReportSource::Inventory(_))
    ));
    assert!(
        matches!(validate_report_query(ProductReportQuery {report_type:ProductReportType::OrderTag,id:"a:b".into()}),Ok(ReportSource::OrderTag(tag)) if tag=="a:b")
    );
}
