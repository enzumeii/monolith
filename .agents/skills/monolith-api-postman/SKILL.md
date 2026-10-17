---
name: monolith-api-postman
description: Use when adding, changing, removing, or reviewing internal API endpoints, external HTTP/GraphQL dependencies, or their Postman requests in this repository.
---

# API contracts and Postman

Read root [AGENTS.md](../../../AGENTS.md), the affected routes/client methods and existing requests under `postman/collections/`.

1. Add, update or remove matching Postman requests in the same change as internal endpoints or external HTTP/GraphQL dependency operations. Cover methods, URLs, parameters, bodies, authentication and affected response examples. Non-HTTP runtime dependencies use their appropriate tooling; do not invent a Postman endpoint for a database or library.
2. Keep every application `.graphql` document paired with an identical Postman query, including supporting/child-pagination operations. Align required variables and examples; use null initial cursors and synthetic IDs. Use existing `{{server}}`/`{{shopify}}` configuration; never embed credentials or private store data.
3. Reuse existing requests/documents/models and preserve request names, unrelated settings and local edits. Remove obsolete duplicates when consolidating operations.
4. Shopify clients are proxies: one GraphQL operation per method call, with explicit page size and nullable cursor for paginated connections. Parameterize every `first`, including nested connections. Return page metadata; services own traversal/chaining and report aggregation. Transport retries remain in the existing HTTP client.
5. Run API `just fix` for Rust/GraphQL formatting and `just check` for formatting/lint checks. Formatting GraphQL also requires updating embedded Postman query text. Parse changed YAML/JSON variables, compare every query to its source and reject missing/obsolete variable keys; run local Postman lint. Static checks do not establish live Shopify behavior.

For reusable runtime clients, read [monolith-dependency-injection](../monolith-dependency-injection/SKILL.md).
