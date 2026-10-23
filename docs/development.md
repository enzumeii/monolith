# Development and operations

Use each package's README and justfile for prerequisites and commands. [Architecture](architecture.md) describes ownership and behavior; [verification](verification.md) records the current evidence.

## Configuration

| Owner | Source / setting | Behavior |
| --- | --- | --- |
| API | Common `application.toml` → `application.{RUN_ENV}.toml`; RUN_ENV defaults to local | One existing typed reader; local scheduler.implementation selects vps/vercel, prod forces vercel before deserialization |
| API listener | TOML; platform `PORT` in container server mode | Container listener uses the platform port |
| API browser | `browser.public_origin`, default-empty `browser.additional_origins`, `browser.insecure_local` in TOML | CORS and cookie mutations share exact origins; additions require HTTPS; primary permits explicit loopback HTTP |
| Database | `DATABASE_URL` from ignored `.env` or environment | Secret PostgreSQL connection URL |
| Report storage | Common/local/prod client.s3 TOML; S3_ACCESS_KEY_ID and S3_SECRET_ACCESS_KEY from private environment | Private reports bucket, region ap-southeast-1; independent of database credentials; local setup supplies synthetic missing/empty values. No signed URL persistence. |
| Signing key | `JWT_SECRET` from ignored `.env` or environment | Hex-encoded random key of at least 32 bytes |
| Shopify URL | `client.shopify.admin_api_url` in `services/api/application.toml` | Full `https://<shop>.myshopify.com/admin/api/2026-10/graphql.json`; local overlay does not replace it |
| Shopify token | `SHOPIFY_ACCESS_TOKEN` from ignored API `.env` or environment | ConfigBuilder overrides client.shopify.access_token before one deserialization; standard file values apply when there is no environment override; Debug redacts credentials |
| Backoffice | Public `application.toml`, `[api] base_url` | Default `http://localhost:3000/v1`; loaded once by typed `Settings`/`ApiConfig` |
| Local browser | UI `http://localhost:8084`, API `http://localhost:3000` | Same hostname is required for exact origins and SameSite cookies |
| Production browser | Trusted `VERCEL_PROJECT_PRODUCTION_URL` / `VERCEL_URL` metadata | API derives the generated HTTPS primary alias; production also permits `https://convention.enzumeii.com` from TOML; preview/development clear additional origins; browser calls use same-origin `/api/v1` |
| Vercel local browser | `VERCEL_ENV=development` and explicit loopback `VERCEL_URL` host:port | API permits HTTP/insecure cookies only for `localhost`, `127.0.0.1` or `[::1]` with a nonzero port |
| Frontend environments | Selected local/prod file published as `application.toml` | Production uses `/api/v1`; build restores local source file, including after failure |
| Frontend hosting | Static CSR HTML/WASM/assets under `/backoffice/` and `/user/` | Browser loads public settings before application calls and session restoration |
| Cron authorization | Environment `CRON_SECRET` | Backend only; never embedded in browser assets |
| Local Vercel environment | Inherited environment → ignored root `.env` → ignored API `.env` | `just vercel-dev` checks DATABASE_URL, JWT_SECRET, CRON_SECRET and S3 credentials, starts/initializes local S3, sets local development metadata and uses the API native dev command through an owned temporary config. The API loads its local TOML overlay and cron choice; production still uses the container |

Inherited environment variables keep dotenvy's normal precedence. Normal API settings remain file-based; container port and trusted generated-host metadata are narrow platform exceptions. Absent Shopify configuration is optional when no token override is supplied. A token override requires the admin URL from file configuration; missing required fields produce normal configuration errors. Explicit client construction parses the trusted URL and validates the token/header. just setup scaffolds an empty SHOPIFY_ACCESS_TOKEN only in new .env files and preserves existing files.

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
| Report generator | `just check`, `just test`; shared library with no setup recipe or server |
| Client UI | `just setup`, `just run`, `just test`, `just check`; existing CSR browser shell |
| Workspace formatting | `cargo fmt --all -- --check` from the repository root |

| API verification mode | Dependencies and scope |
| --- | --- |
| `cargo test --locked -p api --lib` | Library tests and loopback transport fixtures; configured Docker PostgreSQL must be ready; no Shopify credentials |
| `just test` | Compose PostgreSQL/private S3/init, native Cargo tests once, `cargo api-e2e` for TCP endpoint tests only, and Compose down with volumes removed even on failure |
| `just check` | Formatting and strict Clippy on all API targets |
| API E2E | `e2e-test/healthz.rs`, `e2e-test/v1/{auth,admins}.rs`, `e2e-test/v1/admin/{inventories,orders,transactions}.rs` and `e2e-test/v1/admin/cron/{configs,report}.rs`; real TCP endpoints |
| Process lifecycle | `e2e-test/process.rs` exercises owned-child teardown |
| Shared harness | `e2e-test/common.rs` owns temporary TOML, local API child, readiness, and teardown |
| Fixtures | Unique owned records; shared Compose database during a run; cleanup only test-owned data |
| Unit layout | `src/tests/` mirrors implementation paths; test-gated child modules preserve private access |
| Domain contracts | `crates/domain/tests/contracts.rs` verifies shared public serialization |

Do not substitute loopback client fixtures or in-process handler tests for deployed-service E2E. Shopify transport fixtures are native integration tests; routine validation does not call Shopify or change store inventory.

## Using the Shopify client

Handlers resolve the shared state.shopify client initialized once by app_state and pass concrete client references to report services. Services, clients and repositories do not access AppState. Middleware receives only browser/JWT configuration. Request/result models are exported through api::shopify_models; construct IDs with ShopifyId::new. Paginated proxy methods accept a PageRequest with first (1–250) and nullable after, return one page, and never fetch another page automatically.

| Operation | Caller responsibility |
| --- | --- |
| Product listing | products_with_variants(request, page): location/statuses; one product page with fixed250 variants and completeness metadata |
| Accounting enrichment | accounting_orders(search, page): one order page for the accounting service; retains the OrdersByPayment document and nested completeness metadata |
| Tag lookup | orders_by_tag(request, page): literal tags combined with AND; one order page including fixed250 report lines |
| Inventory discovery | find_inventory_to_activate(request, page): one raw level page; caller decides eligibility from tracked/isActive/on_hand |
| Inventory activation | activate_existing_inventory(request): item/location IDs and stable UUID only; no quantity arguments; inspect userErrors/optional level |
| Nested bounds | Fixed250 variants/order lines is the agreed store workload; report services reject unexpected hasNextPage instead of silently truncating |
| Credentials/scopes | Appropriate product/inventory/location/order permissions and approved older-order/protected-customer access where applicable |
| Failure handling | ClientError automatically captures its construction/conversion file:line:column and retains its typed failure and underlying source. Convert to AppError at the response boundary or use log_background_error for one ERROR; returning/propagating a Result does not log |
| Privacy | Do not log token values, request/response bodies, raw upstream fields, payment references or customer information |

Inventory returns one caller-selected product page with active inventory levels; order reports exhaust matching order pages before returning grouped totals. Source models transform their own values; services own pagination and grouping order. Client methods do not interpret mutation userErrors.

| Outbound request behavior | Policy |
| --- | --- |
| Timeout / redirects | client.shopify.timeout_secs in application.toml, checked-in value3 seconds per attempt; redirects disabled |
| Retryable rejection | HTTP 4xx or data-less GraphQL-only `THROTTLED`, sharing one HttpClient retry budget |
| Backoff | At most three retries after the initial attempt; 1/2/4 seconds plus jitter and applicable Retry-After |
| Budget | Shared HTTP/GraphQL attempt count; 30-second total sleep budget |
| Fail without retry | HTTP 5xx and other errors, ambiguous network failures, decoding/partial-data errors, mutation user errors |
| Stored inventory | Enablement sends no initial quantity arguments; live provider quantity behavior remains unverified |

## Stripe-hosted Checkout

The existing Stripe client exposes provider operations to trusted Rust callers. Postman requests in `postman/collections/external/stripe/` call Stripe directly; there is no application checkout/status/webhook endpoint or payment persistence. Shopify is not required.

| Input / operation | Operator contract |
| --- | --- |
| Credentials / version | Existing `stripe` environment URL includes `/v1`; inherited `vault:stripe_secret_key` supplies a sandbox secret locally. No Connect `Stripe-Account` or `Stripe-Version` override: requests use the direct account and its default API version. Record that effective version for any manual exercise; signed events expose their own `api_version` where supplied. Fixtures describe tested object shapes, not live version compatibility |
| Create Checkout Session | Set folder variables for positive integer minor-unit amount, lowercase currency, fixed success/cancel URLs, non-personal `correlation_id`, product name and operation idempotency key. One line item with quantity1 and card payment method; account/device support determines wallet presentation. Keep tax, shipping, discounts and adaptive pricing disabled |
| Retrieve resources | Copy the actual Session ID into `stripe_checkout_session_id`, its PaymentIntent ID into `stripe_payment_intent_id`, and the actual latest Charge ID into the existing Retrieve charge request's `charge_id`. Session `expand[]=payment_intent.latest_charge` and PaymentIntent `expand[]=latest_charge` can return expanded objects; string references remain valid |
| Expected identity | Trusted caller supplies Session ID, merchant account ID, environment, exact money and correlation independently. Retrieve current account must match that merchant; all payment resources must match the expected test/live mode and money. Session and PaymentIntent metadata must retain the correlation |
| Paid verification | Use the typed Rust payment-result verifier for an application decision. It retrieves current provider objects and requires matching context, payment Session paid status, succeeded PaymentIntent with exact received amount and its successful captured Charge. A redirect, completed Session, event name, `no_payment_required` or valid signature alone does not prove paid |
| Expiry / ambiguity | Expire only an open Session with its own stable idempotency key. New create/expire operations are sent once without automatic retries. Re-read state after an expiry race. After ambiguous create/expire transport failure, reconcile the same operation; never blindly issue a new key. Stripe can prune keys after at least24 hours, so replay after retention needs external reconciliation |
| Errors / refunds | Provider rejections, malformed objects, unknown statuses and mismatched account/mode/money/correlation never produce verified paid. Original successful payment verification does not assert an unrefunded balance; refund policy belongs to a consumer |
| Signed events | Pass exact raw bytes, signature header, endpoint secret, trusted time and bounded tolerance to the Rust verifier before decoding. Signature authentication does not deduplicate events or establish payment success; use current resource verification and consumer-owned replay/state handling |

For an explicitly authorized sandbox exercise: retrieve current account and compare the expected merchant → create a test Session → open the returned Stripe-hosted URL and complete the payer flow → retrieve Session, PaymentIntent and Charge → compare the trusted expectations and invoke the Rust verifier. These requests mutate the selected account when creating/expiring; do not run the external collection wholesale against a provider. Example IDs and bodies are synthetic and must be replaced with returned values. No real sandbox or live acceptance is implied by local tests.

Provider references: [Session creation](https://docs.stripe.com/api/checkout/sessions/create), [Session retrieval](https://docs.stripe.com/api/checkout/sessions/retrieve), [PaymentIntent retrieval](https://docs.stripe.com/api/payment_intents/retrieve), [direct-account SDK operation](https://github.com/stripe/stripe-dotnet/blob/master/src/Stripe.net/Services/Accounts/AccountService.cs), [idempotency](https://docs.stripe.com/api/idempotent_requests).

## Runtime, failures, and administration

| Concern | Mechanism |
| --- | --- |
| Password operations | Argon2 through Tokio `spawn_blocking` |
| Shutdown | Native Axum graceful shutdown on Ctrl-C/SIGTERM |
| Refresh retention | Shared cleanup helper; hourly VPS task or reserved Vercel tick time; retained when report scheduling is disabled |
| API rejection | Invalid request400; unauthenticated401; forbidden403; missing Shopify location404; duplicate username409. Missing admin targets return401 with {"error":"Not authorized"} and WWW-Authenticate: Bearer |
| Accounting dependencies | `GET /v1/admin/transactions` requires initialized Stripe, PayPal and Shopify clients. Resolve all three before upstream work; missing/init-failed clients return provider-specific503 in that order. Startup and unrelated endpoints keep optional-provider behavior |
| Accounting collection | Query both payment providers even if one is empty; both empty returns empty transactions/items without Shopify enrichment. Actual upstream failures abort without partial results:502 or timeout504 with safe `Upstream request failed` |
| Internal failure | Generic public 500; underlying source retained internally |
| Normal logs | One boundary event with operation, HTTP status and automatic source location in the prefix; optional client_origin retains the client's original file:line:column. No error messages or causes |
| Error location | First AppError creation/conversion location survives propagation; external-library failures identify our mapping site, not an internal dependency frame |
| DEBUG logs | `RUST_LOG=info,api=debug` enables underlying API causes/context; diagnostic text may contain sensitive values |
| UI failures | Preserve API status; only 401 triggers restoration; malformed response 502, missing config/coordination 503, stale account state 409 |
| Initial administration | Intentional superadmin seed migration |
| Additional administrators | Authenticated `POST /v1/admins`; API validates credentials and stores hashes |
| Schema changes | Preserve admin/seed behavior and applied migration history; SQLx uses committed offline metadata |
| Report receipts | report_results stores audit fields; report_result_files stores ordered private object metadata with parent/filename, parent/position and bucket/key uniqueness. Checkpoint JSON remains worker recovery state; history reads canonical child rows in one SQL snapshot |
| Deployment | Root Vercel services configuration, two CSR builds, one API container and daily HTTP cron; Release workflow publishes releases from main, then waits for production approval before deployment |

## Local planning artifacts

| Path | Policy |
| --- | --- |
| `docs/` | Cohesive current architecture, operations, and separately recorded verification; no per-task appendices |
| `docs/superpowers/` | Ignored local plans; excluded from the published tree |
| `.superpowers/` | Ignored local execution scratch/ledgers; excluded from the published tree |
| Postman workspace | Existing local edits are preserved; carrying files to a worktree does not authorize publishing secrets |

## Inventory and order report requests

| Request | Input / result |
| --- | --- |
| `GET /v1/admin/inventories/{inventory_id}?page_size=50&cursor=abc` | Numeric Shopify location ID; page_size defaults50, range1–250, counts root products; cursor forwards unchanged to Shopify. Returns products and page_info (has_next_page/end_cursor) |
| Inventory rows | ACTIVE products and active inventory levels at the location; zero/negative on_hand retained. A product can produce multiple variant rows; no local row slicing |
| Inventory continuation | Products ordered by total inventory descending across all locations; fixed250 variants per product, no variant traversal. One upstream product page per API call, even if every row filters out at the requested location. Follow returned cursor when has_next_page is true, including empty pages; restart pagination after changing the ordering |
| Missing inventory location | HTTP404 with {"error":"Shopify location not found"}; no catalog query is sent after a failed location lookup |
| `GET /v1/admin/orders/report?tags=TAG` | Comma-separated literal tags, trimmed and combined with AND; empty entries rejected400; root batches250; merge compatible SKU/price/currency rows across every matching order page. Returns products only; page_size/cursor rejected400 |
| Credentials | Existing ADMIN/SUPER_ADMIN bearer or cookie credentials; Shopify URL in TOML and SHOPIFY_ACCESS_TOKEN environment secret |
| Shared helpers | utils/numbers.rs parses native f64 values and multiplies quantities, rounding calculated totals to two decimal places; product models assemble report rows and update totals |
| Upstream errors | Shared mapper returns Upstream request failed with502 for failures and504 for timeouts; original causes remain private |
| Postman | Internal inventories first/continuation and orders report requests use location_id and order_tag variables; replace the synthetic abc cursor with the returned end_cursor |
| Verification | API just test includes real TCP report routes with Docker PostgreSQL and the approved synthetic Shopify upstream; live store verification remains manual |
| Formatting | API just setup installs dprint 0.63.0 with Cargo under services/api/.tools and caches the pinned Rust-based GraphQL/JSON Wasm plugins there. just check checks API GraphQL/JSON, Rust formatting and strict lint; just fix prepares SQLx metadata before formatting. CI uses the same setup recipe. Update Postman query text after formatting GraphQL |

Report queries require product, inventory, location and order access; reading unitCost may also require View product costs permission. Historical order access remains subject to Shopify permissions. Missing product/variant metadata produces null values; quantities use original order lines, not refund-adjusted accounting.

## Inventory enablement

| Request / outcome | Behavior |
| --- | --- |
| POST /v1/admin/inventories/{inventory_id}/enable | Numeric Shopify location ID; no body or query parameters; existing ADMIN/SUPER_ADMIN bearer or cookie credentials |
| Discovery | Validate location, exhaust inventory level batches250 with inactive levels included, then activate unique tracked inactive items with positive on_hand |
| Quantities | Use on_hand for eligibility only; send no onHand or available mutation arguments. Live provider quantity behavior is unverified |
| Response200 | total/success/failed count variant inventory items, not unique products; omit errors if none fail; each failed item includes numeric product_id and a safe reason, so a product ID can repeat |
| Mutation failures | Continue independent items; timeouts report an unconfirmed outcome; prior successful activations are not rolled back |
| Discovery failures | No writes before full discovery: malformed input400, missing location404, unavailable client503, upstream502/timeout504 |
| Provider permission | Shopify write_inventory and permission to activate inventory; one activation enables one variant at one location |
| Manual tooling | Postman inventories/enable uses inherited credentials and location_id; it mutates store inventory when run against a live store |


## Order fulfillment configuration and contracts

| Area | Usage |
| --- | --- |
| Local setup | API `just setup` fills missing/empty SHIPPING_SENDER_* values with synthetic local data, preserving existing nonempty settings and generated secrets on rerun |
| Sender profile | Supply name, phone, email, address, sub_district, district, province, postcode and country_code through SHIPPING_SENDER_*; replace synthetic setup data before actual shipping use. No private sender profile belongs in TOML |
| Email deadline | `client.resend.timeout_secs = 3` in TOML; missing/zero retains an optional-client initialization error and affected calls return 503 |
| Order listing | GET `/v1/admin/orders` supports omitted tags, uppercase fulfillment values, or comma-separated AND tags; report still requires tags. Orders return id, number, nullable full_name (trimmed customer first/last name) and nullable order contact email and nullable shipping_country_code from shippingAddress.countryCodeV2, plus upstream page_info |
| Reship | POST `/v1/admin/orders/1085/reship`, replacing 1085 with the four-digit merchant order number; no body/query; one ListOrders name search (first1, includeContact=true); send before RETURNED tagging; success returns `success` and provider-accepted `email_id` after both operations |
| Fulfillment workload | One selling location, at most 250 order items; completed/cancelled fulfillment orders are skipped; one creation precedes SHIPPING removal |
| Shipping workload | ShippingOrders combines item and shipping-method reads, each fixed250 without child cursors; root orders still paginate. Extra shipping-label query parameters are ignored |
| Provider tooling | ListOrders requires explicit includeContact/includeTags Booleans: listing true/false, merchant lookup true/false, tag lookup false/true. EditOrderTags selects add=true or false; services resolve stored casing and skip no-ops. Read/mutation is not atomic against concurrent case-variant inserts; unrelated tags are preserved |
| Listing cursor | Optional: omission forwards after:null. Supplied cursors must be nonblank; the literal string null is an opaque string, not a JSON null |
| Shipping declaration | Apply the legacy mock rule to any selected price currency: max(sum of non-Tip prices ×5,100). Stock quantities are excluded; stock prices remain included |

## Coverage and releases

| Command / setting | Behavior |
| --- | --- |
| Module `just test` | Existing suite, no coverage tooling required |
| Module `just test --coverage` | Native tests with an isolated report at repository `target/coverage/<crate>/lcov.info` |
| API coverage | Includes TCP API child-process execution; graceful termination flushes profiles before bounded forced cleanup |
| API test isolation | Optional E2E_COMPOSE_PROJECT, E2E_POSTGRES_PORT and E2E_DATABASE_URL allow independent worktrees; defaults remain e2e/5433; E2E_S3_PORT defaults5434 and dedicated synthetic E2E_S3_ACCESS_KEY_ID/E2E_S3_SECRET_ACCESS_KEY isolate storage credentials |
| Coverage CI | The existing check job runs report-generator check/test and the four instrumented module suites sequentially, retains four scoped reports, and uploads them together once to Codecov |
| CODECOV_TOKEN | Add the Codecov repository upload token in GitHub Actions secrets; unavailable secrets skip upload with a visible notice and retain artifacts |
| Root `just setup` / `just check` | Node 24+; setup installs the pinned Conventional Commits reference parser; check runs Node tests, actionlint 1.7.12 and shellcheck |
| Coverage build directory | Local runs isolate builds per crate; CI sets CARGO_LLVM_COV_TARGET_DIR to target and reuses builds sequentially |
| Runtime limits | Native coverage only; WASM compilation remains uninstrumented; browser runtime and doctest coverage are not measured |

| PR title | Version increment |
| --- | --- |
| Any type with `!`, for example `feat(api)!: remove field`, or an uppercase `BREAKING CHANGE:` / `BREAKING-CHANGE:` footer in any landed commit for the PR | Major, including 0.x → 1.0.0 |
| `feat: ...` | Minor |
| `fix: ...` | Patch |
| Other types, including `chore`, `ci`, `docs` | No implicit version increment |

Titles follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/), including case-insensitive types, optional scope and a nonempty description after colon-space. CI and releases share the pinned reference parser. Release classification uses the PR title plus the landed commit bodies and footers for that PR.

| Release rule | Behavior |
| --- | --- |
| Initial baseline | Virtual version 0.1.0 at commit 2f9f9101ad5348c327c75c4504a5827975497ada; earlier history is not released |
| Version-release trigger | Pushes to main only; trusted main code reconciles landed first-parent history in order, including merged fork PRs and native stacks with main as effective trunk |
| Native stacks | Release only merged layers landing on main, once per PR in landed order. Merge/squash layers target their landed merge SHA; rebase commits group by PR and target its final landed SHA. Unmerged upper layers are excluded; ambiguous membership fails |
| Version storage | GitHub vX.Y.Z tags and release notes; Cargo package versions stay unchanged |
| Concurrency/retry | Release jobs serialize in main-releases; approval-gated deployments serialize separately in vercel-production-deployments, so deployment queues cannot replace queued release jobs. Running jobs are never cancelled. Release replay catches queued merges, reuses existing matching tags/releases, and finishes a tag-only partial attempt |
| Conflicts | Invalid titles, direct commits, wrong-trunk or ambiguous associations, conflicting targets, or API errors fail visibly; tags are never moved |
| Changelog | GitHub generated notes bounded by prior tag and final landed PR SHA; bootstrap release links qualifying and preceding unreleased PRs |
| First live release | Requires a human merge; local fixtures/dry runs do not verify hosted write permissions |
| Merge blocking | Repository rules must require the CI check names to enforce title validation before merge; workflow code does not change host rules |

## Vercel services

| Service | Root / builder | Public paths |
| --- | --- | --- |
| `backoffice` | `apps/backoffice/vercel` / `@vercel/static-build` | `/backoffice`, `/backoffice/(.*)` |
| `client-ui` | `apps/client-ui/vercel` / `@vercel/static-build` | `/user`, `/user/(.*)` |
| `api` | `.` / `@vercel/container`; `Dockerfile.api.vercel` | `/api/(.*)` and exact `/_cron` |
| Bindings | None | Browser requests use `/api/v1` directly |

| Root command / file | Purpose |
| --- | --- |
| `just setup` | Install pinned local Vercel CLI and existing CI parser dependencies |
| `just check` | Workflow, shell, policy tests and pinned CLI configuration/service resolution |
| `just vercel-build` | Build both static CSR bundles and the API Docker image |
| `just vercel-dev` | Load local API secrets, start/init persistent local S3, prebuild static bundles, then run all three services on localhost:18027 (optional --port/--queue-port/--runtime-cache-port/--schedules-port); local TOML selects VPS polling or manual Vercel cron |
| `vercel.json` | Static filesystem-first routes, asset404 guards, own-prefix SPA fallbacks, public API and daily cron |
| `scripts/install-vercel-dx.sh` | Compile pinned Dioxus 0.7.10 with the repository Rust toolchain into writable `.tools/vercel-dx`; reuse verified cached version |
| `scripts/build-vercel-ui.sh` | Select public production TOML, build web-only WASM, restore local TOML and copy only public assets under the app prefix |
| `scripts/serve-vercel-ui.mjs` | Development-only static command for unlinked CLI, reading each service's existing routes and generated public directory; excluded from published frontend output |

| Native static build prerequisite | Requirement |
| --- | --- |
| Build host | Linux compiler/linker, pkg-config, OpenSSL development libraries, curl, tar and a writable Rust/cache directory; no sudo, Docker or just required by cloud commands |
| Rust | Repository `rust-toolchain.toml`; install pinned toolchain and wasm32-unknown-unknown target |
| Dioxus | Source-installed 0.7.10 matches host libc; official Linux release executable requires newer glibc than Amazon Linux 2023 |
| Cache | `.tools/vercel-dx`, Cargo registry/toolchain and each app's independent `target/vercel`; ignore generated `vercel/public` |
| Compatibility proof | Amazon Linux 2023 is a local approximation, since Vercel does not publish its exact build image; see verification evidence |

| API invocation | Behavior |
| --- | --- |
| `api` | Existing local configuration and migration behavior; local configuration selects VPS or Vercel cron |
| `api server` | Existing container command; connect only, no automatic migrations; RUN_ENV=prod forces Vercel cron |
| Configuration files | Only application.toml, application.local.toml and application.prod.toml; API image copies common/prod files and sets RUN_ENV=prod; no runtime profiles/CLI flag |
| Cron lifecycle | VPS owns periodic scheduler and independent hourly retention; Vercel awaits authenticated HTTP ticks without background tasks |

The daily `0 0 * * *` schedule is UTC: Hobby execution falls approximately between 07:00 and 07:59 Bangkok. It wakes stored Bangkok cron expressions. Production deployments alone receive scheduled calls; local dev does not schedule them. The backend authenticates `Authorization: Bearer CRON_SECRET`, awaits a bounded 240-second pass and reserves five seconds for retention. Missed/failed invocations await later recovery. Oversized jobs can repeat and exhaust attempts; backlog duration depends on workload.

Static services have no frontend executable or runtime dependency. Their build-only package metadata avoids Cargo backend detection. The root Vercel project includes workspace crates/scripts; each app's generated output is `vercel/public/<prefix>`. Service routes check existing files, reject missing `/assets/` files, then fall back only within that application's prefix. The API owns `/api/v1` directly and returns its ordinary errors for unknown `/api/*` routes. Native `/v1` and `/healthz` aliases receive no public Vercel rewrite. Local unmatched `/` may return CLI directory HTML200; this does not establish hosted root routing.

| Manual production deployment | Operation |
| --- | --- |
| Approval | In the Release run started by a main push: publish releases → **Review deployments** → select production → **wichayutdew** approves → SQLx migrates → all three services deploy in that same run |
| Target | Deploy requires a successful release job. It checks out the exact trusted main HEAD captured by that job as deployment_sha, including when main advances while release or approval is queued |
| Production environment | Configure GitHub environment `production` with required reviewer `wichayutdew`, self-review allowed and a main-only deployment branch policy. The environment gate holds every deployment step, including credential validation and migrations, until approval; the workflow reference alone does not configure these host rules |
| GitHub Actions secrets | `VERCEL_TOKEN`, `VERCEL_ORG_ID` and `VERCEL_PROJECT_ID` for the existing Vercel project, plus `SUPABASE_MIGRATION_DATABASE_URL`; after approval, missing values fail before checkout or tool installation |
| Execution | Install pinned Vercel CLI 65.1.0, validate services, install SQLx CLI 0.9.0 with PostgreSQL/rustls, apply committed migrations, then make one root `vercel deploy --prod --yes` call; normal remote builds include backoffice, client-ui and API in the same project/deployment |
| Completion | Wait for the deployment/build result; failure fails the job, success records the deployment URL in its summary |
| Automation boundary | A main push publishes versions/tags/releases, then offers approval in the same run. Rejected or unapproved deployment does not undo publication; failed release publication skips deployment. There is no separate manual workflow trigger |
| Runtime secrets | Keep database, signing, provider and cron credentials in the existing Vercel project's production environment; CI does not download them. Runtime `DATABASE_URL` must target the same database as the migration URL; this workflow does not set or change it |
| Database | Before deployment, `sqlx --no-dotenv migrate run --source services/api/migrations` applies pending migrations from the selected commit; reruns skip already-applied migrations. The dedicated secret becomes `DATABASE_URL` only for SQLx; preflight checks its presence, and install/build/deploy commands do not receive it. Container startup still connects without migrating |
| Migration failure | Fails the job before any Vercel deployment. Successfully committed migrations remain if a later deployment fails; migrations must remain compatible with the currently running application |
| Verification boundary | Controlled local checks verify workflow contracts and shell behavior; host environment settings and actual approval waiting require separate GitHub readback. Completed Release run 39905230152 cannot acquire this gate retroactively; it applies to future runs after the workflow fix is merged |

Never copy `.env` or local credentials into images or static output. Local validation does not provision, link, migrate or deploy a cloud project.

| Supabase migration connection | Setting |
| --- | --- |
| GitHub secret | Set `SUPABASE_MIGRATION_DATABASE_URL` in repository Actions secrets; do not commit it or paste its password into logs |
| Supplied direct endpoint | `postgresql://postgres:<percent-encoded-password>@db.jbcvsgdvhvzitweteqfo.supabase.co:5432/postgres?sslmode=require` |
| Network choice | The direct endpoint uses IPv6 unless the Supabase IPv4 add-on is enabled. For an IPv4-only runner, copy the **Session pooler** connection from the project's Dashboard Connect panel: port `5432`, database `postgres`, username `postgres.jbcvsgdvhvzitweteqfo`; the pooler host is project/region-specific and must be copied, not guessed |
| Pooler mode | Use the direct connection or session pooler for migrations; avoid transaction pooler port `6543` because SQLx migration locking needs session semantics |
| TLS and escaping | Keep `sslmode=require` in the URL and percent-encode reserved password characters; use the project's database password, not an API key |
| Production runtime | Configure Vercel's production `DATABASE_URL` separately for the same Supabase database; the migration secret is not copied to Vercel |

Connection modes and network support follow [Supabase's PostgreSQL connection guide](https://supabase.com/docs/guides/database/connecting-to-postgres). These local checks do not establish live Supabase connectivity or permissions.
