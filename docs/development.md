# Development and operations

Use each package's README and justfile for prerequisites and commands. [Architecture](architecture.md) describes ownership and behavior; [verification](verification.md) records the current evidence.

## Configuration

| Owner | Source / setting | Behavior |
| --- | --- | --- |
| API | `application.toml`, then optional `application.{RUN_ENV}.toml` | Environment overlay; `RUN_ENV` defaults to `local` |
| API listener | `server.host`, `server.port` in TOML | Non-secret bind settings |
| API browser | `browser.public_origin`, `browser.insecure_local` in TOML | Exact origin; HTTPS unless explicitly using loopback HTTP |
| Database | `DATABASE_URL` from ignored `.env` or environment | Secret PostgreSQL connection URL |
| Signing key | `JWT_SECRET` from ignored `.env` or environment | Hex-encoded random key of at least 32 bytes |
| Shopify URL | `client.shopify.admin_api_url` in `services/api/application.toml` | Full `https://<shop>.myshopify.com/admin/api/2026-10/graphql.json`; local overlay does not replace it |
| Shopify token | `SHOPIFY_ACCESS_TOKEN` from ignored API `.env` or environment | ConfigBuilder overrides client.shopify.access_token before one deserialization; standard file values apply when there is no environment override; Debug redacts credentials |
| Backoffice | Public `application.toml`, `[api] base_url` | Default `http://localhost:3000/v1`; loaded once by typed `Settings`/`ApiConfig` |
| Local browser | UI `http://localhost:8084`, API `http://localhost:3000` | Same hostname is required for exact origins and SameSite cookies |
| Production browser | HTTPS and exact same-site UI/API origin configuration | Cross-site deployment needs a separate cookie/CORS design |
| Frontend environments | One published `application.toml` | Build-time local/prod file selection is planned, not implemented |

Inherited environment variables keep dotenvy's normal precedence. API non-secret listener/browser settings do not use environment overrides. Absent Shopify configuration is optional when no token override is supplied. A token override requires the admin URL from file configuration; missing required fields produce normal configuration errors. Explicit client construction parses the trusted URL and validates the token/header. just setup scaffolds an empty SHOPIFY_ACCESS_TOKEN only in new .env files and preserves existing files.

```mermaid
flowchart LR
    TOML["API TOML + optional overlay"] --> SETTINGS["config.rs typed settings"]
    SECRETS["Ignored .env / environment"] --> SETTINGS
    SETTINGS --> API["Axum listener and database pool"]
    SETTINGS --> CLIENT["AppState ShopifyClient initialization"]
    ASSET["Public frontend TOML asset"] --> LOADER["Fetch and parse once"]
    LOADER --> UI["Pages and session restoration"]
    UI -->|"Credentials and Origin"| API
```

| Backoffice startup state | Behavior |
| --- | --- |
| Loading | Wait before rendering application pages or making API calls |
| Loaded | Provide settings and restore session |
| Fetch/parse error | Show error and Retry; resume only after valid settings load |

## Package commands

Run recipes from the package directory.

| Package | Recipes |
| --- | --- |
| API | `just setup`, `just run`, `just test`, `just check`, `just fix`, `just db-down` |
| Backoffice | `just setup`, `just serve`, `just build`, `just test`, `just check` |
| Domain | `just setup`, `just test`, `just check`; library has no server |
| Client UI | `just setup`, `just run`, `just test`, `just check`; run currently reports a placeholder |
| Workspace formatting | `cargo fmt --all -- --check` from the repository root |

| API verification mode | Dependencies and scope |
| --- | --- |
| `cargo test --locked -p api --lib` | Native library tests and loopback transport fixtures; no Docker or Shopify credentials |
| `just test` | Native Cargo tests, Compose PostgreSQL, `cargo api-e2e`, and Compose down with volumes removed even on failure |
| `just check` | Formatting and strict Clippy on all API targets |
| API E2E | Existing `e2e-test/healthz.rs` and `e2e-test/v1/{auth,admins,products}.rs`; real TCP endpoints |
| Shared harness | `e2e-test/common.rs` owns temporary TOML, local API child, readiness, and teardown |
| Fixtures | Unique owned records; shared Compose database during a run; cleanup only test-owned data |
| Unit layout | `src/tests/` mirrors implementation paths; test-gated child modules preserve private access |
| Domain contracts | `crates/domain/tests/contracts.rs` verifies shared public serialization |

Do not substitute loopback client fixtures or in-process handler tests for deployed-service E2E. Shopify transport fixtures are native integration tests; routine validation does not call Shopify or change store inventory.

## Using the Shopify client

Handlers pass AppState to report services; services resolve the shared state.shopify client initialized once by app_state. Request/result models are exported through api::shopify_models; construct IDs with ShopifyId::new. Paginated proxy methods accept a PageRequest with first (1–250) and nullable after, return one page, and never fetch another page automatically.

| Operation | Caller responsibility |
| --- | --- |
| Product listing | products_with_variants(request, page): location/statuses; one product page with fixed250 variants and completeness metadata |
| Payment lookup | orders_by_payment(request, page): Shopify payment ID; external references may require separate mapping |
| Tag lookup | orders_by_tag(request, page): literal tag; one order page including fixed250 report lines |
| Date-range headers | sold_variants_in_range(request, page): timestamps with offsets and exclusive end; statuses/test/cancellation remain available |
| Order lines | sales_order_lines(request): order ID only; bounded250 lines and hasNextPage metadata |
| Tag replacement | replace_order_tags(request): complete replacement list; empty list clears; inspect userErrors and optional order |
| Inventory discovery | find_inventory_to_activate(request, page): one raw level page; caller decides eligibility from tracked/isActive/on_hand |
| Inventory activation | activate_existing_inventory(request): item/location IDs and stable UUID; no quantity arguments; inspect userErrors/optional level |
| Nested bounds | Fixed250 variants/order lines is the agreed store workload; report services reject unexpected hasNextPage instead of silently truncating |
| Credentials/scopes | Appropriate product/inventory/location/order permissions and approved older-order/protected-customer access where applicable |
| Failure handling | Shopify ClientError failures identify api::client::shopify and retain their typed cause. Convert to AppError at the response boundary or use log_background_error for one ERROR; returning/propagating a Result does not log |
| Privacy | Do not log token values, request/response bodies, raw upstream fields, payment references or customer information |

The report service exhausts outer product/order pages, consumes nested data from those responses and selects active inventory levels. Source models transform their own values; the service owns paging and grouping order. Client methods do not interpret mutation userErrors.

| Outbound request behavior | Policy |
| --- | --- |
| Timeout / redirects | 30 seconds per attempt; redirects disabled |
| Retryable rejection | HTTP 4xx or data-less GraphQL-only `THROTTLED`, sharing one HttpClient retry budget |
| Backoff | At most three retries after the initial attempt; 1/2/4 seconds plus jitter and applicable Retry-After |
| Budget | Shared HTTP/GraphQL attempt count; 30-second total sleep budget |
| Fail without retry | HTTP 5xx and other errors, ambiguous network failures, decoding/partial-data errors, mutation user errors |
| Stored inventory | Inactive levels can retain quantities; activate without resetting available/on-hand |
| Sales interpretation | Current line quantities exclude refunded/removed units and can change after the date range; not event-date net sales |

## Runtime, failures, and administration

| Concern | Mechanism |
| --- | --- |
| Password operations | Argon2 through Tokio `spawn_blocking` |
| Shutdown | Native Axum graceful shutdown on Ctrl-C/SIGTERM |
| Refresh retention | Startup/hourly batches, at most 500 rows, skip locked rows |
| API rejection | Invalid request 400; unauthenticated 401; forbidden 403; missing record 404; duplicate username 409 |
| Internal failure | Generic public 500; underlying source retained internally |
| Normal logs | One boundary event with operation, origin, and category |
| DEBUG logs | `RUST_LOG=info,api=debug` enables underlying API causes/context; diagnostic text may contain sensitive values |
| UI failures | Preserve API status; only 401 triggers restoration; malformed response 502, missing config/coordination 503, stale account state 409 |
| Initial administration | Intentional superadmin seed migration |
| Additional administrators | Authenticated `POST /v1/admins`; API validates credentials and stores hashes |
| Schema changes | Preserve admin/seed behavior and applied migration history; SQLx uses committed offline metadata |
| Deployment | Standalone listener and background maintenance; no serverless adapter or deployment automation |

## Local planning artifacts

| Path | Policy |
| --- | --- |
| `docs/` | Cohesive current architecture, operations, and separately recorded verification; no per-task appendices |
| `docs/superpowers/` | Ignored local plans; excluded from the published tree |
| `.superpowers/` | Ignored local execution scratch/ledgers; excluded from the published tree |
| Postman workspace | Existing local edits are preserved; carrying files to a worktree does not authorize publishing secrets |

## Product report requests

| Request | Input / result |
| --- | --- |
| `GET /v1/products?type=INVENTORY&id=1234` | Numeric location ID; ACTIVE products and active levels at that location; zero/negative on_hand retained |
| `GET /v1/products?type=ORDER_TAG&id=TAG` | Literal tag; merge matching SKU + original unit price + currency and compatible metadata across every matching order page |
| Credentials | Existing ADMIN/SUPER_ADMIN bearer or cookie credentials; Shopify URL in TOML and SHOPIFY_ACCESS_TOKEN environment secret |
| Postman | Internal webserver products requests use `location_id` and `order_tag` variables |
| Verification | Existing API `just test` includes real TCP product routes with Docker PostgreSQL and mocked Shopify, explicitly requested for this feature; live store verification remains manual |
| Report contract | type/id only; one products-only response. Removed page_size/cursor parameters return400 |
| Complete report | Services request first250 root products/orders per call and follow cursors until exhausted. ORDER_TAG aggregation spans every matching order; no client-side continuation |
| Formatting | API just setup installs pinned Prettier with npm ci; just check checks Rust/GraphQL, just fix formats both. CI uses Node.js24 and the lockfile. Update Postman query text after formatting GraphQL |

Report queries require product, inventory, location and order access; reading unitCost may also require View product costs permission. Historical order access remains subject to Shopify permissions. Missing product/variant metadata produces null values; quantities use original order lines, not refund-adjusted accounting.
