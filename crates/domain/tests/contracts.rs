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
fn report_rows_preserve_numeric_values_and_nulls() {
    use domain::dto::OrderReportResponse;
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
    let report: OrderReportResponse = serde_json::from_value(value.clone()).expect("decode report");
    assert_eq!(
        serde_json::to_value(report).expect("serialize report"),
        value
    );
}

#[test]
fn complete_report_wire_has_only_products() {
    let value = serde_json::json!({"products":[]});
    let report: domain::dto::OrderReportResponse =
        serde_json::from_value(value.clone()).expect("complete report");
    assert_eq!(
        serde_json::to_value(report).expect("serialize report"),
        value
    );
}

#[test]
fn report_query_contracts_are_independent() {
    use domain::dto::{InventoryQuery, OrderReportQuery};
    let inventory: InventoryQuery =
        serde_json::from_value(serde_json::json!({"page_size":50,"cursor":"abc"}))
            .expect("inventory query");
    assert_eq!(inventory.page_size, Some(50));
    assert_eq!(inventory.cursor.as_deref(), Some("abc"));
    let order: OrderReportQuery =
        serde_json::from_value(serde_json::json!({"tags":"TAG"})).expect("order query");
    assert_eq!(order.tags, "TAG");
    for extra in [
        serde_json::json!({"page_size":1}),
        serde_json::json!({"cursor":"next"}),
    ] {
        let mut query = serde_json::json!({"tags":"TAG"});
        query
            .as_object_mut()
            .expect("query")
            .extend(extra.as_object().expect("extra").clone());
        assert!(serde_json::from_value::<OrderReportQuery>(query).is_err());
    }
    assert!(serde_json::from_value::<InventoryQuery>(serde_json::json!({"tags":"TAG"})).is_err());
}

#[test]
fn inventory_contract_retains_pagination_metadata() {
    let value =
        serde_json::json!({"products":[],"page_info":{"has_next_page":true,"end_cursor":"next"}});
    let report: domain::dto::InventoryResponse =
        serde_json::from_value(value.clone()).expect("decode inventory page");
    assert_eq!(
        serde_json::to_value(report).expect("encode inventory page"),
        value
    );
}
