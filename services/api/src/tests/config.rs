#![allow(clippy::unwrap_used)]
use super::*;
use config::{Config, File, FileFormat};

#[test]
fn production_allowed_origins_do_not_require_hostname_metadata() {
    for (deployment, production) in [
        (None, None),
        (
            Some("unique-preview.vercel.app"),
            Some("convention.enzumeii.com"),
        ),
        (Some("invalid/path"), Some("https://invalid.example/path")),
    ] {
        let settings = temp_env::with_vars(
            [
                ("RUN_ENV", Some("prod")),
                ("VERCEL_ENV", Some("production")),
                ("VERCEL_URL", deployment),
                ("VERCEL_PROJECT_PRODUCTION_URL", production),
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
        assert_eq!(
            settings.browser.allowed_origins,
            vec![
                "https://convention.enzumeii.com",
                "https://enzumeii.vercel.app"
            ]
        );
        assert!(!settings.browser.insecure_local);
    }
}

#[test]
fn empty_browser_allowed_origins_are_rejected() {
    let settings: Settings = settings_builder()
        .set_override("browser.allowed_origins", Vec::<String>::new())
        .unwrap()
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    assert!(settings.validate().is_err());
}

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
        assert_eq!(
            settings.browser.allowed_origins,
            vec!["http://localhost:18027"]
        );
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
        assert_eq!(
            settings.browser.allowed_origins,
            vec![format!("http://{host}")]
        );
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
        settings.browser.allowed_origins,
        vec!["https://synthetic-deployment.vercel.app"]
    );
    assert!(!format!("{settings:?}").contains("synthetic-cron-secret"));
    assert!(!settings.browser.insecure_local);
    assert_eq!(settings.database.max_connections, 2);
    assert_eq!(settings.database.min_connections, 1);
    assert_eq!(
        settings.database.url,
        "postgres://synthetic:secret@localhost/runtime"
    );
    assert_eq!(
        settings.client.unwrap().s3.unwrap().endpoint,
        "https://jbcvsgdvhvzitweteqfo.storage.supabase.co/storage/v1/s3"
    );
}

#[test]
fn preview_uses_only_its_generated_origin_and_ignores_production_metadata() {
    let settings = temp_env::with_vars(
        [
            ("RUN_ENV", Some("prod")),
            ("VERCEL_ENV", Some("preview")),
            ("VERCEL_URL", Some("unique-preview.vercel.app")),
            (
                "VERCEL_PROJECT_PRODUCTION_URL",
                Some("convention.enzumeii.com"),
            ),
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
    assert_eq!(
        settings.browser.allowed_origins,
        vec!["https://unique-preview.vercel.app"]
    );
    assert!(!settings.browser.insecure_local);
}

#[test]
fn production_forces_secure_cookies_even_if_file_requests_insecure_local() {
    let settings = temp_env::with_vars(
        [
            ("VERCEL_ENV", Some("production")),
            ("VERCEL_URL", None),
            ("VERCEL_PROJECT_PRODUCTION_URL", None),
        ],
        || {
            Settings::load(
                settings_builder()
                    .set_override(
                        "browser.allowed_origins",
                        vec!["https://configured.example.test"],
                    )
                    .unwrap()
                    .set_override("browser.insecure_local", true)
                    .unwrap(),
                "prod",
            )
        },
    )
    .unwrap();
    assert!(!settings.browser.insecure_local);
    assert_eq!(
        settings.browser.allowed_origins,
        vec!["https://configured.example.test"]
    );
}

#[test]
fn preview_metadata_rejects_untrusted_origins_and_missing_host() {
    let overlong_label = format!("{}.vercel.app", "a".repeat(64));
    let overlong_hostname = format!(
        "{}.{}.{}.{}.vercel.app",
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(63)
    );
    for host in [
        None,
        Some("evil.example"),
        Some("localhost:18037"),
        Some("*.vercel.app"),
        Some("-project.vercel.app"),
        Some("project-.vercel.app"),
        Some("project..vercel.app"),
        Some("project_name.vercel.app"),
        Some(overlong_label.as_str()),
        Some(overlong_hostname.as_str()),
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
    assert_eq!(settings.database.max_connections, 2);
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
    assert_eq!(
        settings.browser.allowed_origins,
        vec!["http://localhost:8084"]
    );
    assert!(settings.browser.insecure_local);
    assert_eq!(settings.client.unwrap().shopify.unwrap().timeout_secs, 10);
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
            browser = { allowed_origins = ["http://localhost:8084"], insecure_local = true }
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
    assert_eq!(stripe.timeout_secs, 10);
    assert!(!format!("{stripe:?}").contains("sk_test_private"));
}

#[test]
fn checked_in_resend_timeout_is_ten_seconds() {
    let timeout: u64 = Config::builder()
        .add_source(File::with_name("application"))
        .build()
        .unwrap()
        .get("client.resend.timeout_secs")
        .unwrap();
    assert_eq!(timeout, 10);
}

#[test]
fn scheduler_limits_are_validated_at_startup() {
    let settings: Settings = Config::builder()
        .add_source(File::with_name("application"))
        .add_source(File::with_name("application.local"))
        .set_override("browser.allowed_origins", vec!["http://localhost:8084"])
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
            assert_eq!(
                settings.browser.allowed_origins,
                vec!["http://localhost:8084"]
            );
        }
    });
}

#[test]
fn production_preserves_toml_scheduler_selection() {
    temp_env::with_vars(
        [
            ("VERCEL_ENV", Some("production")),
            ("VERCEL_URL", None),
            ("VERCEL_PROJECT_PRODUCTION_URL", None),
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
            for (value, expected) in [
                ("vps", CronImplementation::Vps),
                ("vercel", CronImplementation::Vercel),
            ] {
                let overlay = format!("[scheduler]\nimplementation={value:?}");
                let settings = Settings::load(
                    settings_builder()
                        .add_source(File::from_str(
                            include_str!("../../application.prod.toml"),
                            FileFormat::Toml,
                        ))
                        .add_source(File::from_str(&overlay, FileFormat::Toml)),
                    "prod",
                )
                .unwrap();
                assert_eq!(settings.scheduler.implementation, expected);
                assert_eq!(
                    settings.browser.allowed_origins,
                    vec![
                        "https://convention.enzumeii.com",
                        "https://enzumeii.vercel.app"
                    ]
                );
                assert!(!settings.browser.insecure_local);
            }
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

#[test]
fn s3_checked_in_settings_use_private_production_endpoint_and_local_override() {
    let base = Config::builder()
        .add_source(File::with_name("application"))
        .build()
        .unwrap();
    assert_eq!(
        base.get_string("client.s3.endpoint").unwrap(),
        "https://jbcvsgdvhvzitweteqfo.storage.supabase.co/storage/v1/s3"
    );
    assert_eq!(
        base.get_string("client.s3.region").unwrap(),
        "ap-southeast-1"
    );
    assert_eq!(base.get_string("client.s3.bucket").unwrap(), "reports");
    assert_eq!(base.get_int("client.s3.signed_url_ttl_secs").unwrap(), 300);
    assert!(base.get_bool("client.s3.force_path_style").unwrap());
    let local = Config::builder()
        .add_source(File::with_name("application"))
        .add_source(File::with_name("application.local"))
        .build()
        .unwrap();
    assert_eq!(
        local.get_string("client.s3.endpoint").unwrap(),
        "http://localhost:3001"
    );
}

#[test]
fn s3_credentials_are_centralized_redacted_and_non_secret_env_does_not_override_toml() {
    temp_env::with_vars(
        [
            ("S3_ACCESS_KEY_ID", Some("synthetic-env-access")),
            ("S3_SECRET_ACCESS_KEY", Some("synthetic-env-secret")),
            ("S3_ENDPOINT", Some("https://wrong.example.test")),
            ("AWS_ACCESS_KEY_ID", Some("unused-ambient-aws-id")),
            ("AWS_SECRET_ACCESS_KEY", Some("unused-ambient-aws-secret")),
        ],
        || {
            let settings = Settings::load(
                settings_builder()
                    .set_override("client.s3.access_key_id", "file-access")
                    .unwrap()
                    .set_override("client.s3.secret_access_key", "file-secret")
                    .unwrap(),
                "local",
            )
            .unwrap();
            let s3 = settings.client.unwrap().s3.unwrap();
            assert_eq!(s3.endpoint, "http://localhost:3001");
            assert_eq!(s3.access_key_id.as_deref(), Some("synthetic-env-access"));
            assert_eq!(
                s3.secret_access_key.as_deref(),
                Some("synthetic-env-secret")
            );
            assert!(!format!("{s3:?}").contains("synthetic-env"));
        },
    );
}

#[test]
fn s3_absent_credentials_remain_optional_and_inherited_values_are_preserved() {
    temp_env::with_vars(
        [
            ("S3_ACCESS_KEY_ID", None::<&str>),
            ("S3_SECRET_ACCESS_KEY", None),
        ],
        || {
            let config = Settings::load(settings_builder(), "local")
                .unwrap()
                .client
                .unwrap()
                .s3
                .unwrap();
            assert!(config.access_key_id.is_none());
            assert!(config.secret_access_key.is_none());
            let settings = Settings::load(
                settings_builder()
                    .set_override("client.s3.access_key_id", "inherited-access")
                    .unwrap()
                    .set_override("client.s3.secret_access_key", "inherited-secret")
                    .unwrap(),
                "local",
            )
            .unwrap();
            let config = settings.client.unwrap().s3.unwrap();
            assert_eq!(config.access_key_id.as_deref(), Some("inherited-access"));
            assert_eq!(
                config.secret_access_key.as_deref(),
                Some("inherited-secret")
            );
            assert!(!format!("{config:?}").contains("inherited"));
        },
    );
}

#[test]
fn browser_allowed_origins_reject_malformed_values_in_any_position() {
    for insecure_local in [false, true] {
        for origin in [
            "http://custom.example.test",
            "https://*.example.test",
            "https://user@custom.example.test",
            "https://user:pass@custom.example.test",
            "https://custom.example.test/",
            "https://custom.example.test/backoffice/",
            "https://custom.example.test?query=1",
            "https://custom.example.test#fragment",
            "null",
            "*",
            "custom.example.test",
            "",
            "https://custom.example.test:invalid",
            "https://custom.example.test:65536",
            "https://custom.example.test:",
        ] {
            for origins in [
                vec![origin, "https://valid.example.test"],
                vec!["https://valid.example.test", origin],
            ] {
                let settings: Settings = settings_builder()
                    .set_override("browser.allowed_origins", origins)
                    .unwrap()
                    .set_override("browser.insecure_local", insecure_local)
                    .unwrap()
                    .build()
                    .unwrap()
                    .try_deserialize()
                    .unwrap();
                assert!(
                    settings.validate().is_err(),
                    "{origin}, insecure_local={insecure_local}"
                );
            }
        }
    }
}

#[test]
fn browser_allowed_origins_accept_https_and_only_explicit_loopback_http() {
    for (origins, insecure_local) in [
        (
            vec![
                "https://convention.enzumeii.com",
                "https://custom.example.test:8443",
            ],
            false,
        ),
        (
            vec!["http://localhost:8084", "https://convention.enzumeii.com"],
            true,
        ),
        (
            vec![
                "https://convention.enzumeii.com",
                "http://127.0.0.1:8084",
                "http://[::1]:8084",
            ],
            true,
        ),
    ] {
        let settings: Settings = settings_builder()
            .set_override("browser.allowed_origins", origins)
            .unwrap()
            .set_override("browser.insecure_local", insecure_local)
            .unwrap()
            .build()
            .unwrap()
            .try_deserialize()
            .unwrap();
        settings.validate().unwrap();
    }
    let settings: Settings = settings_builder()
        .set_override("browser.allowed_origins", vec!["http://localhost:8084"])
        .unwrap()
        .set_override("browser.insecure_local", false)
        .unwrap()
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    assert!(settings.validate().is_err());
    assert!(
        serde_json::from_value::<BrowserConfig>(serde_json::json!({"insecure_local": false}))
            .is_err()
    );
}

#[test]
fn vercel_nonproduction_metadata_replaces_the_configured_allowed_origins() {
    for (mode, environment, host) in [
        ("local", "development", "localhost:18027"),
        ("prod", "development", "localhost:18027"),
        ("prod", "preview", "preview.vercel.app"),
    ] {
        let settings = temp_env::with_vars(
            [
                ("VERCEL_ENV", Some(environment)),
                ("VERCEL_URL", Some(host)),
            ],
            || {
                Settings::load(
                    settings_builder()
                        .set_override(
                            "browser.allowed_origins",
                            vec![
                                "https://custom.example.test",
                                "https://another.example.test",
                            ],
                        )
                        .unwrap(),
                    mode,
                )
            },
        )
        .unwrap();
        assert_eq!(
            settings.browser.allowed_origins,
            vec![if environment == "development" {
                format!("http://{host}")
            } else {
                format!("https://{host}")
            }],
            "{mode}: {environment}"
        );
        assert_eq!(
            settings.browser.insecure_local,
            environment == "development"
        );
    }
}

#[test]
fn all_environments_keep_optional_stripe_key_semantics() {
    for key in [
        None,
        Some(""),
        Some("sk_live_synthetic"),
        Some("unclassified_synthetic"),
    ] {
        temp_env::with_vars(
            [
                ("STRIPE_SECRET_KEY", key),
                ("VERCEL_ENV", Some("preview")),
                ("VERCEL_URL", Some("synthetic.vercel.app")),
            ],
            || {
                for mode in ["local", "qa", "prod"] {
                    let settings = Settings::load(settings_builder(), mode).unwrap();
                    assert_eq!(
                        settings
                            .client
                            .unwrap()
                            .stripe
                            .unwrap()
                            .secret_key
                            .as_deref(),
                        key,
                    );
                }
            },
        );
    }
}

#[test]
fn qa_startup_probe() {
    let Ok(expected) = env::var("QA_CONFIG_PROBE") else {
        return;
    };
    let result = Settings::new();
    if expected == "missing-overlay" {
        assert!(result.unwrap_err().to_string().contains("application.qa"));
        return;
    }
    let settings = result.unwrap();
    assert_eq!(
        settings.server.port,
        if expected == "qa" { 3099 } else { 3098 }
    );
    assert_eq!(
        settings.browser.allowed_origins,
        vec!["http://localhost:8084"]
    );
    assert!(settings.browser.insecure_local);
    assert_eq!(settings.scheduler.implementation, CronImplementation::Vps);
    let client = settings.client.unwrap();
    assert_eq!(client.s3.unwrap().endpoint, "http://localhost:3001");
    let stripe = client.stripe.unwrap();
    assert_eq!(stripe.base_url, "https://api.stripe.com/v1/");
    assert_eq!(stripe.timeout_secs, 10);
    assert_eq!(stripe.secret_key, env::var("QA_EXPECTED_KEY").ok());
}

#[test]
fn ordinary_dotenv_selects_qa_with_inherited_precedence_and_required_overlay() {
    let fixture = std::env::temp_dir().join(format!("monolith-qa-config-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&fixture).unwrap();
    std::fs::write(
        fixture.join("application.toml"),
        include_str!("../../application.toml"),
    )
    .unwrap();
    std::fs::write(
        fixture.join("application.local.toml"),
        format!(
            "{}\n[server]\nport = 3098\n",
            include_str!("../../application.local.toml")
        ),
    )
    .unwrap();
    std::fs::write(
        fixture.join("application.qa.toml"),
        format!(
            "{}\n[server]\nport = 3099\n",
            include_str!("../../application.qa.toml")
        ),
    )
    .unwrap();
    let run =
        |expected: &str, mode: Option<&str>, key: Option<&str>, expected_key: Option<&str>| {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child
                .args(["--exact", "config::tests::qa_startup_probe", "--nocapture"])
                .current_dir(&fixture)
                .env_clear()
                .env("QA_CONFIG_PROBE", expected)
                .env(
                    "DATABASE_URL",
                    "postgres://synthetic:synthetic@localhost/qa",
                )
                .env("JWT_SECRET", "ab".repeat(32));
            if let Some(mode) = mode {
                child.env("RUN_ENV", mode);
            }
            if let Some(key) = key {
                child.env("STRIPE_SECRET_KEY", key);
            }
            if let Some(key) = expected_key {
                child.env("QA_EXPECTED_KEY", key);
            }
            let output = child.output().unwrap();
            assert!(
                output.status.success(),
                "{expected}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        };
    std::fs::write(
        fixture.join(".env"),
        "RUN_ENV=qa\nSTRIPE_SECRET_KEY=synthetic-dotenv-key\n",
    )
    .unwrap();
    run("qa", None, None, Some("synthetic-dotenv-key"));
    run(
        "local",
        Some("local"),
        Some("synthetic-inherited-key"),
        Some("synthetic-inherited-key"),
    );
    run(
        "qa",
        Some("qa"),
        Some("synthetic-inherited-key"),
        Some("synthetic-inherited-key"),
    );
    std::fs::write(fixture.join(".env"), "RUN_ENV=qa\n").unwrap();
    run("qa", None, None, None);
    std::fs::write(fixture.join(".env"), "").unwrap();
    run("local", None, None, None);
    std::fs::remove_file(fixture.join("application.qa.toml")).unwrap();
    run("missing-overlay", Some("qa"), None, None);
    std::fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn paypal_reporting_configuration_remains_valid_without_payment_settings() {
    let config: PaypalConfig = Config::builder()
        .add_source(File::from_str(
            "base_url='https://api-m.paypal.com'\ntimeout_secs=10",
            FileFormat::Toml,
        ))
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    assert!(config.payment.is_none());
}

#[test]
fn paypal_payment_policy_deserializes_from_toml_without_credentials() {
    let source = "base_url='https://api-m.sandbox.paypal.com'\ntimeout_secs=10\n[payment]\nenvironment='sandbox'\nmerchant_id='MERCHANT123'\nwebhook_id='WH123'\ncurrencies=['USD','JPY']\nreturn_url='https://merchant.example/return'\ncancel_url='https://merchant.example/cancel'";
    let config: PaypalConfig = Config::builder()
        .add_source(File::from_str(source, FileFormat::Toml))
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    assert!(config.client_id.is_none());
    let payment = config.payment.unwrap();
    assert_eq!(payment.environment, PaypalEnvironment::Sandbox);
    assert_eq!(payment.merchant_id, "MERCHANT123");
    assert_eq!(payment.webhook_id, "WH123");
    assert_eq!(payment.currencies, ["USD", "JPY"]);
    assert_eq!(payment.return_url, "https://merchant.example/return");
    assert_eq!(payment.cancel_url, "https://merchant.example/cancel");
    let invalid = source.replace("environment='sandbox'", "environment='unexpected'");
    assert!(
        Config::builder()
            .add_source(File::from_str(&invalid, FileFormat::Toml))
            .build()
            .unwrap()
            .try_deserialize::<PaypalConfig>()
            .is_err()
    );
}
