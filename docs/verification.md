# Verification

Results describe this revision's exercised scope, separately from [architecture](architecture.md) and [operations](development.md). They do not establish live Shopify behavior or production capacity.

## Checks

| Evidence | Result |
| --- | --- |
| Native API tests | 129 passed with Docker PostgreSQL ready; zero ignored; each library test runs once |
| Docker PostgreSQL | 129 library tests and 45 real TCP E2E tests passed; Compose teardown completed |
| Formatting/lint | Workspace Rust formatting, strict API/domain/downstream checks and Cargo-installed GraphQL/JSON formatting passed |
| Shared contracts / WASM | 5 domain contracts passed; domain WASM and backoffice native/WASM checks passed |
| Downstream tests | Backoffice 6 tests passed; client-ui check/test passed with no tests |
| Postman | All 9 external GraphQL documents match their requests and variable sets; external9/internal-v1(11) lint passed with zero errors/warnings |
| Formatter behavior | Cargo-installed dprint 0.63.0 with pinned Rust-based plugins checks 9 GraphQL and 12 JSON files. Dirty GraphQL/JSON rejected; write-format corrected both; repeated writes identical. Malformed input rejected unchanged; service-local tool/cache files excluded. SQLx JSON values and GraphQL tokens unchanged; Postman queries synchronized. Existing just fix passed against Docker PostgreSQL, followed by just check |
| Split-contract regressions | Reproduced inventory overfetch and dropped page_info; inventory returns one page and orders retain full totals. Later-page order overflow/invalid amounts reject the summary |
| Model cleanup | Removed unreferenced `SalesOrder`; API error encoding and backoffice decoding use shared `ErrorResponse`. Existing error/status/header regressions and the complete API suite passed; frontend native/WASM checks passed |
| Error simplification | Shared status/message error preserves typed causes, first origin, ordered contexts and sanitized diagnostics, including wrapped/shared client causes. Exact missing-location404 and missing-admin GET/PATCH/DELETE401 bodies and Bearer headers passed; timeout504/non-timeout502 exercises the production mapper. API check, workspace formatting and internal Postman lint passed |
| Automatic error locations | Constructors and all typed conversions capture caller file/line/column; async question-mark conversion, callback mapping, builder/propagation preservation, password blocking work, repository failures and terminal background logging passed. Production upstream mapper retains its caller, order failures retain the service mapping site, and direct/shared/wrapped client origins remain separate. HTTP bodies/headers and ordinary/DEBUG redaction regressions passed |
| Automatic client locations | ClientError constructors, tracked helpers, reqwest/serde conversions and async question-mark conversion retain caller positions and original source types. Protocol/model failures retain their construction file; propagation preserves that first location. All eight failure kinds and direct/shared/context-wrapped causes retain safe boundary logs, client_origin=file:line:column and unchanged public responses; no manual origin wrappers |
| Error log formatting | Production text formatter renders captured file/line/column and bracketed HTTP status in the prefix; 400/401/404/500 and default event/span formatting regressions passed. Structured origin/status and private DEBUG diagnostics remain available; no duplicate origin/status text fields |
| Published-head CI | Confirm separately against the pushed SHA in PR checks |

Product E2E uses real API TCP endpoints and Docker PostgreSQL with the explicitly approved synthetic Shopify HTTP upstream. No live Shopify requests or mutations ran.

## Exercised contracts

| Area | Evidence |
| --- | --- |
| Client transport | Typed JSON/GraphQL decoding, error/partial-data handling, timeout, redirect rejection and original failure causes |
| Request timeout | TOML-only client.shopify.timeout_secs=3; zero rejected. Initial/later inventory and order HTTP failures return504 after at least3 and under8 seconds, without replaying timed-out calls. Existing 1/2/4-second exponential retry and shared attempt-budget tests passed |
| Retry policy | HTTP/GraphQL rejection shares a four-attempt budget; unchanged replay payloads, Retry-After bounds and no ambiguous timeout/5xx replay |
| Shopify proxy boundary | Explicit size/cursor forwarded; one operation per method; returned root metadata without automatic traversal; bounded variants/lines use fixed250; raw inventory/mutation outputs retained |
| Source models | Inventory and tagged-line conversion, SKU Unicode characters4–9, short/missing SKU, active levels, zero/negative stock and nullable metadata |
| Report values | Native f64 parsing/multiplication, totals rounded to two decimal places, zero/negative quantities, scientific weights, compatible metadata comparison, checked quantity overflow and rejection of invalid/non-finite results |
| Service pagination | Inventory forwards size/cursor and preserves one page; orders consume each root page before fetching the next. Missing/repeated/cyclic cursors and later failures reject the order report |
| API pagination | Inventory first and continuation HTTP calls return page_info, including empty pages; order report rejects pagination and keeps products-only global totals |
| Audience routing | Parent v1 protected route group authenticates /v1/admin once; existing auth/admin-account routes remain covered. Dedicated legacy/customer report-path rejection tests are not part of the current suite |
| Tagged orders | Comma-separated tags trimmed and combined with AND; blank entries rejected400 without upstream calls. Real TCP traversal preserves both tag filters and batch250 across pages and aggregates report totals. Literal escaping, bounded nested lines, no child request, compatible rows merged in first-seen order and missing SKUs kept separate |
| Dependency lifecycle | Shared AppState client Arc, optional/invalid initialization retained; services resolve availability and map affected requests to sanitized503 |
| Report failures | Real TCP400/401/404/502/503/504 scenarios; configured upstream receives no invalid/unauthenticated requests. Initial/later operations cover HTTP/GraphQL rejection, partial/missing/malformed data, exhausted throttling, connection refusal and actual response timeouts; exact safe bodies/headers (including Upstream request failed for502/504), paging variables and request counts asserted. Ordinary logs/public bodies sanitized, original causes available through DEBUG diagnostics |
| Existing API | Authentication, authorization, cookies/origins/CORS, refresh-family transactions, database operations and registered routes remain covered by the complete suite |

## Limits

| Area | Limit |
| --- | --- |
| Retrieval | Inventory: one caller-sized root product page (default50), ordered by total inventory descending across all locations; size counts products, not flattened rows. Location filtering can still produce empty pages. Fixed250 variants per product, no variant traversal. Restart cursors after an ordering change. Orders: exhaust batches250 and return one summary; no snapshot isolation |
| Nested workload bound | At most250 variants/order lines per parent is the agreed store assumption; nested hasNextPage=true returns502, preventing silent truncation |
| Live Shopify | Schema compatibility, store permissions and actual requested query cost were not executed against Shopify |
| Concurrent changes | No snapshot isolation; traversal/incomplete nested-data failures are rejected, but complete reports can reflect changing store data across Shopify pages |
| Performance | SKU indexing narrows comparisons; many incompatible groups for one SKU still scan that SKU's candidates; no load/capacity benchmark |
| Browser/deployment | No new rendered browser E2E, production deployment or merge |
| Coverage percentage | Not measured |

## ENZ-14 order fulfillment

| Area | Verification |
| --- | --- |
| Shipping labels | POST `/v1/admin/orders/shipping_label`; all open/paid/unfulfilled PACKING pages, four carrier outputs, country redirects, SKU category/declaration rules, transliteration, Thai Post service selection and Teleport kilograms. Add SHIPPING/courier, then remove PACKING; no date tags |
| Order listing | GET `/v1/admin/orders?tag=SHIPPING,FASTSHIP`; AND tag filters, fulfillment filter, default/boundary page sizes, opaque cursor forwarding, numeric order IDs and upstream page metadata |
| Fulfillment | POST numeric order ID; remaining eligible quantities grouped by location, tracking-pattern precedence and URL encoding, customer notification enabled; remove SHIPPING after all fulfillment calls succeed |
| TCP dependencies | Production API/middleware/client against controlled Shopify HTTP servers; Compose PostgreSQL and uniquely owned admin fixtures. Each reachable operation covers malformed/partial/missing responses, HTTP/GraphQL rejection, throttling and timeout; later-call failures assert no unintended subsequent calls |
| Errors and validation | Missing/invalid/expired JWTs; invalid paths/query/JSON/tracking, unsupported actions and missing order; safe 400/401/404/409/502/503/504 responses. ADMIN and SUPER_ADMIN are the only current roles; these stateless JWT routes perform no database lookup |
| Native regression | After integrating main’s Resend SDK:147 native API tests and66 real-TCP tests passed, with Compose teardown. Domain/backoffice/client native and configured WASM checks/tests passed; API formatting/lint and Postman lint passed |
| Postman | Both collections lint without diagnostics; nine new GraphQL documents and variable sets match executable requests exactly |

| Limit | Behavior |
| --- | --- |
| External side effects | Tag and fulfillment mutations cannot roll back. A later-order label failure may leave earlier orders without PACKING; later fulfillment/tag failure may leave completed fulfillments. Inspect Shopify before retrying ambiguous failures |
| Concurrent batches | No durable label replay, snapshot isolation or exactly-once guarantee; simultaneous label requests can select overlapping orders |
| Nested connections | Shipping methods, order items and fulfillment items traverse every nested cursor before writes. Missing/repeated/cyclic cursors fail502. Fulfillment line arrays are chunked to250 items per mutation |
| Providers and addresses | No live Shopify/carrier mutations; store scopes, query costs and carrier acceptance unverified. Rust Japanese transliteration uses Kakasi rather than Python Cutlet; dictionary/name readings can differ |
| Reship notice | POST `/v1/admin/orders/{order_id}/reship`: positive Shopify ID, no query/body; minimal order contact lookup, additive RETURNED, then shared Resend SDK send from shipping@enzumeii.com. Every explicit POST sends; no automatic email retries or tag-based suppression |
| Reship TCP coverage | Nine scenario groups cover both roles, repeat calls, invalid ID/query/body, auth/cookie origin, missing/unusable contacts, optional dependency/config503, lookup/tag HTTP/GraphQL/timeout/decode failures, refused/disconnected connections at all stages, SDK401/422/429/5xx and unusable IDs. Public bodies and ordinary logs stay sanitized; sending stops after earlier failures |
| Reship deadline | TOML-only reship.email_timeout_secs=10; missing/zero makes only reship unavailable503. Actual controlled stalled send returns504 within the configured one-second test deadline; original elapsed cause retained |
| Final reship regression |152 native API tests and87 real-TCP E2E tests passed with Docker PostgreSQL and controlled Shopify/Resend HTTP services. Compose teardown completed. API/domain/backoffice/client checks/tests and configured WASM checks passed;17 internal/20 external Postman requests lint cleanly; all19 Shopify query/variable pairs match; four reship response examples parse |
| Reviewed transport faults | An additional20 real-TCP scenarios exercise refused/disconnected connections after successful prefixes at label/list/fulfillment/nested-read operations; safe502 and exact received request counts prevent unintended calls/replay. Independent whole-branch review found this coverage gap; no production defect was identified |
| Shared query reuse | Reship reuses `sales_order_lines.graphql` with includeLines=false/includeContact=true. Default includeLines=true/includeContact=false preserves existing report selection and DTOs. The separate reship document and Postman request are removed; existing report-client and real-TCP reship regressions pass |
| Main-merge compatibility | Shipping-label pagination now supplies the identity error mapper required by the shared paginator, retaining the existing502/504 conversion. Four stale order-test503 expectations now match main's intentional Shopify client unavailable message |
| Email partial effects | RETURNED remains if sending fails; timeout can occur after acceptance. HTTP200 means provider acceptance only. No rollback, durable replay, idempotency key or exactly-once guarantee |
| Email provider limits | No live customer email, sender-domain configuration or mailbox-delivery check. Older returned orders may need Shopify read_all_orders beyond the default60-day window. Legacy email wording retained, including85% item refund and shipping-fee exclusions; endpoint performs no refund or replacement shipment |
| Scope | No production deployment, merge or browser changes |
## Inventory enablement (ENZ-22, 2026-10-19)

| Check | Evidence |
| --- | --- |
| Red/green | New response contract failed before DTO implementation; TCP enablement request failed404 before route implementation and then passed200; zero upstream item ID failed the boundary test before validation was tightened |
| API | just check and the complete just test suite passed; additional zero/overflow ID and omitted-location-key regressions passed. Final commit counts and pipeline verification are recorded in the PR |
| Shared DTO | Domain check (native strict lint and WASM),6 serialization tests, and no-default-features compilation passed |
| Dependencies | Existing Compose PostgreSQL, real Axum child processes and controlled Shopify HTTP/TCP servers; fixture-owned rows and temporary configurations cleaned up; Compose teardown completed |
| Eligibility and completeness | Tracked inactive items with positive on_hand; active/untracked/nonpositive items skipped; batches250, empty intermediate pages, deduplication, conflicting entries and missing/repeated/cyclic cursors |
| Outcomes | Per-variant counts, repeated product_id errors, zero candidates, missing/disappearing location, rejected/null/inactive mutation outcomes and later successful independent items |
| Failure matrix | Location, first/later discovery and activation cover HTTP/GraphQL rejection, partial/missing/malformed data, exhausted throttling, real connection loss and timeouts; discovery failures never mutate; mutation failures are counted safely |
| Authentication | Missing/malformed/expired bearer rejection, ADMIN/SUPER_ADMIN success, real PostgreSQL-backed cookie login, valid origin success and missing/foreign origin rejection. Existing JWT validation has no database lookup or immediate revocation check |
| Review regressions | Malformed entries use distinct item IDs so duplicate detection cannot mask quantity/ID validation. A temporary missing-quantity validator mutation originally made the corrected TCP test fail200 vs expected502. Current eligibility requires on_hand; unused available values no longer block activation. Missing discovery location key fails decoding, while explicit null retains404 |
| Stock and retry | No onHand/available mutation inputs; eligibility reads on_hand; stable mutation UUID/payload across retry, successful throttle recovery and exhausted retries |
| Postman | Internal13 and external9 requests lint with zero errors/warnings; YAML parses and formatted activation query/variable names match application source |
| Limits | No live Shopify execution, merchant scope verification, load benchmark or deployment; live provider defaults for omitted quantities are unverified, and timed-out mutations can have unconfirmed outcomes |

## ENZ-22 review remediation (2026-10-19)

| Check | Evidence |
| --- | --- |
| API | Strict `just check` passed; complete `just test` passed133 library tests and56 real TCP E2E tests, followed by Compose teardown |
| Shared consumers | Accessor identity/caller503/source retention; pagination cursor errors502, resource404 and consumer400; existing order-report regressions passed |
| Activation inputs | Retry payload has exactly three variables; no onHand/available arguments or variables. Positive on_hand remains eligibility data; missing or large unused available values do not block activation |
| Repository skill | Skill validator and UI metadata checks passed. Five controlled evaluations chose reuse/enhancement or justified security separation; five baseline controls also did so, so no measured improvement is claimed |
| Review | Independent review found no actionable findings; final-head hosted CI is recorded in the PR |

## ENZ-22 additional review remediation (2026-10-19)

| Check | Evidence |
| --- | --- |
| Shared inventory validation | Both extractors cover positive IDs, preserved leading zeros, empty/zero/signed/decimal/whitespace/Unicode rejection and endpoint-specific query rules |
| Shared client availability | All three callers use one accessor with no message parameter; absent/failed initialization returns503 `Shopify client unavailable`, with initialization causes retained privately |
| Local verification | Strict API `just check` and complete `just test` passed134 library tests and56 TCP E2E tests; Compose teardown completed. Workspace formatting/diff checks passed |
| Postman and review | Changed YAML parses; internal13 requests lint with0 errors/warnings. Independent read-only review found no actionable issues |
| Limits | Final-head hosted CI is recorded in the PR; no live Shopify calls or deployment |
