use domain::dto::{AdminDTO, AdminRole};

#[test]
fn admin_contract_uses_stable_roles_and_excludes_credentials() {
    let admin = AdminDTO {
        id: 7,
        username: "operator".into(),
        role: AdminRole::SuperAdmin,
    };
    let value = serde_json::to_value(admin).expect("serialize DTO");
    assert_eq!(
        value,
        serde_json::json!({"id": 7, "username": "operator", "role": "SUPER_ADMIN"})
    );
    assert_eq!(
        serde_json::to_value(AdminRole::Admin).expect("serialize role"),
        "ADMIN"
    );
    assert_eq!(
        serde_json::from_str::<AdminRole>("\"SUPER_ADMIN\"").expect("decode role"),
        AdminRole::SuperAdmin
    );
}

#[test]
fn products_contract_preserves_numeric_values_nulls_and_query_enum() {
    use domain::dto::{ProductReportQuery, ProductsResponse};
    let query: ProductReportQuery =
        serde_json::from_value(serde_json::json!({"type":"INVENTORY","id":"1234"}))
            .expect("decode report query");
    assert_eq!(query.id, "1234");
    assert!(
        serde_json::from_value::<ProductReportQuery>(serde_json::json!({"type":"OTHER","id":"1"}))
            .is_err()
    );
    let value = serde_json::json!({
        "products": [{
            "category":"KEYCHAIN",
            "collection":"STSWOW",
            "sku":"GENSTSWOW01",
            "title":"Item",
            "variant":null,
            "cost":null,
            "cost_currency":null,
            "weight":2.5,
            "unit":"GRAMS",
            "selling_price":0.1,
            "selling_price_currency":"USD",
            "quantity":3,
            "total_selling_price":0.3,
            "total_weight":7.5
        }]
    });
    let report: ProductsResponse = serde_json::from_value(value.clone()).expect("decode report");
    assert_eq!(
        serde_json::to_value(report).expect("serialize report"),
        value
    );
}

#[test]
fn complete_report_wire_has_only_products() {
    let value = serde_json::json!({"products":[]});
    let report: domain::dto::ProductsResponse =
        serde_json::from_value(value.clone()).expect("complete report");
    assert_eq!(
        serde_json::to_value(report).expect("serialize report"),
        value
    );
}

#[test]
fn report_query_rejects_removed_pagination_fields() {
    for extra in [
        serde_json::json!({"page_size":1}),
        serde_json::json!({"cursor":"next"}),
    ] {
        let mut query = serde_json::json!({"type":"ORDER_TAG","id":"report"});
        query
            .as_object_mut()
            .expect("query object")
            .extend(extra.as_object().expect("extra object").clone());
        assert!(serde_json::from_value::<domain::dto::ProductReportQuery>(query).is_err());
    }
}
