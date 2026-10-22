#![allow(clippy::unwrap_used)]
use super::*;
use config::{Config, File, FileFormat};

#[test]
fn local_vercel_development_preserves_selected_scheduler_and_uses_gateway_origin() {
    for implementation in ["vps", "vercel"] {
        let settings = temp_env::with_vars(
            [
                ("VERCEL_ENV", Some("development")),
                ("VERCEL_URL", Some("localhost:18027")),
                (
                    "DATABASE_URL",
                    Some("postgres://synthetic:synthetic@localhost/runtime"),
                ),
                (
                    "JWT_SECRET",
                    Some("abababababababababababababababababababababababababababababababab"),
                ),
            ],
            || {
                Settings::load(
                    Config::builder()
                        .add_source(File::with_name("application"))
                        .set_override("scheduler.implementation", implementation)
                        .unwrap(),
                    "local",
                )
            },
        )
        .unwrap();
        assert_eq!(settings.browser.public_origin, "http://localhost:18027");
        assert!(settings.browser.insecure_local);
        assert_eq!(
            settings.scheduler.implementation,
            if implementation == "vps" {
                CronImplementation::Vps
            } else {
                CronImplementation::Vercel
            }
        );
    }
}

#[test]
fn vercel_development_metadata_uses_only_explicit_loopback_http() {
    for host in ["localhost:18037", "127.0.0.1:18037", "[::1]:18037"] {
        let settings = temp_env::with_vars(
            [
                ("RUN_ENV", Some("prod")),
                ("VERCEL_ENV", Some("development")),
                ("VERCEL_URL", Some(host)),
                (
                    "JWT_SECRET",
                    Some("abababababababababababababababababababababababababababababababab"),
                ),
                (
                    "DATABASE_URL",
                    Some("postgres://synthetic:secret@localhost/runtime"),
                ),
            ],
            Settings::new,
        )
        .unwrap();
        assert_eq!(settings.browser.public_origin, format!("http://{host}"));
        assert!(settings.browser.insecure_local);
    }
    for host in [
        "evil.example:18037",
        "localhost",
        "localhost:0",
        "localhost:65536",
        "localhost:18037/path",
        "user@localhost:18037",
        "https://localhost:18037",
        "127.0.0.2:18037",
        "localhost:18037?x=1",
        "localhost:18037#x",
    ] {
        assert!(
            temp_env::with_vars(
                [
                    ("RUN_ENV", Some("prod")),
                    ("VERCEL_ENV", Some("development")),
                    ("VERCEL_URL", Some(host))
                ],
                Settings::new
            )
            .is_err(),
            "{host}"
        );
    }
    for mode in ["production", "preview"] {
        assert!(
            temp_env::with_vars(
                [
                    ("RUN_ENV", Some("prod")),
                    ("VERCEL_ENV", Some(mode)),
                    ("VERCEL_URL", Some("localhost:18037")),
                    ("VERCEL_PROJECT_PRODUCTION_URL", None)
                ],
                Settings::new
            )
            .is_err()
        );
    }
}

#[test]
fn production_overlay_and_metadata_use_central_secret_overrides() {
    let settings = temp_env::with_vars(
        [
            ("RUN_ENV", Some("prod")),
            ("VERCEL_URL", Some("synthetic-deployment.vercel.app")),
            ("VERCEL_ENV", Some("preview")),
            ("CRON_SECRET", Some("synthetic-cron-secret")),
            (
                "DATABASE_URL",
                Some("postgres://synthetic:secret@localhost/runtime"),
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
        settings.browser.public_origin,
        "https://synthetic-deployment.vercel.app"
    );
    assert!(!format!("{settings:?}").contains("synthetic-cron-secret"));
    assert!(!settings.browser.insecure_local);
    assert_eq!(settings.database.max_connections, 2);
    assert_eq!(settings.database.min_connections, 0);
    assert_eq!(
        settings.database.url,
        "postgres://synthetic:secret@localhost/runtime"
    );
    assert_eq!(
        settings.client.unwrap().vercel_blob.unwrap().api_url,
        "https://vercel.com/api/blob"
    );
}

#[test]
fn vercel_public_origin_uses_generated_production_alias_and_preview_host() {
    for (mode, expected) in [
        ("production", "https://project.vercel.app"),
        ("preview", "https://unique-preview.vercel.app"),
    ] {
        let settings = temp_env::with_vars(
            [
                ("RUN_ENV", Some("prod")),
                ("VERCEL_ENV", Some(mode)),
                ("VERCEL_URL", Some("unique-preview.vercel.app")),
                ("VERCEL_PROJECT_PRODUCTION_URL", Some("project.vercel.app")),
                (
                    "JWT_SECRET",
                    Some("abababababababababababababababababababababababababababababababab"),
                ),
                (
                    "DATABASE_URL",
                    Some("postgres://synthetic:secret@localhost/runtime"),
                ),
            ],
            Settings::new,
        )
        .unwrap();
        assert_eq!(settings.browser.public_origin, expected);
    }
}

#[test]
fn vercel_metadata_rejects_untrusted_origins_and_missing_host() {
    for host in [
        None,
        Some("evil.example"),
        Some("https://project.vercel.app"),
        Some("project.vercel.app/path"),
        Some("user@project.vercel.app"),
        Some("project.vercel.app:443"),
    ] {
        assert!(
            temp_env::with_vars(
                [
                    ("RUN_ENV", Some("prod")),
                    ("VERCEL_ENV", Some("preview")),
                    ("VERCEL_URL", host)
                ],
                Settings::new
            )
            .is_err()
        );
    }
}

#[test]
fn scheduler_requires_explicit_settings() {
    assert!(
        serde_json::from_value::<SchedulerConfig>(serde_json::json!({"enabled":false})).is_err()
    );
    let scheduler: SchedulerConfig = Config::builder()
        .add_source(File::with_name("application"))
        .build()
        .unwrap()
        .get("scheduler")
        .unwrap();
    assert!(scheduler.enabled);
    assert_eq!(scheduler.max_attempts, 5);
}

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
            scheduler = { implementation="vps", poll_interval_secs=1, retention_interval_secs=3600, invocation_budget_secs=240, retention_reserve_secs=5, enabled=false, concurrency=1, max_attempts=2, retry_backoff_secs=1, lease_secs=5, max_payload_bytes=20971520, max_email_bytes=20971520 }
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
fn shipping_sender_is_typed_from_toml_and_rejects_incomplete_values() {
    let sender: ShippingSender = config::Config::builder()
        .add_source(config::File::from_str(
            r#"
name="Sender"
phone="123"
email="sender@example.test"
address="1 Test"
sub_district="Sub"
district="District"
province="Province"
postcode="00123"
country_code="TH"
"#,
            config::FileFormat::Toml,
        ))
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    assert_eq!(sender.postcode, "00123");
    assert!(
        serde_json::from_value::<ShippingSender>(serde_json::json!({"postcode":"00123"})).is_err()
    );
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
fn resend_configuration_retains_missing_deadline_for_optional_client_validation() {
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
fn review_plan_sender_environment_is_centralized_and_redacted() {
    let settings = temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://unused")),
            (
                "JWT_SECRET",
                Some("abababababababababababababababababababababababababababababababab"),
            ),
            ("SHIPPING_SENDER_NAME", Some("Synthetic Sender")),
            ("SHIPPING_SENDER_PHONE", Some("0000000000")),
            ("SHIPPING_SENDER_EMAIL", Some("sender@example.test")),
            ("SHIPPING_SENDER_ADDRESS", Some("123 Synthetic Street")),
            ("SHIPPING_SENDER_SUB_DISTRICT", Some("Test Subdistrict")),
            ("SHIPPING_SENDER_DISTRICT", Some("Test District")),
            ("SHIPPING_SENDER_PROVINCE", Some("Test Province")),
            ("SHIPPING_SENDER_POSTCODE", Some("10100")),
            ("SHIPPING_SENDER_COUNTRY_CODE", Some("TH")),
        ],
        Settings::new,
    )
    .unwrap();
    let sender = settings.shipping_sender.unwrap();
    assert_eq!(sender.name, "Synthetic Sender");
    assert_eq!(sender.address, "123 Synthetic Street");
    assert!(!format!("{sender:?}").contains("Synthetic"));
}

#[test]
fn explicitly_configured_empty_sender_fields_are_invalid() {
    let mut settings = temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://unused")),
            (
                "JWT_SECRET",
                Some("abababababababababababababababababababababababababababababababab"),
            ),
        ],
        Settings::new,
    )
    .unwrap();
    settings.shipping_sender = Some(ShippingSender {
        name: " ".into(),
        phone: "0000000000".into(),
        email: "sender@example.test".into(),
        address: "Synthetic".into(),
        sub_district: "Synthetic".into(),
        district: "Synthetic".into(),
        province: "Synthetic".into(),
        postcode: "10100".into(),
        country_code: "TH".into(),
    });
    assert!(settings.validate().is_err());
}

#[test]
fn paypal_credentials_are_centralized_and_non_secrets_remain_file_based() {
    let file_config: PaypalConfig = Config::builder()
        .add_source(File::with_name("application"))
        .build()
        .unwrap()
        .get("client.paypal")
        .unwrap();
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
    assert_eq!(paypal.base_url, file_config.base_url);
    assert_eq!(paypal.timeout_secs, file_config.timeout_secs);
    assert!(!format!("{paypal:?}").contains("private-paypal"));
}

#[test]
fn stripe_key_comes_from_central_configuration() {
    let settings = temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://unused")),
            (
                "JWT_SECRET",
                Some("abababababababababababababababababababababababababababababababab"),
            ),
            ("STRIPE_SECRET_KEY", Some("sk_test_private")),
        ],
        Settings::new,
    )
    .unwrap();
    let stripe = settings.client.unwrap().stripe.unwrap();
    assert_eq!(stripe.secret_key.as_deref(), Some("sk_test_private"));
    assert_eq!(stripe.base_url, "https://api.stripe.com/v1/");
    assert_eq!(stripe.timeout_secs, 3);
    assert!(!format!("{stripe:?}").contains("sk_test_private"));
}

#[test]
fn checked_in_resend_timeout_is_three_seconds() {
    let timeout: u64 = Config::builder()
        .add_source(File::with_name("application"))
        .build()
        .unwrap()
        .get("client.resend.timeout_secs")
        .unwrap();
    assert_eq!(timeout, 3);
}

#[test]
fn scheduler_limits_are_validated_at_startup() {
    let settings: Settings = Config::builder()
        .add_source(File::with_name("application"))
        .add_source(File::with_name("application.local"))
        .set_override("browser.public_origin", "http://localhost:8084")
        .unwrap()
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
    assert!(settings.validate().is_ok());
    let mut invalid = settings.clone();
    invalid.scheduler.max_attempts = 0;
    assert!(invalid.validate().is_err());
    invalid.scheduler.max_attempts = 1;
    invalid.scheduler.lease_secs = u64::MAX;
    assert!(invalid.validate().is_err());
}

#[test]
fn blob_secret_override_keeps_file_endpoint_and_redacts_token() {
    let settings = temp_env::with_vars(
        [
            ("DATABASE_URL", Some("postgres://fixture")),
            (
                "JWT_SECRET",
                Some("abababababababababababababababababababababababababababababababab"),
            ),
            ("BLOB_READ_WRITE_TOKEN", Some("synthetic-env-blob-token")),
        ],
        Settings::new,
    )
    .unwrap();
    let blob = settings.client.unwrap().vercel_blob.unwrap();
    assert_eq!(blob.api_url, "http://localhost:3001/");
    assert_eq!(blob.timeout_secs, 3);
    assert_eq!(blob.token.as_deref(), Some("synthetic-env-blob-token"));
    assert!(!format!("{blob:?}").contains("synthetic-env-blob-token"));
}

fn settings_builder() -> ConfigBuilder<DefaultState> {
    Config::builder()
        .add_source(File::from_str(
            include_str!("../../application.toml"),
            FileFormat::Toml,
        ))
        .add_source(File::from_str(
            include_str!("../../application.local.toml"),
            FileFormat::Toml,
        ))
        .set_override("database.url", "postgres://unused")
        .unwrap()
        .set_override("jwt.secret", "ab".repeat(32))
        .unwrap()
}

#[test]
fn scheduler_implementation_is_typed_and_cadence_and_budget_are_bounded() {
    assert!(serde_json::from_str::<CronImplementation>("\"unknown\"").is_err());
    let settings = Settings::load(settings_builder(), "local").unwrap();
    assert_eq!(settings.scheduler.implementation, CronImplementation::Vps);
    assert_eq!(settings.scheduler.poll_interval_secs, 10);
    assert_eq!(settings.scheduler.retention_interval_secs, 3600);
    assert_eq!(settings.scheduler.invocation_budget_secs, 240);
    assert_eq!(settings.scheduler.retention_reserve_secs, 5);
    for (budget, reserve) in [(0, 5), (5, 1), (241, 5), (240, 0), (240, 240), (6, 7)] {
        let mut invalid = settings.clone();
        invalid.scheduler.invocation_budget_secs = budget;
        invalid.scheduler.retention_reserve_secs = reserve;
        assert!(invalid.validate().is_err());
    }
    for (poll, retention) in [(0, 3600), (10, 0), (86401, 3600), (10, 86401)] {
        let mut invalid = settings.clone();
        invalid.scheduler.poll_interval_secs = poll;
        invalid.scheduler.retention_interval_secs = retention;
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn local_toml_can_choose_either_cron_without_deployment_metadata() {
    temp_env::with_vars([("VERCEL_ENV", None::<&str>), ("VERCEL_URL", None)], || {
        for (value, expected) in [
            ("vps", CronImplementation::Vps),
            ("vercel", CronImplementation::Vercel),
        ] {
            let overlay = format!(
                "[scheduler]\nimplementation={value:?}\npoll_interval_secs=1\nretention_interval_secs=2"
            );
            let settings = Settings::load(
                settings_builder().add_source(File::from_str(&overlay, FileFormat::Toml)),
                "local",
            )
            .unwrap();
            assert_eq!(settings.scheduler.implementation, expected);
            assert_eq!(settings.scheduler.poll_interval_secs, 1);
            assert_eq!(settings.scheduler.retention_interval_secs, 2);
            assert_eq!(settings.browser.public_origin, "http://localhost:8084");
        }
    });
}

#[test]
fn production_forces_vercel_even_when_toml_selects_vps() {
    temp_env::with_vars(
        [
            ("VERCEL_ENV", Some("production")),
            ("VERCEL_URL", Some("preview.vercel.app")),
            ("VERCEL_PROJECT_PRODUCTION_URL", Some("project.vercel.app")),
        ],
        || {
            let settings = Settings::load(settings_builder(), "prod").unwrap();
            assert_eq!(
                settings.scheduler.implementation,
                CronImplementation::Vercel
            );
            assert_eq!(settings.browser.public_origin, "https://project.vercel.app");
            assert!(!settings.browser.insecure_local);
        },
    );
}

#[test]
fn production_requires_a_supported_deployment_environment() {
    for environment in [None, Some("unknown"), Some("")] {
        temp_env::with_vars(
            [
                ("VERCEL_ENV", environment),
                ("VERCEL_URL", Some("project.vercel.app")),
            ],
            || assert!(Settings::load(settings_builder(), "prod").is_err()),
        );
    }
}

#[test]
fn unsupported_environment_mode_is_rejected_by_central_reader() {
    temp_env::with_var("RUN_ENV", Some("unknown"), || {
        assert!(Settings::new().is_err());
    });
}
