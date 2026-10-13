---
name: monolith-api-e2e
description: Use when adding, changing, or diagnosing API endpoint E2E tests in this Rust monolith repository.
---

# Monolith API E2E

Read the root [AGENTS.md](../../../AGENTS.md), [development guide](../../../docs/development.md), and the touched route/handler/service before changing tests. The current user request takes precedence over repository defaults.

| Route ownership | Existing test file |
| --- | --- |
| healthz | `services/api/e2e-test/healthz.rs` |
| v1 auth | `services/api/e2e-test/v1/auth.rs` |
| v1 admins | `services/api/e2e-test/v1/admins.rs` |
| Shared fixtures/HTTP utilities | `services/api/e2e-test/common.rs` |

The API Cargo manifest registers `e2e-test/main.rs` as the `e2e` test target; preserve that registration.

1. Name the endpoint behavior and expected status/body/database effect. Inspect related scenarios and reuse fixtures; keep endpoint-specific helpers in the owning route module.
2. Reuse the existing Compose PostgreSQL and local TCP API runner. The root command is `just --justfile services/api/justfile test`; keep its sequence to native `cargo test`, Compose up, `cargo api-e2e`, and Compose down with volumes removed. Follow the configured Docker context, database URL and port rather than inventing infrastructure.
3. Allocate fixtures with the existing unique username prefix and actual returned account IDs. Clean only owned rows and race controls. Share one Compose database across tests within a run; preserve unrelated rows during fixture cleanup. The recipe deletes the E2E database volume after the run, including failures. For race tests, detached control connections must close on failure so advisory locks cannot block cleanup.
4. Send requests to real running endpoints. Cookie scenarios must forward cookie name/value from Set-Cookie, use cookie mode and the configured Origin; inspect `middleware/cookies.rs` and `middleware/token.rs` rather than hardcoding production assumptions. Bearer scenarios retain explicit Authorization and JSON refresh payloads.
5. Run the existing recipe and report actual results, failure names and untested behavior. Keep E2E environment defaults, local API startup, readiness and owned-process teardown in the existing test harness, not the justfile. Scope API environment overrides to the child process; do not change the test process environment. The justfile only coordinates Cargo and Compose. Do not add an alternative E2E root, an API container requirement or backoffice E2E without a user request.

A passing bearer scenario does not establish cookie, CORS or browser behavior. Keep coverage claims specific to exercised paths.
