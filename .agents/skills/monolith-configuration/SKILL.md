---
name: monolith-configuration
description: Use when adding, changing, or reviewing application settings, configuration files, environment variables, or typed configuration readers in this repository.
---

# Application configuration

Read root [AGENTS.md](../../../AGENTS.md) and the affected application's existing configuration and tests. Current readers are [API config.rs](../../../services/api/src/config.rs) and [backoffice config.rs](../../../apps/backoffice/src/config.rs).

1. Apply [monolith-production-configuration](../monolith-production-configuration/SKILL.md) for production-like defaults and local/QA overrides. Put non-sensitive settings in TOML, including endpoints, listener settings, timeouts, limits and feature switches. Use the existing application-file selection and overlays; do not add environment overrides for non-sensitive application settings.
2. Supply secrets, credentials, signing keys, access tokens and private personal contact/address data through environment variables. A shipping sender profile can contain private data even when it is not an API credential. Keep sensitive local values in ignored `.env` files; checked-in templates must use empty placeholders for sensitive fields. Use synthetic contact/address data in test fixtures. Never publish sensitive values in frontend configuration or expose them in Debug output, errors or ordinary logs.
3. Keep loading generic and centralized in the existing `config.rs`. For the API, compose file sources and explicit sensitive-value environment overrides in the configuration builder, then deserialize once into typed settings and validate. Do not patch fields after deserialization, add provider-specific loading branches, or introduce parallel readers. Let normal deserialization and validation report missing or invalid configuration.
4. Load settings once at startup and pass typed values through the existing application state and configuration mechanisms. Handlers, services and clients consume those values instead of reading environment variables or files themselves. Frontend readers load only the published public TOML settings.
5. Preserve intentional required/optional dependency behavior when adding settings. Required configuration errors must remain errors; optional configuration must not silently invent defaults or hide an initialization failure. Do not add a configuration framework or secret fallback without a requirement.
6. Update the affected TOML files, safe environment templates and configuration tests together. Verify file-based non-sensitive values, sensitive override precedence, missing/invalid settings and redaction of credentials and private profile data. Reuse the application's existing check/test recipes; never use real secrets or personal profiles in fixtures.
