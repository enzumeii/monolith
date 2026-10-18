---
name: monolith-configuration
description: Use when adding, changing, or reviewing application settings, configuration files, environment variables, or typed configuration readers in this repository.
---

# Application configuration

Read root [AGENTS.md](../../../AGENTS.md) and the affected application's existing configuration and tests. Current readers are [API config.rs](../../../services/api/src/config.rs) and [backoffice config.rs](../../../apps/backoffice/src/config.rs).

1. Put non-secret settings in `application.toml`, including endpoints, listener settings, timeouts, limits and feature switches. Use the existing application-file selection and overlays; do not add environment overrides for non-secret application settings.
2. Supply secrets, credentials, signing keys and access tokens through environment variables. Keep local secret values in ignored `.env` files; checked-in templates may contain variable names and empty placeholders only. Never publish secrets in frontend configuration or expose them in Debug output, errors or ordinary logs.
3. Keep loading generic and centralized in the existing `config.rs`. For the API, compose file sources and explicit secret environment overrides in the configuration builder, then deserialize once into typed settings and validate. Do not patch fields after deserialization, add provider-specific loading branches, or introduce parallel readers. Let normal deserialization and validation report missing or invalid configuration.
4. Load settings once at startup and pass typed values through the existing application state and configuration mechanisms. Handlers, services and clients consume those values instead of reading environment variables or files themselves. Frontend readers load only the published public TOML settings.
5. Preserve intentional required/optional dependency behavior when adding settings. Required configuration errors must remain errors; optional configuration must not silently invent defaults or hide an initialization failure. Do not add a configuration framework or secret fallback without a requirement.
6. Update the affected TOML files, safe environment templates and configuration tests together. Verify file-based non-secret values, secret override precedence, missing/invalid settings and secret redaction. Reuse the application's existing check/test recipes; never use real secrets in fixtures.
