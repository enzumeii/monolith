#![allow(clippy::unwrap_used)]
use super::*;
use crate::config::{
    BrowserConfig, ClientConfig, DatabaseConfig, JwtSettings, ServerConfig, ShopifyConfig,
};

#[tokio::test]
async fn shopify_client_uses_state_settings_lazily_without_database_or_network() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    let settings = Settings {
        server: ServerConfig {
            host: "localhost".into(),
            port: 3000,
        },
        browser: BrowserConfig {
            public_origin: "http://localhost:8084".into(),
            insecure_local: true,
        },
        database: DatabaseConfig {
            url: "postgres://unused".into(),
            min_connections: 0,
            max_connections: 1,
        },
        jwt: JwtSettings {
            secret: "ab".repeat(32),
        },
        client: None,
    };
    let state = app_state(pool.clone(), settings.clone()).unwrap();
    assert!(matches!(
        state.shopify_client(),
        Err(crate::client::ClientError::InvalidInput(_))
    ));
    let configured = Settings {
        client: Some(ClientConfig {
            shopify: Some(ShopifyConfig {
                admin_api_url: "http://localhost:1/custom".into(),
                access_token: Some("token".into()),
            }),
        }),
        ..settings
    };
    let state = app_state(pool, configured).unwrap();
    assert!(state.shopify_client().is_ok());
}
