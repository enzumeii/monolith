# Development and operations

| Reference | Content |
| --- | --- |
| [Architecture](architecture.md) | Ownership, structure, auth flow, branch changes |
| [Verification](verification.md) | Results and coverage limits |
| Package README / justfile | Prerequisites and reusable commands |

## Configuration

| Owner | Setting | Meaning |
| --- | --- | --- |
| API | `application.toml` + optional `application.{RUN_ENV}.toml` | Server configuration overlay; RUN_ENV defaults to local |
| API | `DATABASE_URL` | PostgreSQL secret connection URL |
| API | `JWT_SECRET` | Hex-encoded random signing key; at least 32 bytes |
| API | `application.toml`: `server.host` / `server.port` | Non-secret listener settings |
| API | `application.toml`: `browser.public_origin` | Exact allowed UI origin |
| API | `application.toml`: `browser.insecure_local` | Explicit loopback HTTP; false for production HTTPS |
| Backoffice | `application.toml`: `[api] base_url` | Public API URL including `/v1`; default `http://localhost:3000/v1` |
| Backoffice | `config.rs`: `Settings` / `ApiConfig` | Load/parse the public TOML once at startup before pages or API calls |
| Frontend file | Served as a Dioxus asset | Public settings only; no secrets or alternate configuration mechanism |
| Backoffice | Existing serve recipe | UI at `http://localhost:8084` |
| Local host spelling | Use `localhost` for both UI and API | Different ports: cross-origin, same-site |
| Production browser | HTTPS, exact origin, same-site UI/API | Secure cookie; cross-site policy requires separate design |

```mermaid
flowchart LR
    OP["API TOML + DB/JWT secrets"] --> API["Local Axum API"]
    FILE["Public application.toml asset"] --> URL["Resolve against window.location.href"]
    URL --> LOAD["Fetch and parse typed Settings"]
    LOAD --> UI["Configured Dioxus app"]
    UI -->|"Credentials and Origin"| API
    API --> PG[("Compose PostgreSQL")]
```

| Startup outcome | App behavior |
| --- | --- |
| Loading configuration | Loading indicator; pages/API calls wait |
| TOML loaded and parsed | Provide typed settings to app; restore session |
| Fetch/parse failure | Show error and Retry; no configured app/API calls |

| Configuration status | State |
| --- | --- |
| Current frontend source | `apps/backoffice/application.toml`; one public Dioxus asset |
| Planned deployment sources | `application.local.toml` / `application.prod.toml`; select and publish as `application.toml` |
| Deployment selection | Not implemented; runtime reader remains single |
| Asset loading | Module-level asset; resolve against current page URL before reqwest; nested/base-path unit regressions |
| Strict lint | Native/WASM checks pass; asset macro remains outside deserialized Settings impl |

## Validation and failures

| Input / failure | Rule / response |
| --- | --- |
| Username | 1–20 Unicode characters; no surrounding/all whitespace |
| New password | 12–128 characters |
| Existing login password | Shorter legacy passwords accepted; upper bound 128 |
| Invalid update | 400 |
| Duplicate username | 409 |
| Internal failure | Generic 500 |
| Error conversions | Pure; no logging side effects |
| Error/warning events | `src/error.rs`, once at HTTP/startup/maintenance boundary |
| Logs / Debug | Operation/category only; redact credentials, causes and database URLs |

## Runtime

| Concern | Mechanism |
| --- | --- |
| Argon2 | Direct Tokio `spawn_blocking`; no admission limiter |
| Database pool | One connection; unchanged |
| API shutdown | Native Axum graceful shutdown; Ctrl-C/SIGTERM future |
| UI serving | Dioxus CLI lifecycle |
| Refresh retention | Startup + hourly ticks; at most 500 rows per batch; skip locked rows |
| Vercel | Listener/maintenance not adapted to functions |

## Schema and admin creation

```mermaid
flowchart LR
    MIG["Schema migrations"] --> DB[("PostgreSQL")]
    SEED["Intentional superadmin seed migration"] --> DB
    API["Authenticated POST /v1/admins"] --> HASH["API validates and hashes password"]
    HASH --> DB
```

| Concern | Rule |
| --- | --- |
| Admin/seed migrations | Preserve intentional behavior and the original admin password |
| Refresh migration | Family/retention additions merged into `20260829090958_add_refresh_token.sql` |
| Applied production history | Not a precedent for rewriting migrations |
| Additional admins | Authenticated admin API; super admin permission required |
| Password handling | API validates new passwords and stores Argon2 hashes |
| Key rotation | Old access signatures fail; valid refresh can obtain a newly signed JWT; admin password hash is independent |

## Tests and commands

| Concern | Existing placement / command |
| --- | --- |
| Installer / root guide | `init.sh` installs pinned CLI packages; root README links package guides and docs |
| API startup/test/check | `services/api/justfile`: `run`, `test`, `check` |
| UI serving/build/test/check | `apps/backoffice/justfile`: `serve`, `build`, `test`, `check` |
| Client UI local workflow | `apps/client-ui/justfile`: `setup`, `run`, `test`, `check`; run prints the current stub |
| Domain local workflow | `crates/domain/justfile`: `setup`, `test`, `check`; library has no server |
| Shared contract checks | Domain/backoffice `test` recipes include domain contract tests and WASM compilation |
| Unit-test convention | When unit tests exist, use the crate's `src/tests/`, mirroring core modules; current API/backoffice tests follow this |
| Domain contract test | `crates/domain/tests/contracts.rs`; public integration contract, separate from unit-test convention |
| Backoffice HTTP unit mapping | `src/api.rs` → `src/tests/api.rs` |
| Unit private access | Test-gated child-module registration |
| API E2E | `services/api/e2e-test/{healthz,v1/auth,v1/admins}.rs` |
| E2E registration | API Cargo `[[test]]`: `name = "e2e"`, `path = "e2e-test/main.rs"` |
| Shared E2E utilities | `e2e-test/common.rs` owns API environment, child process, readiness and teardown; endpoint helpers remain with endpoint tests |
| API test sequence | Native `cargo test` → Compose up → `cargo api-e2e` → Compose down `-v`, including on failure |
| Database lifetime | Shared across tests within a run; deleted after the run |
| Dependency | Existing Compose PostgreSQL; API runs locally over TCP |
| Fixtures | Unique owned rows; cleanup only own rows/controls |
| Race controls | Detached connections release advisory locks on failure |
| Retention test | Connection-owned temporary table/rows; API keeps public `cleanup_refresh_tokens` export |
| Refresh scope names | `delete_refresh_family`, `delete_account_refresh_tokens`, `cleanup_refresh_tokens` |
| E2E requests | `TestClient::send`; real TCP HTTP requests |
| Client refresh helper | `refresh_session_from_cookie`; rotates through existing refresh endpoint |
| Backoffice/browser E2E | Excluded by request |

## Client failure classification

| Condition | Status |
| --- | --- |
| Not signed in | 401 |
| Insufficient UI role | 403 |
| Stale request / account changed | 409 |
| Malformed upstream response | 502 |
| Missing configuration / browser coordination / unavailable API | 503 |
| API HTTP failure | Preserve upstream status; only 401 triggers restoration |
