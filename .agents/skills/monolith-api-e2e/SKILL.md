---
name: monolith-api-e2e
description: Use when adding, changing, or diagnosing API endpoint E2E tests in this Rust monolith repository.
---

# Monolith API E2E

Read the root [AGENTS.md](../../../AGENTS.md), [development guide](../../../docs/development.md), and the touched route/handler/service before changing tests. The current user request takes precedence over repository defaults.

For every new endpoint, also use [monolith-endpoint-test-coverage](../monolith-endpoint-test-coverage/SKILL.md): use isolated dependencies such as Docker PostgreSQL and controlled API servers, exercise real connections, and cover success, every validation rejection and each distinct handled dependency failure through the real HTTP endpoint. Docker-backed services count as mocked dependencies in this repository; replacing database/client calls with in-process doubles does not establish connection coverage.

| Route ownership | Existing test file |
| --- | --- |
| healthz | `services/api/e2e-test/healthz.rs` |
| v1 auth | `services/api/e2e-test/v1/auth.rs` |
| v1 admin reports | `services/api/e2e-test/v1/admin/{inventories,orders}.rs` |
| v1 admins | `services/api/e2e-test/v1/admins.rs` |
| Shared fixtures/HTTP utilities | `services/api/e2e-test/common.rs` |

The API Cargo manifest registers `e2e-test/main.rs` as the `e2e` test target; preserve that registration.
Declare E2E module trees through named files such as `v1.rs` and `v1/admin.rs`, matching application modules; do not introduce `mod.rs` files.

1. Name the endpoint behavior and expected status/body/database effect. Inspect related scenarios and reuse fixtures; keep endpoint-specific helpers in the owning route module.
2. Reuse the local TCP API runner, Compose PostgreSQL and existing test command. Point real outbound clients at isolated API test servers as described by the coverage skill. The root command is `just --justfile services/api/justfile test`; keep its sequence to Compose up, native `cargo test`, `cargo api-e2e`, and Compose down with volumes removed. Run library tests once with PostgreSQL ready; do not ignore database-dependent tests. Preserve the configured Docker context, database URL and port.
3. Allocate database fixtures with the existing unique username prefix and actual returned account IDs. Clean only owned rows and race controls. Share one Compose database across tests within a run; preserve unrelated rows during fixture cleanup. The recipe deletes the E2E database volume after the run, including failures. For race tests, detached control connections must close on failure so advisory locks cannot block cleanup. API test servers also isolate and reset their own scenario state.
4. Use the existing `TestClient::send` to send requests to real running endpoints. Cookie scenarios must forward cookie name/value from Set-Cookie, use cookie mode and the configured Origin; inspect `middleware/cookies.rs` and `middleware/token.rs` rather than hardcoding production assumptions. Bearer scenarios retain explicit Authorization and JSON refresh payloads.
5. Run the existing recipe and report actual results, failure names and untested behavior. Keep E2E environment defaults, local API startup, readiness and owned-process teardown in the existing test harness, not the justfile. Supply non-secret API settings through an owned temporary `application.toml` and remove it during teardown. Scope required secret/credential environment overrides to the child process; use synthetic credentials for mocks and never change the test process environment. The justfile only coordinates Cargo and Compose. Do not add an alternative E2E root, an API container requirement or backoffice E2E without a user request.

A passing bearer scenario does not establish cookie, CORS or browser behavior. Keep coverage claims specific to exercised paths.
