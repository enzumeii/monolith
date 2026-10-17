# Repository rules

These rules apply to the entire repository.

## README files

- Keep every `README.md` concise and focused on prerequisites and commands to run, test, format, and lint the code.
- Do not explain architecture, module structure, or implementation details in README files.
- Prefer documented `just` recipes over duplicating long command sequences.
- Keep the root README limited to prerequisite installer guidance, module README links and the docs directory. Each module README must state prerequisites, what `just setup` does, local execution/verification and useful commands.

## Code clarity

- Make code self-documenting through clear names, small functions, explicit types, and cohesive modules.
- Add code comments only when complex logic needs an explanation that the code cannot reasonably convey. Explain the reason or invariant, rather than restating the code.
- Simplify complex code and split it into understandable, focused components before adding explanatory comments. Avoid unnecessary abstraction.

## Reusable commands

- Add frequently used development, execution, testing, formatting, and linting commands to the appropriate existing `justfile`.
- Place a concise comment immediately above each recipe explaining its purpose and usage, including required arguments when applicable.
- Reuse existing recipes and conventions instead of introducing duplicate commands or alternate command locations.

## End-to-end tests

- Run all E2E tests against a production-like environment. External runtime dependencies, such as PostgreSQL, must run in Docker. The application under test may run locally; Docker is not required for the application itself.
- Send requests directly to the running application's real endpoints over the network. Browser E2E tests must use the running application's real URLs.
- Do not substitute mocks, in-process routers, or directly invoked handlers for deployed services in E2E tests. Use unit or integration tests for those approaches.
- Use the existing Docker Compose setup and `services/api/e2e-test/` suite registered by the API Cargo manifest. Keep `healthz.rs` and `v1/{auth,admins}.rs` aligned with routes; put only shared fixtures/HTTP utilities in `common.rs`. Do not introduce an alternative E2E location.
- Use the database configured by Docker Compose. Create uniquely identifiable fixtures and clean up only records owned by the test; do not create or drop a database per test or reset shared data.
- Keep API `just test` to native `cargo test`, Compose up, `cargo api-e2e`, and Compose down with volumes removed, including on failure. Keep E2E environment defaults and local API startup/readiness/process teardown inside the existing test harness. Share the Compose database during a run and delete its volume afterward.

## Rust module boundaries and layout

- Keep `crates/domain` limited to shared request/response DTOs and wire enums.
- Keep database models, JWT claims, authenticated principals, SQLx queries and transactions in `services/api`.
- Backoffice is a client-only Dioxus app calling Axum directly. Axum owns browser credential cookies, CORS and origin checks.
- Keep `lib.rs` for module declarations and re-exports. Put application startup and backoffice root App components in `main.rs`.
- Keep backoffice shared HTTP construction/decoding/errors in `src/api.rs`, with endpoint calls in `src/api/{auth,admins}.rs`.
- Keep API cookie transport/origin checks in `middleware/cookies.rs` and bearer/JWT authentication/role checks in `middleware/token.rs`. Login/refresh/logout transactions remain in `services/auth.rs`.
- Keep local UI/API URLs on the same hostname (`localhost` in the checked-in application TOML files); exact Origin checks and SameSite cookies require consistent host spelling. E2E keeps its own configured loopback origin.
- Keep API non-secret listener/browser settings in `application.toml`; only database credentials and JWT signing secrets use environment overrides. E2E writes an owned temporary `application.toml` for its local API and deletes it during teardown.
- Backoffice public settings belong in one published `application.toml` and one typed `config.rs` reader. Load settings once at startup before rendering application pages or making API calls. Never put secrets in the frontend configuration.
- When deployment configuration is implemented, select `application.local.toml` or `application.prod.toml` and publish it as `application.toml`; retain the single runtime reader. This split is currently planned, not implemented.
- Put exported functions before private helpers, including within inherent impl blocks.
- Put unit tests in each crate's `src/tests/` directory, mirroring implementation module paths. Use test-gated child-module declarations to preserve private access.
- Keep transaction boundaries in services and SQL queries in repositories.

## Preserve agreed behavior

- Preserve independent refresh families and atomic consume/replacement and update/revocation operations.
- Do not introduce immediate access-JWT revocation or stronger cross-request locking without an explicit requirement.
- Preserve intentional admin/seed migrations. Consolidating unpublished migrations requires explicit authorization; do not rewrite applied production migration history.
- Prefer existing framework lifecycle and resource mechanisms. Add infrastructure or abstractions only when an agreed requirement needs them.

## Documentation and repository skills

- Keep `docs/` focused on tables and Mermaid diagrams with brief supporting prose. Record verification results separately from architecture and state coverage limits explicitly.
- For API endpoint testing, use `.agents/skills/monolith-api-e2e/SKILL.md`.
- For architecture reviews, use `.agents/skills/monolith-architecture-review/SKILL.md`.

- For internal API endpoints and external HTTP/GraphQL dependencies, use `.agents/skills/monolith-api-postman/SKILL.md`; keep matching Postman requests synchronized in the same change.
- For reusable runtime dependencies, use `.agents/skills/monolith-dependency-injection/SKILL.md`; construct clients once in AppState and inject shared instances.
