#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn new_applies_database_url_and_jwt_secret_overrides() {
    let settings = temp_env::with_vars(
        [
            (
                "DATABASE_URL",
                Some("postgres://override:override@localhost/override"),
            ),
            (
                "JWT_SECRET",
                Some("abababababababababababababababababababababababababababababababab"),
            ),
        ],
        Settings::new,
    )
    .unwrap();

    assert_eq!(
        settings.database.url,
        "postgres://override:override@localhost/override"
    );
    assert_eq!(
        settings.jwt.secret,
        "abababababababababababababababababababababababababababababababab"
    );
}

#[test]
fn new_reads_non_secret_fields_from_file_when_only_secrets_come_from_env() {
    // Secrets (database.url, jwt.secret) are env-only by design and never
    // live in application.toml. Supply them via env, then assert the
    // non-secret fields still come from the file.
    let settings = temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://env:env@localhost/env")),
            (
                "JWT_SECRET",
                Some("abababababababababababababababababababababababababababababababab"),
            ),
        ],
        Settings::new,
    )
    .unwrap();

    assert_eq!(settings.server.port, 3000);
    assert_eq!(settings.database.max_connections, 1);
    assert_eq!(settings.database.url, "postgres://env:env@localhost/env");
    assert_eq!(
        settings.jwt.secret,
        "abababababababababababababababababababababababababababababababab"
    );
}

#[test]
fn new_reads_non_secret_configuration_only_from_toml() {
    let settings = temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://env:env@localhost/env")),
            (
                "JWT_SECRET",
                Some("abababababababababababababababababababababababababababababababab"),
            ),
            ("API_HOST", Some("ignored.example.test")),
            ("API_PORT", Some("invalid")),
            ("PUBLIC_ORIGIN", Some("https://ignored.example.test")),
            ("INSECURE_LOCAL_COOKIES", Some("invalid")),
            ("SHOPIFY_TIMEOUT_SECS", Some("999")),
        ],
        Settings::new,
    )
    .expect("non-secret environment values must not affect settings");
    assert_eq!(settings.server.host, "0.0.0.0");
    assert_eq!(settings.server.port, 3000);
    assert_eq!(settings.browser.public_origin, "http://localhost:8084");
    assert!(settings.browser.insecure_local);
    assert_eq!(settings.client.unwrap().shopify.unwrap().timeout_secs, 3);
}

mod invalid_config_tests {
    use super::*;
    #[test]
    fn database_debug_hides_credentials() {
        let config = DatabaseConfig {
            url: "postgres://private:secret@localhost/db".into(),
            min_connections: 0,
            max_connections: 1,
        };
        assert!(!format!("{config:?}").contains("secret"));
    }
    #[test]
    fn startup_rejects_placeholder_and_undersized_signing_keys() {
        for key in ["", "CHANGE_ME", "not-hex", &"ab".repeat(31)] {
            let result = temp_env::with_vars(
                [
                    ("DATABASE_URL", Some("postgres://unused")),
                    ("JWT_SECRET", Some(key)),
                ],
                Settings::new,
            );
            assert!(result.is_err(), "unsafe signing key was accepted");
        }
    }
}

#[test]
fn shopify_token_is_loaded_by_configuration_and_never_debugged() {
    let settings = temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://unused")),
            (
                "JWT_SECRET",
                Some("abababababababababababababababababababababababababababababababab"),
            ),
            ("SHOPIFY_ACCESS_TOKEN", Some("private-token")),
        ],
        Settings::new,
    )
    .unwrap();
    let shopify = settings.client.unwrap().shopify.unwrap();
    assert_eq!(shopify.access_token.as_deref(), Some("private-token"));
    assert!(!format!("{shopify:?}").contains("private-token"));
}

#[test]
fn shopify_file_token_follows_normal_deserialization() {
    let config: ShopifyConfig = config::Config::builder()
        .add_source(config::File::from_str(
            "timeout_secs = 3\nadmin_api_url = 'https://shop.myshopify.com/admin/api/2026-10/graphql.json'\naccess_token = 'ignored-token'",
            config::FileFormat::Toml,
        )).build().unwrap().try_deserialize().unwrap();
    assert_eq!(config.access_token.as_deref(), Some("ignored-token"));
}

#[test]
fn local_overlay_preserves_shopify_url_configured_in_application_toml() {
    let configured = include_str!("../../application.toml")
        .replace("enzumeii.myshopify.com", "configured-shop.myshopify.com");
    let settings: Settings = config::Config::builder()
        .add_source(config::File::from_str(
            &configured,
            config::FileFormat::Toml,
        ))
        .add_source(config::File::from_str(
            include_str!("../../application.local.toml"),
            config::FileFormat::Toml,
        ))
        .set_override("database.url", "postgres://unused")
        .unwrap()
        .set_override(
            "jwt.secret",
            "abababababababababababababababababababababababababababababababab",
        )
        .unwrap()
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    assert_eq!(
        settings.client.unwrap().shopify.unwrap().admin_api_url,
        "https://configured-shop.myshopify.com/admin/api/2026-10/graphql.json"
    );
}

#[test]
fn builder_token_precedence_and_missing_url_follow_normal_configuration() {
    for (configured, token) in [
        (false, None),
        (false, Some("env-token")),
        (true, None),
        (true, Some("env-token")),
    ] {
        let source = r#"
            server = { host = "localhost", port = 3000 }
            database = { max_connections = 1, min_connections = 1 }
            browser = { public_origin = "http://localhost:8084", insecure_local = true }
        "#;
        let builder = config::Config::builder()
            .add_source(config::File::from_str(source, config::FileFormat::Toml))
            .set_override("database.url", "postgres://unused")
            .unwrap()
            .set_override("jwt.secret", "ab".repeat(32))
            .unwrap();
        let builder = if configured {
            builder
                .set_override("client.shopify.timeout_secs", 3)
                .unwrap()
                .set_override("client.shopify.admin_api_url", "http://localhost:1/graphql")
                .unwrap()
                .set_override("client.shopify.access_token", "file-token")
                .unwrap()
        } else {
            builder
        };
        let builder = if let Some(token) = token {
            builder
                .set_override("client.shopify.access_token", token)
                .unwrap()
        } else {
            builder
        };
        let result = builder.build().unwrap().try_deserialize::<Settings>();
        if !configured && token.is_some() {
            assert!(result.is_err());
            continue;
        }
        let settings = result.unwrap();
        settings.validate().unwrap();
        if configured {
            assert_eq!(
                settings
                    .client
                    .unwrap()
                    .shopify
                    .unwrap()
                    .access_token
                    .as_deref(),
                token.or(Some("file-token"))
            );
        } else {
            assert!(settings.client.is_none());
        }
    }
}

#[test]
fn builder_override_populates_shopify_token_without_a_construction_helper() {
    let config: ShopifyConfig = config::Config::builder()
        .add_source(config::File::from_str(
            "timeout_secs = 3\nadmin_api_url = 'https://shop.myshopify.com/graphql'\naccess_token = 'file-token'",
            config::FileFormat::Toml,
        ))
        .set_override("access_token", "env-token")
        .unwrap()
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    assert_eq!(config.access_token.as_deref(), Some("env-token"));
}

#[test]
fn resend_api_key_comes_from_central_configuration() {
    let settings = temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://unused")),
            (
                "JWT_SECRET",
                Some("abababababababababababababababababababababababababababababababab"),
            ),
            ("RESEND_API_KEY", Some("re_private-fixture")),
        ],
        Settings::new,
    )
    .unwrap();
    let resend = settings.client.unwrap().resend.unwrap();
    assert_eq!(resend.api_key.as_deref(), Some("re_private-fixture"));
    assert!(!format!("{resend:?}").contains("re_private-fixture"));
}

#[test]
fn resend_settings_follow_file_and_secret_override_precedence() {
    let settings: ResendConfig = config::Config::builder()
        .add_source(config::File::from_str(
            "api_key = 'file-key'",
            config::FileFormat::Toml,
        ))
        .set_override("api_key", "env-key")
        .unwrap()
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    assert_eq!(settings.api_key.as_deref(), Some("env-key"));
    let absent: ResendConfig = config::Config::builder()
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    assert!(absent.api_key.is_none());
}

#[test]
fn resend_configuration_needs_only_the_api_key() {
    let config: ResendConfig = config::Config::builder()
        .add_source(config::File::from_str(
            "api_key = 'fixture-key'",
            config::FileFormat::Toml,
        ))
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    assert_eq!(config.api_key.as_deref(), Some("fixture-key"));
    assert!(!format!("{config:?}").contains("fixture-key"));
}

#[test]
fn paypal_credentials_are_centralized_and_non_secrets_remain_file_based() {
    let settings = temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://unused")),
            (
                "JWT_SECRET",
                Some("abababababababababababababababababababababababababababababababab"),
            ),
            ("PAYPAL_CLIENT_ID", Some("private-paypal-id")),
            ("PAYPAL_CLIENT_SECRET", Some("private-paypal-secret")),
            ("PAYPAL_BASE_URL", Some("invalid-env-url")),
            ("PAYPAL_TIMEOUT_SECS", Some("invalid-env-timeout")),
        ],
        Settings::new,
    )
    .unwrap();
    let paypal = settings.client.unwrap().paypal.unwrap();
    assert_eq!(paypal.client_id.as_deref(), Some("private-paypal-id"));
    assert_eq!(
        paypal.client_secret.as_deref(),
        Some("private-paypal-secret")
    );
    assert_eq!(paypal.base_url, "https://api-m.sandbox.paypal.com/");
    assert_eq!(paypal.timeout_secs, 10);
    assert!(!format!("{paypal:?}").contains("private-paypal"));
}
