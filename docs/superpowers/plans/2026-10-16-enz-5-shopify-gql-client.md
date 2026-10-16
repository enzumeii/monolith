# ENZ-5 Shopify GraphQL Client Implementation Plan

> For agentic workers: use the executing-plans skill for future implementation. Plan approved in chat; the subsequent instruction to proceed under act-mkt-work authorizes implementation, verification, and publication. Live Shopify mutations remain unauthorized.

**Goal:** Add reusable HTTP and GraphQL transport and six typed Shopify operations inside the Axum API.

**Architecture:** Compose a Shopify client over a GraphQL client over a shared reqwest HTTP client. Shopify wire models remain API-owned; no routes, frontend integration, database work, or new application lifecycle machinery.

**Tech stack:** Existing Rust 2024 workspace, reqwest 0.12, Tokio, serde/serde_json, chrono, uuid, thiserror, and tracing. Promote the existing workspace reqwest dependency into the API's normal dependencies; avoid a new GraphQL generator dependency.

**Source:** [ENZ-5 — Shopify GQL Client](https://linear.app/wichayutdew/issue/ENZ-5/shopify-gql-client), retrieved through Linear MCP on 2026-10-16. Full source description is preserved in the appendix. Acceptance Criteria is empty in the issue; the criteria below are proposed for approval, using the six local Postman requests and this chat's corrections.

## Goal/Acceptance Criteria

| Deliverable | Acceptance criterion |
| --- | --- |
| Generic HTTP transport | `services/api/src/client.rs` provides a reusable reqwest-backed JSON request method, typed responses, bounded timeouts, and distinct transport, HTTP, and decoding failures. Reuse the connection pool. Add bounded exponential backoff for HTTP 429 only; never retry HTTP 5xx. |
| GraphQL transport | Serialize `query`, `operationName`, and typed `variables`; decode typed `data`, GraphQL `errors`, and optional cost/throttle metadata. HTTP 200 with GraphQL errors is a failure, including partial data; only a data-less response consisting exclusively of `THROTTLED` errors qualifies for bounded backoff. Missing data without errors is a protocol failure. Never retry an HTTP 5xx response even if its body mentions throttling. |
| Shopify authentication | `services/api/src/config.rs` loads `SHOPIFY_ACCESS_TOKEN` through the API's existing `.env` workflow into redacted runtime configuration. `services/api/src/client/shopify.rs` receives the loaded configuration and supplies `X-Shopify-Access-Token` per request. The full Shopify Admin API URL is the sole setting under `[client.shopify]` in `application.toml` (example: `admin_api_url = "https://your-shop.myshopify.com/admin/api/2026-10/graphql.json"`). Never send the token to a redirect target. |
| Shopify DTOs | `services/api/src/models/shopify.rs` owns typed inputs, money, product/variant, order/address/transaction, sale line, inventory, pagination, and mutation payload models. Keep decimal amounts as strings paired with currency; honor nullable customer, image, product/variant, and inventory fields. |
| Product listing | Products contain all their variants with SKU and location-contextual price `{ amount, currencyCode }`, description, status, tags, images, and requested-location quantities. Accept product status filters. Traverse product and nested variant cursors without truncation. Preserve Postman's product creation ascending order; this is distinct from the earlier optional descending inventory-scan discussion. |
| Payment lookup | Search orders using an escaped `payment_id` term and return all matching orders and transaction/customer contact details represented in the Postman query. Do not treat arbitrary gateway references as Shopify payment IDs or assume uniqueness. |
| Sales date range | Use explicit offset-aware timestamps and `[start, end)` order creation filtering without financial, cancellation, fulfillment, or test-order exclusions; traverse every matching order and every line item. Return ordered `quantity`, current `currentQuantity`, identifiers, financial/fulfillment status, cancellation state, test flag, and original unit prices with shop/presentment currency. |
| Order tag replacement | Replace the entire tag list using `orderUpdate`; preserve mutation `userErrors` as a typed failure. Empty tags clears tags. This is not an append operation. |
| Inventory discovery | Scan only the supplied location's inventory levels with `includeInactive:true`. Return only levels where the item is tracked, `isActive == false`, and `on_hand > 0`, along with item/variant/product IDs and quantities. Apply those predicates in Rust: Shopify has no documented server-side filter for them. Traverse all inventory pages. |
| Inventory activation | Accept inventory item ID, location ID, and caller-provided idempotency UUID. Call `inventoryActivate` with `@idempotent`; omit both `available` and `onHand`, preserving stored quantities. Discovery never performs mutations. |
| Existing API | No Shopify credentials required for ordinary API startup or native tests. Existing authentication, routes, DB migrations, shared DTOs, and public error contracts remain intact. |

## Non Goal

- No new HTTP endpoints, UI pages, scheduled sync, inventory reset, automatic bulk activation, database persistence, or migration.
- No OAuth installation/token refresh, multi-shop credential store, webhook handling, bulk-operation framework, general retry scheduler, or arbitrary gateway-payment reconciliation.
- No event-date net-sales accounting: this report selects orders created within the range and exposes their current line quantities; later refunds can affect those values.
- No new Postman reorganization. Carry all existing tracked and untracked Postman changes into the future worktree, including deletions, `.postman/resources.yaml`, and environments. Preserve their contents; stage/publish only approved credential-free scope. Never copy secrets to a remote.
- No new Shopify E2E tests and no E2E execution for this client-only change. No live Shopify calls or mutations as part of plan preparation or routine validation.

## Implementation Steps and Tests

### 1. Implement reusable JSON/GraphQL transport

**Files:** create `services/api/src/client.rs`, `services/api/src/tests/client.rs`; modify `services/api/src/lib.rs` for module declarations and `services/api/Cargo.toml` to use workspace reqwest at runtime. Update `Cargo.lock` only if dependency resolution requires it.

**Interfaces:** `HttpClient::new(timeout: Duration) -> Result<Self, ClientError>`; `HttpClient::request_json<B: Serialize, R: DeserializeOwned>(method: Method, url: Url, headers: HeaderMap, body: Option<&B>) -> Result<R, ClientError>`; `GraphqlClient::execute<V: Serialize, D: DeserializeOwned>(headers: HeaderMap, request: &GraphqlRequest<V>) -> Result<D, ClientError>`. GraphqlClient owns its endpoint and a clone of the shared HTTP client. It accepts generic headers per request and knows nothing about Shopify authentication. ShopifyClient constructs and passes its own sensitive token header. All request methods are async. Define the request and response envelope types here, rather than mixing generic envelopes into Shopify DTOs.

- [ ] Write failing tests using a temporary loopback HTTP fixture under `src/tests/`, classified as transport integration tests, not E2E: method/path/headers/body serialization, typed decoding, HTTP 401/429/500, malformed JSON, timeout, GraphQL HTTP-200 errors, partial data plus errors, and missing data.
- [ ] Implement the three concrete composed clients without inheritance traits or a general middleware framework. Disable redirects. Implement a shared bounded backoff policy: initial request plus at most three retries, with delays of 1, 2, and 4 seconds plus 0–250 ms jitter. Retry only HTTP 429 or an HTTP-200 GraphQL response with no data and exclusively `THROTTLED` errors. All HTTP 5xx responses, other HTTP errors, network failures/timeouts, decoding errors, mutation user errors, and partial-data responses fail without retry. Honor a valid `Retry-After` (seconds or HTTP date) as a minimum delay; if the required delay exceeds the 30-second sleep budget for one logical request, return a typed exhausted/deferred error rather than retry early. HTTP and GraphQL retries share one total four-attempt budget; do not nest independent retry loops. Preserve the exact request variables/headers and activation idempotency key across retries. Use a fixed 30-second per-attempt request timeout; do not add timeout fields to Shopify configuration.
- [ ] Test backoff using a paused Tokio clock or an injected test sleeper: 429 then success; GraphQL THROTTLED then success; exhaustion after four attempts; Retry-After minimum; excessive Retry-After stops without early replay; missing/invalid Retry-After falls back to backoff; mixed GraphQL errors/partial data are not retried. For 500, 502, 503, and 504 assert exactly one network attempt and no retry sleep, even with Retry-After or a throttling-shaped body. For mutation throttling assert an unchanged idempotency key on every attempt. Enable Tokio test-util only in dev dependencies if needed.
- [ ] Keep upstream errors typed and retain function origin/source information. Do not log tokens, request/response bodies, payment references, or customer data. Redact credentials from Debug. Retain request ID and throttle metadata where available without logging raw bodies.
- [ ] Run `cargo test --locked -p api --lib client` and confirm failure before implementation and success afterward. Include a redirect test proving a second server receives no credentials.

### 2. Configure the Shopify client and define its wire models

**Files:** create `services/api/src/client/shopify.rs`, `services/api/src/models/shopify.rs`, `services/api/src/tests/models/shopify.rs`, and `services/api/src/tests/client/shopify.rs`; modify `models.rs`, `config.rs`, `src/tests/config.rs`, API TOML files, API README, and the existing `just setup` recipe as needed.

**Interfaces:** `ShopifyClient::new(config: &ShopifyConfig) -> Result<Self, ClientError>` consumes the URL and already-loaded token from typed configuration; it never reads environment variables or loads dotenv itself. Add optional `Settings.client: Option<ClientConfig>` with `ClientConfig.shopify: Option<ShopifyConfig>` and `ShopifyConfig.admin_api_url: String` as its only TOML field. Add an in-memory `access_token: Option<String>` populated exclusively by `config.rs` from `SHOPIFY_ACCESS_TOKEN` after dotenv loading; mark it `#[serde(skip)]` so TOML cannot supply it and implement redacted Debug. A missing client/Shopify section is supported. Keep the complete versioned URL in TOML, not a hardcoded endpoint. Reuse the existing `dotenvy::dotenv()` bootstrap to load `services/api/.env`; inherited environment variables retain dotenvy's normal precedence. Treat the review's `.ev` spelling as `.env`.

- [ ] Test config.rs loads the token from the dotenv/environment workflow into the runtime Shopify configuration, rejects TOML as a token source, and redacts Debug. Missing/empty token remains allowed for ordinary Settings/startup but fails on explicit ShopifyClient construction. Verify an injected typed token works without the client consulting the process environment. Existing Settings struct literals must be updated where necessary.
- [ ] Parse and validate the configured full Admin API URL: HTTPS, a Shopify shop hostname ending in `.myshopify.com`, no userinfo/port/query/fragment, and the approved `/admin/api/2026-10/graphql.json` path. Use this validated configured URL directly. Tests inject a loopback endpoint through a test-only constructor.
- [ ] Define the six request types below and focused wire response structures matching all selected Postman fields, using serde camelCase mapping. Shared `Money`, `Connection<T>`, `PageInfo`, `InventoryQuantity`, and `MutationUserError` belong in this API module. Use typed product status and IDs with resource-kind validation; reject wrong-resource IDs before network calls.
- [ ] Add sanitized JSON fixtures and round-trip/deserialization tests for money with currency, null images/addresses/references, inactive levels with positive stock, and negative/zero quantities. Avoid binary floating point for money.
- [ ] Document only environment prerequisites and verification commands in README. Do not place the token in TOML, generated .env templates, Postman exports, logs, or frontend configuration. Document `SHOPIFY_ACCESS_TOKEN` in the API's ignored `.env` and the URL-only TOML section; do not overwrite an existing `.env` or require a credential for unrelated startup/tests.

### 3. Implement the six Shopify functions with complete pagination

**Files:** implement in `client/shopify.rs`; keep query documents in `services/api/src/client/shopify/` as six `.graphql` files plus private continuation queries where necessary. Extend `models/shopify.rs` and `src/tests/client/shopify.rs`.

All functions are async and return `Result<Output, ClientError>`:

| Function and input DTO | Output and behavior |
| --- | --- |
| `list_products(&ListProductsRequest { location_id, statuses })` | `Vec<Product>` with every variant assembled. Empty statuses means no status clause. Use a typed status-to-search mapping. Fetch one product at a time and paginate that product's variants before advancing. |
| `orders_by_payment_id(&OrdersByPaymentRequest { payment_id })` | `Vec<Order>`; escape Shopify search literals and follow every order page. |
| `sales_in_range(&SalesRangeRequest { start: DateTime<FixedOffset>, end: DateTime<FixedOffset> })` | `Vec<SalesOrder>` containing all sale lines, prices/currencies, financial/fulfillment statuses, cancellation state, and test flag. Include all matching orders regardless of status/test state; upstream services own business filtering and aggregation. Reject `start >= end`. Fetch one order at a time, paging its lines inside the same GraphQL operation before advancing orders. |
| `replace_order_tags(&ReplaceOrderTagsRequest { order_id, tags: Vec<String> })` | Updated `OrderTags { id, name, tags }`, or typed mutation errors. |
| `find_inventory_to_activate(&InventoryDiscoveryRequest { location_id })` | `Vec<InventoryActivationCandidate>` filtered by tracked/inactive/positive-on-hand after reading all location inventory pages. Missing on-hand field is a malformed response, never silently treated as a qualifying candidate. |
| `activate_inventory(&ActivateInventoryRequest { inventory_item_id, location_id, idempotency_key: Uuid })` | Activated inventory level with activation state and quantities; variables contain IDs/key only. Return user errors; never retry with zero quantity or silently replace the supplied idempotency key. |

- [ ] Pin six sanitized fixture request/response pairs to the Postman operations. Preserve fields and semantics while correcting pagination gaps. Use `countryCodeV2` instead of the deprecated address country field. Keep the currently supported singular inventory-item `variant` field for source parity; record its deprecation as a future migration, not a reason to drop variant details.
- [ ] Test two products with multiple variant pages, two orders with multiple line-item pages, empty/no-match results, and repeated/missing continuation cursors. Fail a non-progressing page rather than loop forever.
- [ ] Test payment references containing quotes, backslashes, colons, and parentheses; confirm no search injection. Distinguish multiple matches from not-found.
- [ ] Test date boundary timestamps and timezone offsets, September end-exclusive boundary, cancelled/unpaid/test order inclusion, fulfillment status, refunded/removed quantities, null deleted references, and amount/currency preservation. Return records; do not invent aggregate net-sales or paid-only rules.
- [ ] Test inactive on_hand 1 is included; active on_hand 1, inactive on_hand 0/-1, and untracked items are excluded. Check every location page. Discovery makes no mutation calls.
- [ ] Test activation sends neither quantity argument, uses the supplied idempotency key, and preserves a fixture's available/on_hand values of 1. Test mutation user errors and ambiguous network failure with no automatic replay. Add equivalent user-error tests for tag replacement and empty-tag clearing.
- [ ] Run targeted tests after each function. Do not execute copied production sample order/item IDs as live mutations.

### 4. Verify and document the integration boundary

**Files:** update `docs/architecture.md` with a compact client-dependency table/diagram, `docs/development.md` with usage and semantics, and `docs/verification.md` with actual results and limitations. Keep documentation aligned with the six function signatures.

- [ ] Run the validation commands below, review diff for credentials and unrelated Postman changes, and record the actual results separately from architectural guidance.
- [ ] Inspect connected source paths and error conversions, not only the final diff. Confirm lib.rs contains declarations/re-exports only and tests mirror source paths under `src/tests/`.
- [ ] Use native DTO/contract tests and loopback transport integration tests only. Do not add or run E2E for this change. Record that real Shopify behavior is not exercised; existing unrelated E2E infrastructure remains unchanged.

## Validation

| Command/check | Expected evidence |
| --- | --- |
| `cargo test --locked -p api --lib client` | Transport, Shopify contracts, pagination, and safe mutation serialization pass. |
| `cargo test --locked -p api --lib models::shopify::tests` | Shopify DTO fixtures decode with currency and nullability intact. |
| `cargo test --locked -p api --lib config::tests` | Existing config and optional Shopify credential isolation pass. |
| `just --justfile services/api/justfile check` | Existing format and strict Clippy checks pass. |
| `cargo test --locked -p api --lib` | All native API library tests pass without Shopify credentials; no E2E target or Compose execution. |
| `cargo fmt --all -- --check` | Workspace formatting passes. |
| Diff inspection | No secrets, unintended Postman staging, migrations, routes, or shared-domain changes. |

Planning evidence only: repository and Postman files were read; no build/tests or live Shopify calls were run. No E2E testing is required for this change per review. Local HTTP fixtures are transport integration tests, not Shopify E2E. The existing E2E recipes and CI remain unchanged; CI may still execute existing repository checks automatically.

## Risks/Decisions Needed

| Decision | Proposed resolution for review |
| --- | --- |
| Pagination API: one page or complete results? | Confirmed by review: the Shopify client fetches all pages itself, including nested variant/line-item pages, and returns fully collected results. Callers never manage cursors. Private helpers manage cursors. This avoids exposing the Postman truncation bugs to callers; large catalogs use memory proportional to results. Streaming/bulk export is deferred. |
| Should sales exclude cancelled/unpaid orders or aggregate quantities? | Confirmed by review: list every order matching the date range, including test, cancelled, and unpaid orders, and return statuses/test flag plus both line quantity fields. Upstream services own all status filtering and calculation. These are order-created-date results, not event-date refunds. |
| Configuration/auth | Confirmed by review: `[client.shopify]` contains only the full `admin_api_url`; load `SHOPIFY_ACCESS_TOKEN` in `config.rs` from the existing ignored `.env` workflow and pass it to ShopifyClient as typed runtime configuration. ShopifyClient performs no environment access. This issue expressly adds a Shopify secret environment variable beyond existing DB/JWT secrets. No global startup requirement. |
| Inventory semantics | Latest chat intent supersedes the old file name “Find inventory with no quantity”: discover inactive positive-on-hand levels and activate preserving quantities. Never convert an existing number to zero. |
| Retry strategy | Review requires backoff and forbids retries on 5xx. Proposed retryable cases are explicit rate-limit rejections only (429 / data-less GraphQL THROTTLED), with four total attempts and a 30-second sleep budget. Network ambiguity is not retried. |
| Live verification | Confirmed by review: no E2E tests or E2E runs for this change. Verify with native contracts and loopback transport tests; real Shopify compatibility remains an explicit coverage limit. |
| Execution/publication authority | Approved in chat; subsequent explicit instruction to proceed under act-mkt-work authorizes implementation and publication of the approved scope. |
| Dirty source checkout | Confirmed by review: carry all current Postman changes into the new worktree, not only the six Shopify requests. Apply the tracked binary diff for `.postman/` and `postman/` plus copy untracked files, preserve deletions, verify parity, and leave the original checkout intact. Copy local secrets only as necessary with restrictive permissions; never stage/publish secrets. |

If a proposed resolution is wrong, revise this artifact at the same review gate before any implementation.

## Publications Contract/Metadata

| Field | Value |
| --- | --- |
| Issue | ENZ-5; Linear UUID `55d2e587-72a3-4ee6-9f9c-76002c0e9b64` |
| Repository/host | `git@github.com:enzumeii/monolith.git`; GitHub |
| Observed branch/base | `main`, tracking `origin/main`; HEAD `4969cb337ed6ed24e5942f4c338b8713ab1e10ae` |
| Remote default ref | Local symbolic ref `refs/remotes/origin/HEAD -> refs/remotes/origin/main`; not freshly fetched |
| Future source branch | Proposed `feat/ENZ-5`; Linear's suggested `wichayutdew/enz-5-shopify-gql-client` is metadata, not an instruction |
| Future PR target | `main`; refresh base before implementation |
| Proposed commit/PR title | `feat(api): add typed Shopify GraphQL client` |
| Current authorization | Implement, verify, commit, push, and open the approved draft PR; no Linear writes or live Shopify mutations |
| Future publication policy | After explicit implementation/publication authorization: isolated worktree, scoped staging, non-force push, draft PR, attach PR to chat, inspect required CI; do not merge |
| Description template | No repository PR template found under `.github`. If publication is later authorized, follow the skill's no-template create/read-back policy rather than inventing a template. |
| Required future CI | Existing `.github/workflows/ci.yml` format and API checks/tests, plus any branch-protection checks verified at publication time |

## Execution appendix (machine-readable)

```yaml
schema_version: 1
issue: ENZ-5
issue_uuid: 55d2e587-72a3-4ee6-9f9c-76002c0e9b64
issue_url: https://linear.app/wichayutdew/issue/ENZ-5/shopify-gql-client
retrieval: linear_mcp_get_issue_and_list_comments
retrieved_on: '2026-10-16'
issue_updated_at: '2026-10-16T11:53:24.549Z'
issue_status: In Progress
issue_comments: []
issue_relations: {blocks: [], blockedBy: [], relatedTo: [], duplicateOf: null}
jira_record: null
invoking_request: 'create a plan for https://linear.app/wichayutdew/issue/ENZ-5/shopify-gql-client'
repository: /Users/wphongphanpa/repositories/personal/monolith
base_head: 4969cb337ed6ed24e5942f4c338b8713ab1e10ae
proposed_branch: feat/ENZ-5
target_branch: main
api_version: '2026-10'
shopify_config_section: client.shopify
shopify_config_fields: [admin_api_url]
shopify_token_source: '.env:SHOPIFY_ACCESS_TOKEN'
shopify_token_loader: services/api/src/config.rs
client_environment_access: false
retryable_http_statuses: [429]
retryable_graphql_codes: [THROTTLED]
retry_http_5xx: false
max_attempts_per_request: 4
backoff_seconds: [1, 2, 4]
retry_jitter_ms: [0, 250]
max_retry_sleep_seconds: 30
fetch_all_pages_in_client: true
order_business_filtering: upstream_service
e2e_required: false
carry_postman_changes_to_worktree: all_tracked_and_untracked
source_postman_directory: postman/collections/external/shopify
source_operations:
  - '1. List all products and variants.request.yaml'
  - '2. transaction search by payment ref.request.yaml'
  - '3. Monthly Sold report.request.yaml'
  - '4. ReplaceOrderTags.request.yaml'
  - '5.1 Find inventory with no quantity.request.yaml'
  - '5.2 activate variant.request.yaml'
plan_review_required: true
implementation_authorized: true
publication_authorized: true
shopify_mutations_authorized: false
execution_method: native
intake_outcome: ready
plan_outcome: approved
next_route: prepare_workspace_implement_verify_publish
```

Original Linear description (verbatim):

```text
# Description

Integrate to shopify GraphQL [https://shopify.dev/docs/api/admin-graphql/2026-10](<https://shopify.dev/docs/api/admin-graphql/2026-10>)

# Things to do

* Create Generic rest client and extend another client to accept GraphQL request on `/Users/wphongphanpa/repositories/personal/monolith/services/api/src/client.rs`
* Integrate with shopify GraphQL with authentication under `/Users/wphongphanpa/repositories/personal/monolith/services/api/src/client/shopify.rs`
  * The shopify token should be part of environment variable
* Look into postman `external/shopify` directory and create 6 functions that called to shopifyGql accordingly
* the client request/response should have shopify DTO object in `/Users/wphongphanpa/repositories/personal/monolith/services/api/src/models/shopify.rs`

# Acceptance Criteria
```

Official references checked while planning:

- https://shopify.dev/docs/api/admin-graphql/2026-10
- https://shopify.dev/docs/api/admin-graphql/latest/objects/InventoryItem
- https://shopify.dev/docs/api/admin-graphql/latest/objects/MailingAddress
- https://shopify.dev/docs/apps/build/orders-fulfillment/inventory-management-apps/manage-quantities-states
- https://shopify.dev/docs/api/admin-graphql/latest/queries/products
- https://shopify.dev/docs/api/admin-graphql/latest/objects/ProductVariantContextualPricing
