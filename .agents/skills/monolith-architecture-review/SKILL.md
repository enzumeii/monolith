---
name: monolith-architecture-review
description: Use when reviewing architecture, module boundaries, test organization, or unnecessary layers in this Rust Dioxus and Axum repository.
---

# Monolith architecture review

Read the root [AGENTS.md](../../../AGENTS.md), [architecture](../../../docs/architecture.md) and [verification record](../../../docs/verification.md). Treat the current user request as authoritative; docs describe the baseline rather than authorizing implementation changes.

1. Establish the requested diff/revision and read the connected routes, callers, use cases, models and tests. Separate current code from intermediate branch history; use `rg` and focused Git diffs.
2. Trace each affected boundary using this table. Validate suspected issues in actual consumers before reporting them.

| Boundary | Evidence to inspect |
| --- | --- |
| Shared contracts | Domain dependencies and DTO consumers; persistence/claims stay API-owned |
| Entry points | lib.rs declares/re-exports modules; main.rs owns startup and backoffice root components |
| API layering | HTTP extraction in handlers; use cases/transactions in services; SQL in repositories |
| Browser transport | Backoffice api.rs owns shared HTTP helpers; endpoint-owned api/ modules contain endpoint calls |
| Auth middleware | cookies.rs owns transport/origin checks; token.rs owns bearer/JWT authentication and role checks; preserve explicit bearer precedence |
| Refresh lifecycle | Independent login families, atomic consume/replacement, scoped logout, update/revocation transaction |
| Access policy | Natural JWT expiry after revocation; no promised immediate access invalidation |
| Concurrency | Distinguish atomic writes from global issuance/revocation serialization |
| Source organization | Exported functions before private helpers; unit files mirror core logic under src/tests, including backoffice src/tests/api.rs |
| E2E | services/api/e2e-test/main.rs registers healthz, process teardown and v1 modules; v1/admin.rs registers cron, inventories, orders and transactions; common.rs owns shared fixtures, temporary TOML, secret environment, local process startup/readiness/teardown and TestClient::send |
| Test orchestration | API just test runs Compose up → cargo test → cargo api-e2e → Compose down -v, also on failure; library tests run once with PostgreSQL ready, with no ignored tests; no API environment exports or process management in justfile |
| Configuration | API non-secret settings use typed TOML configuration; sensitive values, including provider credentials and private sender profiles, use environment overrides through the existing configuration reader (see [monolith-configuration](../monolith-configuration/SKILL.md)); E2E owns temporary TOML. Backoffice has one published application.toml and typed reader; verify browser asset URL resolution and startup gating; distinguish planned local/prod sources from implemented code |
| Modularity | Existing framework mechanisms and cohesive components; no requirement-free layers |

3. Map changed behavior to existing tests and their actual assertions. Bearer E2E does not prove cookie/CORS or browser behavior; compilation does not prove UI correctness. Preserve the existing E2E layout and shared Docker PostgreSQL policy.
4. Report validated findings by impact, with file/line evidence, concrete trigger, consequence and the smallest relevant fix. Distinguish bugs from agreed policy tradeoffs and optional coverage additions. If no findings remain, state the checked scope and limits.

A review does not authorize edits, publication, stronger locking, a BFF or new E2E infrastructure. Implement only when the user request includes that work. Put architecture explanations in docs as tables/Mermaid and commands in existing justfiles/concise READMEs. Verify recorded check results against their actual snapshot; preserve failed checks and coverage limits rather than reporting the whole checkout as passing.
