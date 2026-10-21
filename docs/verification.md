# Verification

Results describe this revision's exercised scope, separately from [architecture](architecture.md) and [operations](development.md). They do not establish live Shopify behavior or production capacity.

## ENZ-26 — 2026-10-20 / 21

| Local evidence | Result |
| --- | --- |
| CI tooling | Root just check passed: actionlint, shellcheck, Bash syntax, 60 Node policy/release/report/helper tests; pinned reference-parser dependency audit reports zero vulnerabilities |
| Rust checks | After incorporating remote accounting changes: workspace formatting and all four module check recipes passed; corrected nine incoming test-only domain lint errors; all11 domain unit/contract tests passed |
| API coverage command | 200 native tests + 97 TCP endpoint tests + 2 controlled teardown tests passed; isolated Compose enz26 on port15433 removed with volumes |
| Backoffice coverage command | 53 native tests and 9 shared-domain tests passed; uninstrumented domain WASM compilation passed |
| Domain coverage command | 1 native unit and 8 contract tests passed |
| Client UI coverage command | Builds successfully; no tests currently exist |
| API LCOV | 53 production files; 3220/3323 executable lines hit; production main.rs has nonzero hits from real TCP child execution |
| Backoffice LCOV | 34 production files; 734/1501 executable lines hit |
| Domain LCOV | 2 production files; 7/12 executable lines hit |
| Client UI LCOV | 1 production file; 0/3 executable lines hit |
| Report validation | Full four-module sequence with shared target passed at 45f2d1d; all four reports survived later cleans and retained owning-crate production paths; failed runs remove stale reports |
| Manual command checks | Valid publication title accepted; invalid coverage option rejected with exit2 before tests; each final module coverage command completed |
| Independent review | Release-token finding fixed with trusted-main trigger; follow-up parser separator/breaking-marker findings reproduced with failing tests then corrected; final 60-test suite passed |
| Release scope | Controlled GitHub API fixtures cover Conventional Commits type/breaking-footer rules, no-bump types, dry run, retry/checkpoint, target conflicts and partial failure; no live release was created |
| Hosted evidence | Confirm published head, CI and Codecov uploads separately in PR checks; this table records local evidence only |

The local coverage counts above use pre-accounting-merge source. The branch subsequently incorporated the remote accounting merge; its full suite and coverage are verified by exact-head hosted CI separately. Native line coverage excludes browser/WASM runtime and doctests. The shared default local E2E database had an incompatible migration history from concurrent work; verification used the existing Compose service under an independent project/port without changing migrations or resetting shared data.

## Earlier recorded checks

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

## ENZ-14 order fulfillment (historical pre-remediation evidence)

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

## ENZ-14 fulfillment location batching review fix (2026-10-19)

| Check | Evidence |
| --- | --- |
| Red/green | Three new real-TCP regressions failed before the service change and passed afterward: same-location orders on one or separate pages share one creation;251 orders batch250+1; a251-line order plus a small order preserves distinct line chunks without duplicate order IDs |
| API | Strict API just check passed. Complete just test passed152 native and90 real-TCP tests with zero ignored; Compose PostgreSQL teardown completed |
| Preserved behavior | Existing separate-location, remaining-quantity, tracking, later-failure, tag-order, label/list/report and reship scenarios passed. SHIPPING is removed only after all required creations succeed |
| Postman | Internal18 and external19 requests lint with zero errors/warnings. All18 Shopify GraphQL query/variable pairs match; existing fulfillment example now shows two same-location orders |
| Limits | Controlled Shopify/Resend HTTP services and Compose PostgreSQL only; no live provider calls or mailbox-delivery proof. Required overflow batches can create multiple fulfillments/notifications. Existing partial-side-effect and ambiguous-timeout limits remain |


## ENZ-14 review remediation — local verification (2026-10-20)

Earlier ENZ-14 sections record historical evidence; this section covers the revised single-location contract.

| Check | Result / scope |
| --- | --- |
| API | `just check` passed; complete `just test` passed 157 native and 94 real-TCP tests, zero ignored; owned Compose PostgreSQL removed afterward |
| Reship | Four-digit merchant lookup reuses ListOrders with conditional email, selects exact number across pages, rejects ambiguity before writes, preserves missing-email guard, tags before one send, returns `email_id` |
| Failure paths | Controlled Shopify/Resend HTTP services exercise initial/later-page reads, tag/send errors, connection loss and deadlines; no live customer email or provider mutations |
| Fulfillment | One selling location and at most 250 order items; one combined creation, 250-line/entry boundaries, completed-order skipping before extra line reads, stop before SHIPPING removal on failure |
| Sender configuration | Central environment loading and redaction; synthetic fixtures; setup preserves existing nonempty and duplicate keys, fills empty/missing fields, and remains idempotent |
| Review regressions | Duplicate-key overwrite and old label fallback mailbox reproduced failing, then passed after fixes; fallback now shipping@enzumeii.com |
| Shared consumers | Domain 7 contract tests; backoffice 29 native tests; domain/backoffice configured WASM checks and all module strict checks passed; client test target builds with no tests |
| Postman | Internal 18 and external 18 requests lint with zero errors/warnings; all 17 Shopify query documents match executable request text and variables parse |
| Repository skills | New Rust layout skill plus configuration/ownership updates validate; metadata/links and touched test-module placement checked; no agent-behavior improvement claim |
| Limits | Live Shopify permissions/query cost, carrier acceptance and inbox delivery remain unverified. Multi-location and more-than-250-item fulfillment are outside the supplied business bounds. Published-head CI is recorded separately during delivery |

## ENZ-14 review comment revision (2026-10-20)

Earlier sections record historical behavior and test runs; this section supersedes their lookup, nested-pagination and reship-ordering descriptions.

| Area | Revised contract / verification scope |
| --- | --- |
| Shared metadata | One PageInfo type for inventory, orders and Shopify; accepts camelCase provider fields and emits unchanged snake_case public fields; API-owned cursor checks remain on Connection |
| Bounded children | Existing ShippingOrders and FulfillmentOrders include items directly at first250; shipping methods also first250. Nodes-only fixtures omit unselected pageInfo. Root list pagination remains |
| Lookup | Existing ListOrders name filter, first1 and contact projection; store naming convention supplies matching, without service scanning or ambiguity checks |
| Reship order | Lookup and email validation, send once, add RETURNED, then success/email_id. Send errors/timeout prevent tagging; tagging error can follow accepted email. No rollback or automatic send retry |
| Request validation | Fulfillment IDs reuse ShopifyId; shipping-label query parameters are ignored; four-digit bodyless/queryless reship validation stays intact |
| Configuration | Resend timeout3 in TOML; just setup is the sole environment scaffold; redundant .env.example removed |
| Limits | Controlled providers only; no live Shopify mutations, customer emails, carrier acceptance or inbox delivery checks. Item/method counts above250 and multiple selling locations remain outside agreed bounds |

| Verification | Result |
| --- | --- |
| Native API | 190 tests passed after preserving Shopify Connection camelCase serialization alongside public snake_case PageInfo |
| Shared consumers | Domain strict/native/WASM checks and 8 contracts passed; backoffice strict native/WASM checks and 29 native tests passed |
| Regression evidence | Realistic camelCase metadata and nodes-only fixtures failed before their model changes and passed afterward. Real-TCP ordering test failed with tag-before-email and passed with email-before-tag |
| Postman static checks | Internal18/external21 requests lint with zero errors or warnings; all14 remaining Shopify documents match executable query text and required variables |
| Postman execution | Saved reship and shipping-label HTTP requests ran against the real local API with controlled Shopify/Resend servers; observed lookup → email → tag. Local GraphQL request execution requires Postman login and was unavailable; no live-provider compatibility claim |
| Independent review | No remaining code correctness findings; corrected the stale inventory-alias documentation |
| Real-TCP API | Full API recipe passed all91 E2E tests with Docker PostgreSQL and controlled Shopify/Resend HTTP services; Compose teardown completed; zero ignored |

## Additional ENZ-14 comments (2026-10-20)

| Contract | Verification scope |
| --- | --- |
| Currency-independent declaration | New unit regression failed on the THB guard and passed after removing both currency checks. Covers THB/USD/EUR, price summed once, Tip exclusion, stock price/quantity distinction, minimum100 and existing invalid/nonfinite/overflow rejection |
| Listing cursor | Omitted cursor remains accepted and forwards after:null; explicit cursor and default/page-size behavior preserved; validation wording now says optional |
| Typed fulfillment | ShopifyId retained from extractor through service and fulfillment-order client; standard AsRef borrowing supports the existing tag boundary; invalid numeric ID and u64-boundary coverage retained |
| Model ownership | Provider DTOs and selected-field tests consolidated into Shopify models; duplicate resource envelope replaced by existing ResourceReference; serde hooks and root/nested metadata contracts preserved |
| Limits | Controlled providers only; no actual FX rates, live Shopify writes or customer emails. Postman GraphQL execution still requires login; static parity and Rust TCP checks cover its transport contracts |

| Check | Result |
| --- | --- |
| Full API recipe | 192 native tests and92 real-TCP tests passed, zero failed/ignored; Compose PostgreSQL teardown completed |
| Final query/model checks | Required nested shopMoney currency selection retained for MoneyBag; new cross-file query contract failed on omission and passed after restoration. All13 consolidated model tests and the focused non-THB TCP scenario passed against the final query |
| Saved HTTP requests | Listing and shipping-label requests ran against the rebuilt real local API and a controlled Shopify server; both passed |
| Static contracts | API formatting/strict lint and both Postman collection lints passed; all14 GraphQL documents match request text and variable sets |
| Review | Independent final review reports no remaining findings; shared DTO/backoffice code unchanged from the prior verified round |


## ENZ-14 review follow-up (2026-10-20)

| Area | Verified result |
| --- | --- |
| API checks | Rust/GraphQL formatting and strict all-target Clippy passed |
| API tests | 200 native tests and 97 real-TCP endpoint tests passed; Compose PostgreSQL was removed with its volume afterwards |
| Domain | One DTO unit test and eight integration contract tests passed; formatting, strict Clippy and WASM compilation passed |
| Listing response | id/number plus nullable full_name, order contact email and shipping_country_code; trimmed profile names, partial/Unicode/blank names, guest/null contacts, present/null shipping address/country and unchanged pagination |
| Listing projection | Explicit includeContact/includeTags flags; missing selected contact/root address fields and malformed selected values return safe502; no extra customer query |
| Tag edits | Stored spelling used for case-insensitive removals; existing additions and no-op edits skip mutations; unrelated tags preserved; each new read/mutation failure stage covered over TCP |
| Effect ordering | Label transitions, fulfillment-before-SHIPPING-removal and accepted-email-before-RETURNED operations retained, including later failure and connection-loss cases |
| Provider contract | Separate read-only Shopify2026-10 diagnostic reproduced omitted includeContact INVALID_VARIABLE and accepted the same search with an explicit Boolean; filter syntax unchanged |
| Postman | Internal18/external21 requests lint clean; all14 GraphQL query texts and variable sets match; saved listing and shipping-label HTTP requests passed against the real local API and controlled Shopify server |
| Review | Independent static review found no remaining actionable issues |

Endpoint tests use controlled Shopify/Resend servers and real PostgreSQL connections. The new contact/country projection and tag transitions were verified with controlled providers; no live mutations ran. Postman CLI GraphQL execution requires login and was not completed; its request passed lint/parity checks and the application query ran through the TCP tests. Published-head CI is confirmed separately after push.

## ENZ-12 accounting review remediation (2026-10-20)

| Check | Evidence |
| --- | --- |
| API | Strict API formatting/lint passed; 215 native tests and 105 real TCP E2E test functions passed with zero failures/ignored tests. Accounting test functions include operation-by-operation fault matrices. Compose PostgreSQL teardown and volume removal completed |
| Stripe settlement | THB balance amounts/fees are used once; USD100 settled as THB3600 remains THB3600. Refund signs, native-THB/null-rate behavior, unsupported settlement/fee currencies and missing conversion rates are covered |
| PayPal | At-most31-day lookback windows, independent page traversal, duplicate overlap removal, distinct same-ID events, withdrawal-rate context and inclusive payout instants across offsets/year boundaries are covered |
| Shopify matching | Refund transaction/shipping connections and originalPriceSet shipping alias match the documented schema. Created/updated discovery exhausts root pages, deduplicates order IDs and matches legacy receipt references, including the literal Stripe Card Payments gateway. A failing refund-reference regression was corrected; sale/refund TCP fixtures preserve the complete opaque PaymentSession reference. Sale/refund shipping/items, partial allocation, unmatched rows, ambiguous references and incomplete connections are covered |
| Dependency faults | Real application/client sockets exercise Stripe payout/balance/charge/refund, PayPal OAuth/reporting and Shopify first/later discovery operations. Controlled HTTP rejection, malformed/missing/invalid bodies, disconnect/refusal, timeout, exhausted throttling and GraphQL error/partial-data scenarios assert safe errors and exact call prefixes/counts |
| Contracts | Workspace formatting, whitespace checks and exact application/Postman GraphQL plus variable parity passed. Postman lint scanned40 requests with zero errors/warnings. Shared DTO/route shape is unchanged |
| Limits | Controlled provider servers and the existing Docker database were used; no live provider call, production deployment or merge. Live store scopes, requested query cost and gateway-specific receipt stability remain unverified. Requested nested connections retain the documented bounded workload and fail safely when incomplete; discovery has no upstream snapshot isolation |


## ENZ-12 native-currency report follow-up (2026-10-20)

| Check | Evidence |
| --- | --- |
| PayPal sample | Synthetic fixture preserves AUD53.08 gross, AUD50.26 net, AUD2.82 charged fee and AUD10.08 shipping included in gross. Currency is explicit; exchange_rate and bank po_date are null. No transaction-to-bank FX or payout association is inferred |
| Date contract | Raw+ and encoded+ offsets pass the same RFC3339 validation and yield identical populated reports/provider requests. PayPal filters inclusive transaction instants without FX lookback; Stripe retains inclusive payout bounds |
| Wire/order contract | transactions has stripe/paypal arrays, including empty providers. Transactions are stably sorted ascending by payout then transaction instant, null payouts last; accounting items are stably sorted by transaction instant. Different timezone offsets are compared as instants |
| Monetary cases | Signed charged/returned fees, zero/missing shipping, signed refund shipping, provider shipping precedence, compatible Shopify fallback, mixed-currency rejection and supported currency precision are covered. Unknown PayPal shipping is null; provider-fee VAT is not inferred from merchant tax |
| Pagination/errors | Existing controlled-provider TCP fault matrices retained. A reportable payment repeated at the shared boundary of two31-day windows is emitted once with unchanged gross/net/shipping; pages reset and exhaust per window |
| Verification | 222 native API tests and109 real TCP E2E test functions passed, zero failures/ignored; Compose teardown completed. Strict API check, domain3 tests, Postman40 requests with zero errors/warnings, workspace formatting, whitespace and GraphQL/Postman parity passed |
| Limits | Controlled providers and real Compose PostgreSQL connections, no live-account request or mutation. The supplied payment fields are reproduced using synthetic identifiers without personal data. An all-THB PayPal report and actual bank payout reconciliation require additional authoritative data; consumers must respect per-row currencies. The actual erroneous client response was not supplied, so the separate gross/net mismatch was not independently reproduced |

## ENZ-12 protocol and helper review follow-up (2026-10-20)

| Check | Evidence |
| --- | --- |
| API regression | Final native API run:245 passed, zero failed/ignored. Existing full recipe:109 real-TCP E2E cases passed against controlled providers and the owned Compose PostgreSQL instance; teardown removed its container/network/volume. Two final pagination tests were also included in the final native run |
| Shared behavior | Optional provider accessors reuse AppState clients and retain initialization errors. Date/number utilities preserve parsing, units and fallback behavior. Shared numbered and ID-cursor drivers retain page ordering, metadata/progress guards and stop-on-error behavior; nine pagination tests pass |
| Provider enums | Exact known wire round trips, unknown PayPal event/currency identities, supported native precision, descriptions/refunds, fee/shipping compatibility and unknown Stripe kind/fee handling are covered. Unknown values do not acquire fabricated precision or FX |
| Shopify reuse | Accounting and shipping share SKU classification and ASCII phone extraction while retaining feature-owned labels/priorities, numeric/string phone policies, address formatting and romanization. Character-based SKU extraction handles non-ASCII prefixes. Named sale-allocation fields preserve existing rules |
| Checks | Strict API lint/formatting, workspace Rust formatting and whitespace checks pass. Domain tests:8 passed; domain WASM compiles. The separate domain strict-lint recipe reports9 existing test-fixture violations in unchanged crates/domain/src/tests/dto/accounting.rs (literal formatting and unwrap usage); these are outside this refactor |
| Postman | Committed snapshot:40 requests scanned with zero errors/warnings. The user's local transaction-request edits and generated folder metadata were preserved byte-for-byte and excluded from staging |
| Repository guidance | monolith-protocol-enums passes skill structural validation. Independent baseline and guided scenarios both preserve unknown identities/error policy and keep customer memo free text; the strict-status negative control remains rejecting. Baseline was already correct, so no behavioral improvement is claimed |
| Review and limits | Independent scoped review found no actionable findings. No live provider requests, production deployment or merge. Exact pushed-head CI and native thread replies are confirmed separately after publication |


## ENZ-24 admin accounting page (2026-10-21)

| Check | Evidence |
| --- | --- |
| Local checks | Backoffice just check passed native and WASM strict lint/formatting. Just test passed 63 backoffice tests and 11 domain unit/contract tests, plus the domain WASM check. Just build produced the browser bundle |
| Accounting contract | All 20 transaction fields and all 11 item fields are projected without recalculation. Unit tests cover date validation/seconds/+07:00, query encoding, nulls, signed refunds, exact large integer IDs, mixed currencies and CSV escaping/order |
| Lifecycle | Controlled Dioxus action tests cover idle, submit/retry, filter invalidation, cancellation/late responses after logout or principal changes, and same-principal refresh without replay |
| Browser | Real localhost UI and synthetic HTTP fixture responses verified separate Stripe (2), PayPal (1) and Items (1) tables, safe provider error/retry, all empty states, invalid ranges, cleared results after edits, and disabled export during loading/error/empty results |
| Downloads | Actual downloaded accounting CSV parsed as 3 rows and 20 columns; Stripe then PayPal order, null cells, negative refund, full u64 ID and Unicode/quote/newline text verified. Existing Orders Best XLSX download remained valid and retained leading-zero strings without formula cells |
| Presentation | Dark desktop 1440px and mobile 390px layouts checked; navigation wraps and table overflow stays inside focusable regions. Light presentation checked through a fixture-only stylesheet preview disabling the dark media block; OS color preference was not changed |
| Limits | No live provider/production data, deployment, or backend changes. Existing API TCP E2E/Postman suites were not rerun for this frontend change. Browser fixtures and downloaded files are excluded from Git. Exact pushed-head CI is confirmed separately after publication |


## ENZ-24 consolidated item export (2026-10-21)

| Check | Evidence |
| --- | --- |
| User-requested export | Consolidated download is accounting-report.zip containing transactions.csv (20 columns, Stripe then PayPal) and items.csv (11 columns, every backend item including sale/refund rows). Empty sections retain their CSV headers |
| Verification | Backoffice just check, just test and just build pass: 66 backoffice tests, 11 domain tests and domain WASM check. New archive tests failed before implementation, then passed for both entries, escaped text, refund rows, header-only sections and item-only export eligibility |
| Browser downloads | Actual ZIP inspected with two transaction rows and two item rows; provider order, SALE/REFUND, signed quantities, Unicode/quotes/newlines, leading-zero SKU and full u64 IDs preserved. Item-only report enables ZIP; entirely empty report disables it. Existing Orders Best XLSX downloaded and inspected after the shared DownloadFile extraction |
| Limits | Synthetic localhost HTTP responses only. ZIP uses stored entries from the already-locked zip library. Backend calculations and product report CSV/XLSX formats remain unchanged. Exact updated-head CI is confirmed separately |

## ENZ-33 initial cleanup baseline

| Area | Observed result |
| --- | --- |
| API | Existing `just test` passed 249 native tests and111 real-TCP E2E tests, zero failures/ignored; isolated Compose PostgreSQL was removed afterward |
| Backoffice / contracts | 67 backoffice native tests; domain3 unit/8 contract tests; configured domain WASM check and backoffice web build passed |
| Formatting / lint | API dprint/Rust/strict Clippy, backoffice native/WASM strict Clippy and domain check passed |
| Dependency ownership | Services, clients, repositories and middleware contain no AppState references; handlers resolve provider availability; startup/router composition retains application wiring |
| Reduced projections | Inventory/order-report minimal-response tests failed on removed required fields before the DTO change and passed afterward; surviving client/model/service tests retain pagination, completeness and failure assertions |
| Postman static checks | External18/internal19 requests lint with zero errors/warnings; all11 retained Shopify documents and variables match parsed executable requests |
| Saved GraphQL envelopes | Four changed saved queries executed as identical JSON envelopes via `postman request` with status200/data/no-errors assertions; all4 passed. Inline bodies avoid the installed CLI file-body failure |
| Saved internal requests | Inventory report, order report and accounting requests returned200 against the real local API with controlled upstreams; zero request/script failures |
| Browser / downloads | Real local API and built UI: report CSV222 bytes/14 headers, report XLSX5699 bytes/14 headers, Fastship CSV307 bytes/22 headers, each with one synthetic row; accounting ZIP872 bytes with exactly transactions.csv/items.csv, one row each; downloaded bytes inspected |
| Browser states | Loading skeleton, native picker cancellation, fallback filename/format dialog and focus restoration, empty accounting export suppression, sanitized upstream error and Retry verified |
| Limits | Native picker save not automated; fallback exercised in a disposable built copy with the native capability disabled. Native GraphQL collection execution requires Postman login; no live Shopify/schema/scope/cost, carrier acceptance, email delivery, deployment or load claim |
| Review | Initial author diff/consumer review preceded PR29 feedback; results in this table predate the review-comment fixes |


## ENZ-33 PR29 comment fixes (2026-10-21)

| Area | Observed result / remaining coverage |
| --- | --- |
| Required accounting providers | Fresh combined API `just test` passed 250 native and 111 real-TCP tests with zero failures/ignored; all 12 accounting test functions passed and isolated Compose PostgreSQL was removed. Missing/init-failed Stripe, PayPal and Shopify return provider-specific 503 with zero upstream calls; both configured providers can independently return no records |
| Shared upstream mapper | After relocation to `error.rs`, 31 error, 18 Orders, 7 Inventories and 9 pagination native tests passed; strict API check passed. New mapper regression preserves safe 502/504, typed source, tracked origins and redacted diagnostics |
| XLSX extraction / consumers | After unchanged encoder/row-limit relocation, backoffice `just check`, `just test` and `just build` passed: 67 backoffice tests and domain 3 unit/8 contract tests; native/WASM strict Clippy, domain WASM and browser bundle passed. Generic text-cell/row-limit tests reuse the existing XML fixture; Reports, all courier mappings and Accounting export suites remain covered |
| Static scope | Domain `just check`, workspace formatting and patch whitespace checks passed; shared DTOs and feature-specific export mappings are unchanged |
| Browser accounting | Real built UI and real API with controlled Stripe/PayPal/Shopify: populated Stripe1/PayPal0/Items1, all-empty export disabled, sanitized failure/Retry, loading/export disabled. Actual ZIP872 bytes has exactly transactions.csv/items.csv,20/11 headers and one row each |
| Browser XLSX | Reports fallback Save report as, filename/format selection and restored button focus verified in a disposable capability-disabled bundle. Actual XLSX5700 bytes:14headers/one row, expected shared strings, string headers and zero formula cells |
| Saved accounting request | Actual Postman HTTP collection request ran with inherited synthetic bearer token: populated200, empty200 and missing-PayPal503; each CLI exit0/zero request failures. Separate real-HTTP assertions checked populated/empty shapes and exact safe provider error; collection itself contains no test scripts |
| Final local verification | Independent combined diff/consumer review found no gaps. Strict API check, workspace formatting, patch whitespace and Postman lint passed (19 internal/18 external, zero warnings/errors); six saved examples and all 11 external GraphQL query/variable pairs verified. CI for the final pushed source SHA is recorded separately |
| Limits | Controlled localhost providers only; no live provider compatibility, merchant scope, cost, load, email or deployment claim. Native picker save/cancellation/focus remain manual/unverified this run: permitted tab Escape did not dismiss the native dialog. Browser fallback was exercised only in a disposable capability-disabled bundle |

## ENZ-29 saved report history review remediation (2026-10-21)

| Check | Evidence |
| --- | --- |
| Local checks | Strict API, domain native/WASM and backoffice native/WASM checks passed; backoffice tests passed (66 app, 14 domain unit/contract tests) and its browser bundle built |
| API regression | 281 library tests and 120 real TCP E2E cases passed against controlled providers and Compose PostgreSQL; owned Compose services were removed afterwards |
| Audit | Independent report metadata has no cron foreign key. Generation/email states and safe errors are checkpointed eagerly; failed generation retains attempted_at and a null generated_at. History filters, pagination, authorization, file selection, missing/corrupt files and obsolete query/route rejection are covered |
| Files | Inventory CSV and transaction CSVs retain established columns, both transaction providers, currencies and items. Actual XLSX ZIP/XML checks cover named sheets and string/formula safety. Filesystem tests cover immutable bytes, partial directories, checksum errors and recovery/adoption |
| Delivery recovery | Controlled SDK HTTP verifies CSV/XLSX attachment MIME and exact saved/downloaded bytes. Publication retry does not regenerate or repeat an accepted email. PostgreSQL tests cover audit rollback, stale tokens, frozen format/cursor, lease renewal, in-flight uncertainty, final-attempt expiry and audit-commit recovery |
| Review regressions | Four new regressions were observed failing before fixes, then passed. Cancellation recovery uses the stored lease-expiry instant rather than wall-clock scheduling assumptions |
| Fixture isolation | Ordinary E2E API processes explicitly disable scheduling; worker fixtures enable it. Scheduler defaults remain enabled. Publication retry retains its second attempt behind a controlled delay, then becomes due only after the storage blocker is removed |
| CI recovery | Initial hosted runs exposed an existing reship call-count failure and then a publication-fixture race; the reship case passed on rerun. Fixture retry timing and scheduler isolation were corrected. Exact final-head CI is recorded separately |
| Postman | Internal collection lint: 28 requests, zero errors/warnings. Isolated local execution: 9 real requests and 5 assertions passed; no cloud result upload |
| Limits | Original unpublished migration was edited with explicit user authorization; no published database was migrated. Storage is an ignored local directory pending Vercel Blob configuration; retention and shared deployment storage remain deployment decisions. No manual UI test, live provider delivery, production deployment or merge was performed. Exact pushed-head CI is confirmed separately |


## ENZ-29 simplified schema and services (2026-10-21)

This contract supersedes the earlier ENZ-29 verification entry above.

| Check | Evidence |
| --- | --- |
| Schema and ownership | Unpublished migration has 9 configuration, 9 job and 8 independent audit fields. Cron runtime, services/cron/{config,report,job}.rs and separate configuration/job/audit repositories match docs/cron.md. |
| Local API | Strict check passed. Final native run passed 282 tests; the complete isolated TCP run passed 119 tests. After refining lease renewal timing, the slow-email TCP case passed again. |
| Shared and UI | Domain native/WASM and report-generator native/WASM checks passed; 14 domain and 2 shared generator tests passed. Backoffice native/WASM checks, all 66 tests and browser build passed. Client UI strict check passed. |
| Workflow tooling | All 60 tooling tests, actionlint, shellcheck and Bash syntax checks passed. The new provider-client skill passed its validator. |
| Postman | All 28 internal requests linted without errors/warnings. An isolated real API/static-host run passed 9 requests and 6 assertions; direct storage requests carry no API Authorization header. No cloud result upload. |
| Browser | Chromium exercised the actual backoffice CSV/XLSX save flow against an owned API, PostgreSQL and controlled Shopify HTTP server. Downloaded contents and filenames were inspected; XLSX has no formula cells. Accounting ZIP used a controlled API response: actual downloaded ZIP retains both CSV entries, 20/11 headers, negative refund quantity and leading-zero SKU. No page errors. |
| Safety and recovery | Tests cover eager failures, one attempt budget, expired/fenced leases, a one-second lease renewal, immutable saved bytes/mail requests, private checkpoints, publication rollback, direct URL retention after config deletion, malformed metadata and revision-based catch-up. |
| Isolation | API runs used dedicated enz29schema, enz29unit and enz29lease Compose projects/ports; owned containers and volumes were cleaned. Other worktrees were preserved. |
| Limits | Native OS file picker was disabled in browser automation; the fallback save dialog was exercised. Live provider acceptance, Blob integration, production migration and deployment remain unverified. Exact pushed-head CI is recorded separately after publication. |

## ENZ-30 scheduled reports — local verification (2026-10-22)

Local delivery on `feat/ENZ-30`, based on ENZ-29 `274eb5f8a864648cb4404a5916e687ce80aa9768` plus its copied, uncommitted API dependency changes. No commit, push, PR, tracker update or deployment was performed. The ENZ-29 source checkout was not edited.

| Check | Result |
| --- | --- |
| Backoffice `just test` | PASS: 98 backoffice tests, 9 domain unit tests, 8 domain contract tests; domain WASM check. |
| Native and WASM web `cargo clippy --all-targets -- -D warnings` | PASS after final implementation changes. |
| Backoffice `just build` | PASS; compiled Dioxus bundle exercised against real local API and PostgreSQL. |
| Existing cron real-TCP E2E modules | PASS: 2 configuration tests and 2 report-history tests, isolated Compose database, normal harness startup/teardown. |
| API `just test` | BLOCKED: 12 inherited library-test compile errors referencing moved scheduler/storage/email/validation items. No tests removed or disabled. |
| Full diagnostic API E2E attempt | INCOMPLETE: publication retry failure recorded; no final suite summary. Not a passing full-suite result. |
| Backoffice/API `just check` | BLOCKED by inherited formatting. Separate strict frontend lint passes. Scoped formatting differs only in an inherited API import block, verified identical to the copied baseline. |
| Fixed Bangkok time under `TZ=America/Los_Angeles` | PASS: 2 timestamp/range tests; fractional boundaries preserved. |
| Independent reviews | Final scoped reviews pass after fixes to defaults, conflict recovery, recurrence selection, dirty navigation and confirmation borrow lifetime. |

| Real browser exercise | Evidence/result |
| --- | --- |
| ADMIN and SUPER_ADMIN | Cookie sign-in; create, edit, enable/disable and delete; ADMIN edited SUPER_ADMIN-created schedule. All worker schedules disabled during verification. |
| Presets and preview | Previous month, first half and weekly inventory returned five Bangkok runs; half-open periods shown; field edit cleared stale preview; canonical inventory ID errors preserved input. Report-type switches visibly select the matching recurrence. |
| History | Global list, numbered pages, type/date bounds with explicit +07:00, generated/attempted fallback, nullable generation, detail/back and safe overall failures. Offset round-trip regression fixed and browser verified. |
| Draft departures | Native Back/Forward Cancel preserves draft; Confirm reaches original destination. In-app Cancel/Confirm and dirty close pass. Reload attempt retained the draft; automation did not expose a native reload-dialog object. |
| Confirmation | Cancel initially focused, keyboard navigation/Escape, trigger focus restored; existing profile confirmation opens and cancels. Native dialog at phone width inspected. |
| Responsive layout | Desktop and 390×844 override inspected. Narrow history table is 768px inside its horizontal scroll region; page width stays 375px. Temporary viewport override reset. |
| Stale responses | Mounted production Router/History/Detail with real HTTP: filter/detail/account/logout changes reject old completions; refresh retains previous data while pending and after 503, retry replaces it. Preview revisions and duplicate guards covered by tests. |
| Files and retained history | Six browser-saved files compared byte-for-byte by SHA-256: transactions.csv, items.csv, 149-byte header-only inventory, 4,984,609-byte inventory, opaque corrupt bytes and a file from FAILED email workflow. Saved history remained after schedule edits/deletes. |
| Storage/session separation | Public storage HTTP requests need no metadata credentials; missing object returns 404. Real API metadata routes use cookie sessions. Scheduler disabled and provider clients absent; metadata/file paths perform no generation or email dispatch. |

| Coverage limit | Detail |
| --- | --- |
| Browser timezone | Actual browser was Asia/Bangkok; available browser controls provide no timezone override. A non-Bangkok browser session remains unverified; fixed-offset code and non-Bangkok process tests pass. |
| Download UI | Ordinary links use original returned URLs. Browser download helper saved and verified bytes; native Save As completion was not verified. Missing-file error rendering was blocked by the browser tool's URL policy; HTTP 404 verified independently. No controlled storage-host outage or expired-metadata browser session was completed. |
| Conflict/recovery faults | 409 reload latches, stale/failed reload rejection and native history-restoration failure were tested deterministically; real-API concurrent 409 and native browser restoration rejection were not forced. |
| Screenshots | Desktop/narrow screenshots inspected in browser output; exporting screenshots to local files was denied by the read-only browser filesystem. |

Detailed logs, fixture manifest, hash evidence, task reports and the ENZ-30-only patch are retained under `.superpowers/sdd/2026-10-22-enz-30-scheduled-reports/`. Runtime credentials are private, ignored files. Only owned verification processes and Compose volumes were cleaned up.

### ENZ-30 UI refinements (2026-10-22)

| Requested change | Implementation and verification |
| --- | --- |
| More prominent navigation | Bordered, padded Schedules/Report history tabs with selected/hover/focus states; selected contrast preserved during keyboard focus. |
| Cell borders and shared table | Both scheduled-report tables now use existing `DataTable`, extended with optional caption/class for rich rows; shared cell borders cover all sides. Existing download/table consumers retain their interfaces. |
| Popup creation/editing | Shared `Popup` reuses Confirm's native-dialog lifecycle. Real API create/preview/save, asynchronous edit focus, Escape/discard cancellation, native Back guard, clean close and invoker focus restoration exercised. Desktop width 864px; phone popup 358px in 375px document. |
| Shared calendar/time UX | `DateTimePicker` in Accounting/history, `TimePicker` in schedules: month/year navigation, date grid, Today/Clear and 24-hour hour/minute selects. Six date arithmetic/precision/empty-value tests added. Browser leap day 2028-02-29 17:45, Clear, Accounting Today and real history +07:00 bounds verified. Phone calendar width 301px with no page overflow. |
| Final checks | PASS: 104 backoffice tests, 9 domain unit and 8 contract tests, domain WASM check, strict native/WASM lint, changed-file formatting, popup JavaScript syntax and browser build. Scoped review finding about popup width fixed and re-reviewed. |

Reuse decisions: extend `components/report_table.rs::DataTable` for rich schedule/history rows; extract existing Confirm bridge into `components/popup.rs`/`popup.js` instead of adding another modal lifecycle. No shared calendar existed, so `components/date_time.rs` owns the common controls and date arithmetic. API request and timestamp semantics remain with their existing features.

A synthetic preview is left running at `http://localhost:8084/scheduled-reports`, with scheduler disabled. Its API/storage/database are isolated owned fixtures. Runtime configuration, process IDs and cleanup helper are in `.superpowers/sdd/2026-10-22-enz-30-scheduled-reports/runtime-ui-refinement/`; do not treat preview data as production data. No commits, pushes or external updates.

### ENZ-30 calendar seconds (2026-10-22)

Shared calendar pickers in Accounting/history now offer Second (00–59), alongside Hour/Minute. Scheduler time controls remain HH:mm. Three regressions cover second replacement, boundary choices, precision preservation and suffix duplication. All 107 backoffice tests, formatting and strict native/WASM lint pass; browser build passes. Real browser checks: 08:15:43.1200 → second59/minute20 yields 08:20:59.1200; history URL contains :59 with +07:00, Accounting accepts second00, schedule form exposes no seconds control, and phone layout has no horizontal page overflow.

### ENZ-30 reconciliation with ENZ-29 (2026-10-22)

This checkpoint supersedes the earlier copied-dependency verification blockers. At the user's explicit request, `feat/ENZ-30` now starts at committed ENZ-29 `8c5e953ca4cac1d9efd2ea23d3955e6f8457d0b2`. ENZ-30 changes remain uncommitted. Source ENZ-29 was not modified.

| Check | Result |
| --- | --- |
| Conflict reconciliation | Complete dirty-tree snapshot retained. Independent read-only review PASS: 47 original owned paths preserved byte-for-byte, one model merge retains required owned timezone String conversion; 400 non-owned tracked files match upstream. Two owned files now equal upstream, leaving 46 changed paths. Index clean. |
| API checks and tests | `just --justfile services/api/justfile check` PASS; full `test` PASS: 297 native tests and 118 real-TCP E2E tests, zero failed/ignored. Isolated Compose project `enz30-verify-20261022` torn down with volumes. |
| Frontend checks and tests | Backoffice `check` and `test` PASS: 107 backoffice tests, 9 domain unit tests and 8 domain contract tests; native/WASM strict lint, formatting and domain WASM check pass. |
| Builds | API and actual Dioxus browser bundle rebuilt successfully. Runtime API SHA256 `653b5f882b76e5b971b8bcc7abcc599d60caabd7e49c967fdc29500592f8ab58`. |
| Rebased runtime | Fresh owned PostgreSQL volume avoids old migration checksum reuse. API 13132 / Docker Vercel Blob 15442; both roles pass real cookie/CORS, disabled-schedule CRUD, Bangkok preview, 25+7 history pages and all 32 detail checks. Scheduler disabled; zero jobs at seeding. |
| Browser on rebased API | Login, global history, two-file detail/back, popup create/preview/save pass. Newly saved browser schedule remains disabled. Prominent tabs, bordered shared tables and retained calendar/seconds UI render. Six browser-saved CSV files match seeded original SHA256 hashes, including 149-byte header-only inventory, 4,984,609-byte inventory, and both files from failed email workflow. |
| Local delivery | Patch regenerated against current HEAD and checked using a separate index. No commits, pushes, PRs, tracker changes or deployments. |

Earlier browser timezone, native Save As and controlled fault coverage limits still apply; no claim of complete browser acceptance. Reconciliation does not change those UI implementations. Logs: `/tmp/enz-30-rebased-{api-check,api-test,api-build,ui-check,ui-test,ui-build}.log`. Snapshots and independent review evidence: `.superpowers/sdd/2026-10-22-enz-30-scheduled-reports/reconcile-enz29-20261022/`.

The preview at `http://localhost:8084/scheduled-reports` now uses the rebased synthetic API/Blob runtime. Its private helper, process identities, credentials, fixture metadata and browser download hashes are under `runtime-rebased-20261022/`. The previous API/storage processes and old Compose project were stopped after ownership checks. Current preview remains running intentionally; provider integrations and scheduler remain disabled.
