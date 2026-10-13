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
        ],
        Settings::new,
    )
    .expect("non-secret environment values must not affect settings");
    assert_eq!(settings.server.host, "0.0.0.0");
    assert_eq!(settings.server.port, 3000);
    assert_eq!(settings.browser.public_origin, "http://localhost:8084");
    assert!(settings.browser.insecure_local);
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
