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
