---
name: monolith-api-postman
description: Use when adding, changing, removing, or reviewing internal API endpoints, outbound API connections, or their Postman requests in this repository.
---

# API contracts and Postman

Read root [AGENTS.md](../../../AGENTS.md), the affected routes/client methods and existing requests under `postman/collections/`.

1. Every new or changed internal API endpoint or outbound API connection must include matching executable Postman requests in the same change. This applies to REST/HTTP and other supported API protocols as well as GraphQL; it is not limited to `.graphql` changes. Remove obsolete requests when an operation is removed. Non-API dependencies such as databases or libraries use their appropriate tooling.
2. Make requests usable for testing actual responses: include the method, URL, path/query parameters, headers, authentication, body and pagination inputs as applicable. Document required variables and expected response status/body, with safe representative success/error examples. Reuse existing environment and base URL variables; never embed credentials or sensitive data. Saved examples do not replace a runnable request.
3. For GraphQL, additionally pair every application `.graphql` document with an identical Postman query, including supporting pagination operations. Align variables and examples; use synthetic IDs and initial cursors appropriate to the API.
4. Reuse existing requests/documents/models and preserve request names, unrelated settings and local edits. Remove obsolete duplicates when consolidating operations. If Postman cannot exercise an API protocol, report that limitation and identify an appropriate way to test its responses instead of inventing an HTTP endpoint.
5. Keep client methods scoped to one upstream operation; services own pagination and aggregation. Paginated endpoints forward the requested page size/cursor and return upstream page metadata. Complete-result endpoints traverse upstream pages using the largest supported page size and aggregate before responding. Retain completeness metadata and reject unexpectedly truncated results. Keep transport retries in the shared HTTP client.
6. Use API `just fix` for Rust/GraphQL formatting and `just check` for formatting/lint checks. Synchronize embedded Postman queries after formatting. Validate changed request YAML/JSON, required parameters and variables; compare GraphQL queries with their sources and reject missing/obsolete variable keys. Run local Postman lint. Static checks do not establish that a live API returns the expected response.

For reusable runtime clients, read [monolith-dependency-injection](../monolith-dependency-injection/SKILL.md).
