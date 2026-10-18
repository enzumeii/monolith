---
name: monolith-endpoint-test-coverage
description: Use when introducing an API endpoint or reviewing whether a new endpoint has complete happy-path, request-validation, and external-dependency failure coverage in this repository.
---

# New endpoint E2E coverage

Every new endpoint must include automated E2E coverage in the same change. Read root [AGENTS.md](../../../AGENTS.md), the endpoint's route, validation, service and dependency calls, and [the API E2E harness guidance](../monolith-api-e2e/SKILL.md).

## Test boundary

- Start the real application and call its endpoint over HTTP using the existing `services/api/e2e-test/` harness. Exercise production routing, middleware, validation, service orchestration and response encoding. Direct handler/service calls and in-process router tests do not replace these endpoint tests.
- Mock every external dependency used by the endpoint, including database access, outbound HTTP/GraphQL APIs, caches, queues and external authentication providers. Include dependencies reached through middleware. Tests must not require live providers or real database connections for this new-endpoint coverage.
- Replace dependencies at their boundaries: inject a controlled database/repository test double and direct the real outbound API client to a local mock server. Keep the endpoint's application logic real. Use existing injection seams; if a required dependency cannot be replaced, implement the smallest necessary seam as part of the endpoint work. A Docker database, an unused lazy pool, or mocking the handler itself does not satisfy database mocking.
- Give each scenario isolated mock state, deterministic responses and bounded timeouts. Reject unexpected calls and clean up owned mock servers/application processes, including on failure. Preserve existing real-database integration tests separately; mocked endpoint coverage does not verify SQL, migrations or live provider compatibility.

## Required scenario matrix

Before implementation, list each validation rule and each reachable dependency operation. For every scenario, record its request, mock setup, expected HTTP status/body/headers and expected dependency calls.

| Category | Required coverage |
| --- | --- |
| Happy path | A valid request with successful mock responses returns the exact expected status and response data. Assert relevant headers and side effects; cover documented defaults and pagination/aggregation when present. |
| Request validation | Exercise each rejection condition for path, query, headers and body: missing required values, wrong types/format, bounds and field combinations as applicable. Isolate the failing rule with otherwise valid input. Test accepted boundaries/defaults, and verify rejected requests do not trigger business dependency calls after validation. |
| Authentication/authorization | Exercise each applicable missing/invalid/expired credential and permission rejection through the real middleware. Preserve expected status, safe message and challenge/cookie headers. |
| Dependency failures | For each dependency operation, inject each distinct failure the endpoint handles: unavailable connection, no response/timeout, rejected response, malformed/incomplete response or database operation error as applicable. Assert the endpoint's mapped error and headers, no success/partial result, and no unintended subsequent calls or writes. |

For validation failures, allow only the prerequisite middleware dependency calls required before validation; assert no business operation proceeds. For multi-call flows, include later-call failures to catch partial responses or unintended side effects. Model transaction outcomes in database doubles and retain real-database integration coverage for actual atomicity.

Use reachable behavior to choose cases; do not invent impossible failure branches or enumerate every upstream status when they share one mapping. A missing record or empty upstream result is a domain outcome: test its documented behavior separately from connection/timeout failures.

## Verification and completion

Use [the error-handling skill](../monolith-error-handling/SKILL.md) for descriptive safe errors, HTTP status mappings and diagnostics. Run the existing API `just check` and `just test` recipes; keep tests in the registered E2E suite and endpoint-owned modules, with only shared utilities in `common.rs`. Update [Postman requests](../monolith-api-postman/SKILL.md) in the same endpoint change.

Review the scenario matrix against the implemented tests. Report actual passing cases and remaining gaps; an endpoint is not fully covered if any of the three required categories is missing. State explicitly that external dependencies were mocked and which integration behaviors remain unverified.
