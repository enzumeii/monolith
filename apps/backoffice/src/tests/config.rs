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
            "/backoffice/assets/application.toml",
            "https://admin.example.test/backoffice/profile"
        )
        .expect("valid page and asset URL")
        .as_str(),
        "https://admin.example.test/backoffice/assets/application.toml"
    );
}

#[test]
fn config_asset_rejects_invalid_page_url() {
    assert!(resolve_config_url("/assets/application.toml", "not a URL").is_err());
}

#[test]
fn deployment_configs_select_public_api_without_changing_local_urls() {
    let local: super::Settings =
        toml::from_str(include_str!("../../application.toml")).expect("local settings");
    let production: super::Settings =
        toml::from_str(include_str!("../../application.prod.toml")).expect("production settings");
    assert_eq!(local.api.base_url, "http://localhost:3000/v1");
    assert_eq!(production.api.base_url, "/api/v1");
}

#[test]
fn relative_api_uses_the_current_generated_origin() {
    assert_eq!(
        super::resolve_api_url(
            "/api/v1",
            "https://preview-123.vercel.app/backoffice/orders"
        )
        .expect("relative API"),
        "https://preview-123.vercel.app/api/v1"
    );
    assert_eq!(
        super::resolve_api_url("http://localhost:3000/v1", "http://localhost:8084/")
            .expect("local API"),
        "http://localhost:3000/v1"
    );
    for invalid in [
        "//evil.test/v1",
        "javascript:alert(1)",
        "https://name:secret@example.test/v1",
        "relative/path",
    ] {
        assert!(super::resolve_api_url(invalid, "https://example.test/").is_err());
    }
}

#[test]
fn convention_location_is_required_and_uses_numeric_location_validation() {
    for contents in [
        include_str!("../../application.toml"),
        include_str!("../../application.prod.toml"),
    ] {
        let mut settings: super::Settings = toml::from_str(contents).expect("settings");
        assert_eq!(settings.conventions.catalogue_location_id, "65761018027");
        assert!(settings.validate().is_ok());
        for invalid in ["", "0", "000", "gid://shopify/Location/1", "1 2", "-1"] {
            settings.conventions.catalogue_location_id = invalid.into();
            assert!(settings.validate().is_err());
        }
    }
    assert!(toml::from_str::<super::Settings>("[api]\nbase_url = \"/api/v1\"").is_err());
}
