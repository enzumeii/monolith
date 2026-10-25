---
name: monolith-production-configuration
description: Use when adding, changing, or reviewing application.toml defaults, local/QA/production overrides, or configuration packaging in this repository.
---

# Production configuration defaults

Apply [monolith-configuration](../monolith-configuration/SKILL.md) for typed loading, sensitive values and validation.

Treat each application's `application.toml` as the shared, production-like baseline. Use production origins, live provider endpoints and production security settings where the application supports them. Keep secrets and private data in environment variables.

Put local differences in `application.local.toml` and QA differences in `application.qa.toml`, using the application's existing environment selection. Local origins must use matching `localhost` hostnames across UI and API. Sandbox endpoints, insecure cookies and development listeners belong in the relevant overrides. Production inherits the base; retain `application.prod.toml` only for actual production differences or when an existing consumer requires the file. Avoid repeating unchanged settings in overlays.

Before changing defaults or removing an override, inspect its reader, build composition, deployment packaging and test fixtures. Preserve the generic typed reader, explicit secret precedence and optional dependency behavior. Frontend publication must compose the base with the selected override into one complete public `application.toml`.

Verify the effective base and each affected merged environment configuration, including security settings and provider endpoints. Update consumers and fixtures when removing a file; an absent override must not break a required build or silently select another environment. Configuration edits do not authorize a deployment or conversion of unrelated applications.
