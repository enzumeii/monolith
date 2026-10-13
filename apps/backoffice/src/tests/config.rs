use super::resolve_config_url;

#[test]
fn config_asset_resolves_from_nested_routes() {
    assert_eq!(
        resolve_config_url(
            "/assets/application.toml",
            "https://admin.example.test/admins?page=2"
        )
        .expect("valid page and asset URL")
        .as_str(),
        "https://admin.example.test/assets/application.toml"
    );
}

#[test]
fn config_asset_preserves_hosted_base_path() {
    assert_eq!(
        resolve_config_url(
            "/office/assets/application.toml",
            "https://admin.example.test/office/profile"
        )
        .expect("valid page and asset URL")
        .as_str(),
        "https://admin.example.test/office/assets/application.toml"
    );
}

#[test]
fn config_asset_rejects_invalid_page_url() {
    assert!(resolve_config_url("/assets/application.toml", "not a URL").is_err());
}
