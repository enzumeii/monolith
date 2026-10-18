---
name: monolith-api-postman
description: Use when adding, changing, removing, or reviewing internal API endpoints, external HTTP/GraphQL dependencies, or their Postman requests in this repository.
---

# API contracts and Postman

Read root [AGENTS.md](../../../AGENTS.md), the affected routes/client methods and existing requests under `postman/collections/`.

1. Add, update or remove matching Postman requests in the same change as internal endpoints or external HTTP/GraphQL operations. Cover methods, URLs, parameters, bodies, authentication and affected response examples. Non-HTTP dependencies use their appropriate tooling; do not invent Postman endpoints for databases or libraries.
2. Keep every application `.graphql` document paired with an identical Postman query, including supporting pagination operations. Align variables and examples; use synthetic IDs and initial cursors appropriate to the API. Reuse existing environment and base URL variables; never embed credentials or sensitive data.
3. Reuse existing requests/documents/models and preserve request names, unrelated settings and local edits. Remove obsolete duplicates when consolidating operations.
4. Keep client methods scoped to one upstream operation; services own pagination and aggregation. Paginated endpoints forward the requested page size/cursor and return upstream page metadata. Complete-result endpoints traverse upstream pages using the largest supported page size and aggregate before responding. Retain completeness metadata and reject unexpectedly truncated results. Keep transport retries in the shared HTTP client.
5. Use API `just fix` for Rust/GraphQL formatting and `just check` for formatting/lint checks. Synchronize embedded Postman queries after formatting. Parse changed YAML/JSON variables, compare queries with their sources, reject missing/obsolete variable keys, and run local Postman lint. Static checks do not establish live upstream behavior.

For reusable runtime clients, read [monolith-dependency-injection](../monolith-dependency-injection/SKILL.md).
