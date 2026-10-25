# Verification

## PR #66 service reviewability — integrated-main verification (2026-10-24)

Approved plan version 2 integrates reviewed main `d677fc828e8fee810f79537895d170869e592a82` into pushed refactor `74c7784c464d661cd0a754283c2626857e3f15fa`. The ignored `complexity/base-refresh/source-freeze-tested.json` records 182 changed path states, unchanged through the complete recipe. Only this verification entry follows the run. Earlier verification sections retain their original source limits.

| Check | Actual result |
| --- | --- |
| Integration | Six conflicts resolved: union checkout/files declarations, exact Shopify query-source parity, main Stripe fixtures plus checkout expiry, and preserved deletion of the unused expire request. Main validation/upload ownership and PR readability skill guidance retained |
| Strict API | Exit 0: 34 setup-helper tests, dprint, Rust formatting, strict all-target and E2E Clippy. An extra invocation incorrectly requested a nonexistent integration feature and stopped before tests; corrected to the registered E2E target, exit 0 |
| Full API recipe | Exit 0: 495 native tests (22.46s), 226 real-HTTP tests (820.71s), zero failed or ignored; real Compose PostgreSQL and controlled providers |
| Domain and root | Domain formatting, strict Clippy, WASM check and tests pass (21 unit + 8 integration, zero failed/ignored). Root check passes all 111 tooling tests, workflow/shell lint and Vercel configuration checks. Initial root run lacked the pinned parser dependency; documented just setup installed it before the successful full rerun |
| Independent integrated review | Both gateway paths and precise completion checkpoint traced from code. All 47 checked checkout-critical source/test paths match the refactor; 186 checked inherited UI/shared-UI/CI/scripts/Cargo/Compose paths match main. Original-draft readback and ReconcileOnly guard remain intact |
| Skills | Fresh read-only focused scenarios on both merged skills cover pure formatting, uncertain capture, validation/upload boundaries, hidden helper decisions and avoiding cosmetic runtime layers. All pass; successful independent tracing is general, lost-response tracing applies to uncertain external effects. Earlier broader scenario results remain historical evidence |
| Postman | Workspace lint: 87 entities, zero errors, one existing cron-preview FMT209 warning. Exact resolved Shopify query parity; no provider request execution |
| Whitespace and cleanup | Diff versus reviewed main passes. Old-parent diff flags inherited Postman space-before-tab indentation; main content preserved. Owned enz48 containers/network and recorded PostgreSQL/S3 volumes removed; no shared data reset |
| Evidence and limits | Ignored complexity/base-refresh/ retains approved plan, logs, hashes, independent review and cleanup audit. Bounded integration review does not replace full functional review of PR #58. No complete browser purchase, live provider mutation/callback/email, deployment or external exactly-once claim. Hosted new-source checks remain the delivery gate |

## PR #66 service reviewability — local verification (2026-10-24)

Results bind to approved service-reviewability plan version 1 atop `74b2bb83b1d3ea94d78b5047dde88233f4f32b5d`. The ignored `complexity/source-freeze-tested.json` records 40 changed/moved/deleted path states, unchanged through the full recipe. Subsequent documentation-only changes are this entry and conditional-scope refinements in the two skills; application code/test hashes remain identical. Focused skill scenarios and static checks were repeated after those refinements.

| Check | Actual result |
| --- | --- |
| Strict API | Exit 0: 34 setup-helper tests, dprint, Rust formatting and strict all-target/E2E Clippy. Initial integration check stopped at model declaration formatting; corrected formatting and reran the complete recipe successfully |
| Full API recipe | Exit 0: 478 native tests (22.37s) and 211 real-HTTP tests (799.93s), zero failed or ignored. Native database tests ran with owned Compose PostgreSQL ready |
| Ownership | Payment contracts/value conversions moved to provider models; read-only payment decisions moved to services/payments. Webhook clients retain raw authentication/postback; notification services synchronously consume authenticated facts. Only checkout-specific dependency wrappers removed |
| Preserved inputs and effects | Moved payment/money assertions retain behavior. Four additional native cases cover malformed/duplicate Stripe metadata, exact PayPal correlation fallback and typed PayPal notification selection. Full HTTP suite retains authentication/error order, irrelevant-event acknowledgment, ownership, precise money, stable keys/fingerprints, fenced writes and lost-response recovery |
| Independent comprehension | A reviewer reconstructed Stripe, PayPal and lost draft/completion responses from code before reading the author plan/docs. Source-linked decisions, pre-effect checkpoints, original-resource lookup and no-blind-repeat guards are recorded in ignored complexity/independent-walkthrough.md. No introduced regression or unexplained helper hop found |
| Skills | Only repository DDD and architecture-review skills changed. Five fresh baseline samples identified missing independent-comprehension acceptance. Five fresh samples per updated skill and separate deadline-pressure cases require code tracing rather than totals/file moves; pressure checks preserve callback timing and reject exactly-once claims from lease alone. One DDD sample lacked the optional old architecture control and was scored only for its loaded DDD guidance. Final conditional wording was tested with five fresh samples per skill plus pressure cases: external uncertain effects require recovery tracing; pure in-memory formatting does not. Frontmatter and all relative links pass direct checks |
| Postman | Workspace lint: 84 entities, zero errors, one existing cron-preview FMT209 warning. Provider/internal HTTP contracts unchanged; no external collection executed |
| Cleanup | Compose enz48, PostgreSQL15448/S315449, no dotenv discovery. Full recipe teardown plus independent audit confirms no owned containers/network/API processes; both recorded PostgreSQL/S3 volumes removed |
| Evidence and limits | Ignored complexity/api-check-final.log, api-full.log, postman-lint.log, skill-tests.json, source hashes and cleanup-final.json retain results. Existing claim/recovery algorithm is unchanged: ten-minute token fencing protects database writes, not external exactly-once execution. No live provider payment/callback, merchant email, complete browser purchase or deployment claim. Hosted new-source checks are a separate delivery gate |

## PR #66 verifier-output cleanup — local verification (2026-10-24)

Results bind to approved review-plan version 4 atop `32f2685349b1c632cbf4e6e0ce601c8d6765e04e`. The ignored `source-freeze-v4-tested.json` records 18 changed source/test/documentation hashes, unchanged during the full recipe; this verification entry is the subsequent documentation addition.

| Check | Actual result |
| --- | --- |
| Strict API | Exit 0: 34 setup-helper tests, dprint, Rust formatting and strict all-target/E2E Clippy |
| Full API recipe | Exit 0: 474 native tests (22.91s) and 211 real-HTTP tests (802.83s), zero failed or ignored; native database tests ran against owned Compose PostgreSQL |
| Verifier boundaries | Removed unused payment context copies, pending/unpaid ID payloads, redundant webhook outputs and event-ID forwarding ending in an ignored argument. All 27 functions in the six payment/webhook/evidence/money modules remain used; PayPal evidence/money modules are unchanged. The unused Shopify Nodes Default impl is removed |
| Input regressions | Signed Stripe tests reject missing/malformed event IDs, non-string API versions and malformed nested references. PayPal tests reject invalid event IDs before postback and preserve capture/order contradiction checks. Expanded existing HTTP callback matrix rejects missing/malformed event IDs for both gateways and malformed Stripe API versions without advancement |
| Preserved behavior | Full suite retains trusted account/environment/money/correlation checks, refund/dispute guards, raw authentication, callback/recovery/fenced-state behavior and separate payment/Shopify completion. Generic provider DTOs, protocol serialization and HTTP contracts remain intact |
| Static review and Postman | Independent diff/consumer review finds no actionable regressions. Local workspace lint: 84 entities, zero errors and one existing cron-preview FMT209 warning. Initial top-level lint invocation was unsupported; corrected workspace lint exited 0. No provider collection was executed |
| Isolation and cleanup | Compose enz48, PostgreSQL15448/S315449, no dotenv discovery. Full recipe tears down owned services; independent audit confirms recorded database/S3 volumes, network, containers and API processes are absent |
| Evidence and limits | Ignored v4-api-check.log, v4-api-full.log, v4-postman-lint.log and v4-cleanup-final.json retain results. No live provider payment/callback, merchant email, browser purchase or deployment claim. New published-source CI remains a separate delivery check |

## PR #66 checkout review comments — local verification (2026-10-24)

Results bind to the approved review-plan version 2 and frozen source atop `239989608a418656437cbed0b08557a128f0be8a`. The ignored `.superpowers/review-comments/PR-66/source-freeze-full-tested.json` records all 33 changed implementation/test/documentation paths; hashes remained unchanged during the final full recipe. This verification entry is the only subsequent documentation addition.

| Check | Actual result |
| --- | --- |
| Final API strict recipe | Exit 0: 34 setup-helper tests, dprint, Rust formatting and strict all-target/E2E Clippy |
| Final full API recipe | Exit 0: 473 native tests (22.49s) and 211 real-HTTP tests (797.69s), zero failed or ignored. Native database tests ran with Compose PostgreSQL ready; the recipe completed owned teardown |
| Focused checkout rerun | Exit 0: all 26 checkout TCP cases (106.68s), including Stripe/PayPal and post-mutation draft/payment ID-save failures. The subsequent full recipe includes all these cases |
| Persistence and recovery | Fresh SQLx migrations create one checkout control/linkage table. Interruption-phase row assertions reject local email/items/purchase payloads and confirm removed item/event/rate tables. Draft recovery covers unique/zero/multiple/noisy/paginated/incomplete results, dependency failures, changed purchase evidence and required-tag subsets; ambiguous absence never creates another draft |
| Payment and completion | Tests preserve exact money, ownership, price consent, last-unit contention, PayPal whole-unit currencies and six-digit response timestamp stability. Gateway retries compare the original key/body/frozen deadline; owned-row faults verify recovery after upstream creation but before ID persistence. Paid and uncertain Shopify completion remain separate; callbacks share fenced state and preserve raw authentication/irrelevant-event handling |
| Initial failure and correction | First full run: 473 native passed; 205/210 TCP passed. Four failures shared a stale controlled draft-search page-size assertion (100 instead of 250); another retained the former 200 expectation for an invalid draft instead of the approved safe 409. Corrected fixtures/assertions without changing production code, then passed checkout and complete final recipes |
| Postman/static review | Workspace lint: 84 entities, zero errors, one pre-existing cron-preview FMT209 warning. New DraftOrders query bytes and variable keys match the formatted source; exact-tag example uses the production quoting. Scoped whitespace checks and independent source review pass. No provider request collection was executed |
| Journey artifact | Local journey HTML/Markdown updated for authoritative Shopify data, browser attempt credentials and the pre-draft recovery boundary. Desktop1440/mobile390 renders in light/dark have no page overflow; viewport screenshots inspected. These are documentation renders, not frontend purchase acceptance |
| Isolation and cleanup | Compose `enz48`, PostgreSQL15448/S315449, synthetic credentials, no dotenv discovery. Final independent audit finds no owned containers, networks, volumes or API processes; both recorded database/S3 volume names are removed. The reused ENZ-48 checkout and unrelated work remain intact |
| Evidence and limits | Ignored `api-check-final.log`, `checkout-final.log`, `api-full-final.log`, initial `api-full.log`, manifests and `cleanup-final.json` preserve actual results and source boundaries. No live Stripe/PayPal payment, merchant email delivery, production webhook registration, complete browser purchase or deployment was performed. Hosted results remain delivery-owned and must match the published source |

## ENZ-48 durable guest checkout — local verification (2026-10-24)

Final implementation source `08e98fec8a2bea2aca1ee4e59f798fc2590c2e9a` includes main `7a319c36f5b12b8e5def22b08743394947b1ceaf` and merged ENZ-35. The final checkout-only inventory-policy guard has separate regression evidence below; earlier full-suite results retain their original source boundary.

| Check | Actual result |
| --- | --- |
| Full API regression before final dependency integration | Existing API `just test` passes 463 native and 204 real-TCP tests, zero failed/ignored; TCP phase 791.34s. This run precedes the final ENZ-35 catalogue revision and shared oversized-body error follow-up; it is not a claim that all 206 final TCP cases ran locally together |
| Integrated strict/native checks | API check passes 34 setup-helper tests, dprint, Rust formatting and strict all-target/E2E Clippy. Integrated `5d5e473` passes 466 native API tests with Compose PostgreSQL ready, zero failed/ignored (22.60s). Strict API check is repeated successfully after the final checkout guard |
| Integrated affected TCP modules | Ten catalogue cases pass (46.12s) and 25 convention cases pass (37.75s), zero failed/ignored. These modules are unchanged by the final checkout-only guard |
| Final checkout TCP matrix | All 21 cases pass after the guard, zero failed/ignored (103.89s). Uses real API TCP, SQLx/PostgreSQL and controlled Shopify/Stripe/PayPal HTTP services; covers both gateways, request/authentication rules, exact money/reservation, early/duplicate callbacks, capture/completion faults, lost responses, restart/ownership, rate admission and last-unit contention |
| Inventory integration regression | Initial integrated checkout run fails its existing CONTINUE-policy case because catalogue eligibility broadened. Checkout now explicitly requires DENY, tracking and sufficient available stock. The same unchanged regression passes in the final 21-case run; catalogue continues accepting both policies |
| Recovery and body limits | Dedicated native regressions verify completion intent is saved immediately before the actual Shopify mutation and late Stripe idempotent recovery retains the original expiry. TCP regressions confirm unknown outcomes never blindly create replacements, paid state survives Shopify failures, and oversized create/verify/webhook requests return shared safe JSON errors |
| Shared contracts | Domain check passes native strict lint and `wasm32-unknown-unknown` compilation; 18 unit and eight contract tests pass on the integrated DTOs. The final checkout-only guard does not change domain code |
| Postman / scope | Final workspace lint scans 84 entities with zero errors and one existing cron-preview FMT209 warning. Checkout requests and provider projections are synchronized. No collection-wide provider mutations were executed. Diff against main contains only checkout-related changes; ENZ-35 source worktree and existing migration remain untouched |
| Isolation / cleanup | Owned Compose project `enz48`, PostgreSQL port 15448 and S3 port 15449; temporary synthetic API configuration and controlled providers. EXIT teardown removes owned services, volume and network. Independent final Docker and API-process audits are empty |
| Limits | No live charge, genuine merchant webhook delivery, native Shopify email, frontend purchase/recovery acceptance, production routing or deployment is established. Frontend pages/storage are a future consumer contract. Publication and hosted CI results are recorded separately from this local evidence |

### Hosted timestamp-precision follow-up

[CI run 39995173768](https://github.com/enzumeii/monolith/actions/runs/39995173768) tests source `f8a09ddbfe682ad878ffa431bc7e485ae27530bb` through synthetic merge `c062407fbc8bfc15ef246d0c10e82d2933b59977`; both have tree `2c4cdd525c0c36f8140c23f39872c8e1a8abcd4b`. Build succeeds and all 466 native tests pass, but TCP finishes 204 passed / two failed. Both failures expose nanosecond initial reservation timestamps versus microsecond PostgreSQL recovery timestamps on Linux; payment identities, amounts, status and redirect actions match.

| Follow-up | Actual result |
| --- | --- |
| Deterministic regression | A forced `.123456789` deadline versus its PostgreSQL-precision roundtrip fails before repair, including on macOS. Checkout response serialization now uses the existing Chrono microsecond truncation mechanism; initial and recovered deadline strings agree. Existing TCP equality assertions remain unchanged |
| Final local checks | API strict check passes 34 setup tests, dprint, Rust formatting and both strict Clippy targets. All 467 native API tests pass (22.79s). All 21 checkout TCP cases pass (103.63s), zero failed/ignored; both gateways and restart/retry regressions included |
| Source / limits | Follow-up changes only checkout response timestamp formatting and its native regression. Domain, catalogue and convention source remain unchanged from their successful checks above. Owned Compose teardown succeeds. The failed hosted run is retained as failure evidence; a later passing run must be confirmed on the updated published source |

## ENZ-35 PR #57 latest-main integration — local verification (2026-10-23)

The integrated source merges review fixes `fb08fff6e635bbc2b19af78f71485ff966ae64a6` with main `7bd6c2e050f7991e9f6eeeeb5127c2483d79eeba`. Three additive conflicts retain both Shopify client operation groups, convention catalogue documentation and complete verification histories. PayPal crate visibility and fail-closed currency handling remain intact. Independent review identified and cleared one semantic interaction: the expanded currency enum must not silently widen draft-completion payment support.

| Check | Actual result |
| --- | --- |
| Completion currency compatibility | Focused regression fails before repair for newly named KRW/VND/AED/INR, then passes 4/4. Explicit arms preserve main's exact 31 supported currencies and precisions; every other enum variant remains unsupported. Existing completion behavior and tests remain intact |
| Final native / strict checks | Repaired source exits 0: 443 native tests (22.19s), zero failed/ignored, with PostgreSQL ready; Rust formatting and strict all-target/E2E Clippy pass. The preceding full API check also passes 34 setup-helper tests and GraphQL/JSON formatting |
| Focused real-TCP integration | Initial resolved integration tree `7d2acce9f37991bc71dc0d913e68b47e59e70924` passes 10 catalogue (45.13s) and 24 convention (37.67s) cases. The later two-file completion-helper repair does not change these endpoint paths; those TCP modules were not repeated |
| Shared contracts | Domain check and test pass 14 unit + 8 contract cases, strict lint and WASM compilation on the integrated DTOs; the isolated completion-helper repair does not change domain code |
| Preservation / isolation | All 51 upstream changed paths retain their additions; the completion helper and its focused tests have the documented compatibility correction. All 57 disjoint feature paths remain identical. Final owned Compose down-v exits 0; container, volume, network and API-process audits are empty |
| Evidence / limits | Ignored final-integration-*.json/log/sh scripts and precision-repair-report.md retain source hashes, reviewed index binding, exact commands, red/green proof and actual exits. The full 184-TCP result above remains bound to the pre-integration review fixes. No repeated full local TCP/coverage run, fresh hosted-CI, live-provider, browser or deployment claim |

## ENZ-35 PR #57 catalogue and inventory-policy snapshots — local verification (2026-10-23)

Results bind to the frozen 26-path working-tree delta atop `b996cbdb6c4eb39bcabf41e3eee156521e055dc0`, which includes main `0c506da45e1d0e0498e508b2dd475eb3767a712d`. Complete diff SHA256: `64f64a712993267f8720dab5c1b827e361412e9bf4f718201dc8e2ea51d30b92`. Later main revisions require separate integration evidence; older checkpoints below retain their original bindings.

| Check | Actual result |
| --- | --- |
| Shared contracts | Expected red drops inventory_policy before implementation; domain check and test pass: 14 unit + 8 contract cases, strict lint and WASM compilation |
| API strict / integration | Exit 0: 34 setup-helper tests, dprint, Rust formatting, strict all-target/E2E Clippy. Removed the duplicate cron declaration and retained PayPal's explicit supported-currency policy with fail-closed rejection for the expanded enum; focused payment-money tests pass 4/4 |
| Final full API recipe | Exit 0: 423 native tests (22.60s) + 184 real-TCP tests (692.76s), zero failed/ignored. One final native run with PostgreSQL ready, then all TCP modules, followed by owned Compose down-v |
| Catalogue / snapshots | Active/tracked root search, grouped OR search, absent public product status, generic upstream completeness failure, cursor metadata, DENY admission, nullable policy snapshots, strict stored/provider decoding, URL-only saved images, policy-only timestamps, legacy no-call behavior, atomic refresh/null clearing and concurrent locked-row preservation pass |
| Fresh schema | 26 columns across convention tables; five nullable display columns plus nullable inventory_policy TEXT constrained to DENY/CONTINUE, no image-alt columns. SQLx applies only the original convention CREATE migration; no production migration or backfill |
| Postman / static | Root workspace lint: 77 entities, zero errors, one existing cron-preview FMT209 warning. Both external GraphQL queries and variable keys match source exactly; workspace formatting and diff checks pass |
| Isolation / cleanup | Compose enz35, PostgreSQL15435/S315436, owned target, synthetic credentials and dotenv discovery disabled. Recipe and cleanup exit 0; independent label/process audit finds no owned containers, volumes, networks or API processes. All 26 frozen hashes remain unchanged before this evidence-only update |
| Evidence / limits | Ignored latest-source-freeze.json, latest-source.diff, latest-api-full.log, latest-schema-proof.log and latest-cleanup-audit.json retain source binding, actual execution and cleanup. No local coverage duplicate, live-provider, browser, deployment or fresh hosted-CI claim |

## ENZ-35 PR #57 migration fold — local verification (2026-10-23)

The direct user-authorized correction folds seven nullable display TEXT fields into the original unpublished `20261023090000_conventions.sql` and removes the separate display-metadata migration. Results bind to `migration-fold-source-freeze.json` atop `b2bf1d3c06b6076b320cd57314d80154fb55a45a`; API, provider, DTO, routing and test implementation remain unchanged.

| Check | Actual result |
| --- | --- |
| Effective schema | PostgreSQL catalog comparison of former CREATE-plus-ALTER against folded CREATE is identical: all 27 columns across both tables, including physical order, types, nullable/default semantics, constraints, indexes, function and trigger. Display fields remain nullable TEXT with no defaults; timestamp definitions are unchanged |
| Fresh migrations | Real SQLx applies the committed migration sequence to fresh owned Compose PostgreSQL. Ledger records `20261023090000` once and no removed additive migration; no backfill or production migration |
| Native / convention TCP | Exit 0: 389 native tests (22.40s) and 24 real-TCP convention tests (38.59s), zero failed/ignored. Missing-data rows, snapshot refresh/null clearing/provider failure, locked snapshot retention, UTC, constraints, rollback and routes remain covered |
| Strict / static | Exit 0: API check including 34 setup-helper tests, dprint, Rust formatting and strict all-target/E2E Clippy; workspace formatting and diff whitespace checks pass |
| Isolation / cleanup | Owned `enz35` Compose on PostgreSQL15435/S315436, existing target, synthetic credentials and dotenv discovery disabled. Down-v exits 0; Docker label and owned API-process audits are empty. Frozen product/documentation hashes are preserved |
| Evidence / limits | Ignored `migration-fold-*.log`, `migration-fold-results.json`, schema equality and cleanup JSON retain commands and actual exits. Initial proof stopped on a harness count typo (30 instead of 27) before Cargo tests, cleaned up, then the corrected fresh run passed. Earlier full 184-TCP evidence remains bound to v4 below; fresh full hosted CI/coverage remains delivery-owned. No live-provider, browser, deployment or production-database action |

## ENZ-35 PR #57 display metadata and route nesting — local verification (2026-10-23)

Historical results below apply to approved v4 commit `b2bf1d3c06b6076b320cd57314d80154fb55a45a`, whose delta atop `34e1e9645830cc6e908e839f77d3251161034ea8` is bound to ignored `delta-source-freeze-v2.json` under `.superpowers/review-comments/PR-57/2026-10-23/`. The later migration fold has separate fresh evidence above.

| Check | Actual result |
| --- | --- |
| Shared contracts | Expected red loses display fields before implementation; final domain tests pass 13 unit + 8 contract cases, zero failed/ignored. Domain check passes strict lint and WASM compilation |
| API strict check | Exit 0: 34 setup-helper tests, Rust/GraphQL formatting, strict native/all-target and TCP-test Clippy |
| Final full API recipe | Exit 0: 389 native + 184 real-TCP tests, zero failed/ignored; TCP phase 692.05s. Native suite runs once with Compose PostgreSQL ready; recipe removes owned PostgreSQL/S3 containers, volume and network |
| Convention regressions | All 24 cases pass, including five new TCP cases for snapshot refresh/null clearing/offline retention, nullable legacy data, missing/malformed requested provider fields, slashless nested routes and aliases, and latest locked snapshot retention. Existing lifecycle, last-completed-save, later-provider failure, write/commit rollback, pagination, auth/cookie/CORS, currency and UTC regressions remain green |
| Postman / static | Lint from the actual workspace root: 72 entities, zero errors, one existing cron-preview FMT209 warning. Three touched YAML requests parse; ConventionVariant query bytes and variable keys match the formatted GraphQL source. Workspace format and diff checks pass |
| Cleanup | Independent v4 post-run audit finds no enz35 containers, volumes, networks or API processes |
| Limits | Controlled Shopify TCP and real PostgreSQL/S3 dependencies only. No live merchant/browser/deployment run or extra local coverage run; fresh hosted CI remains delivery work |

Results describe this revision's exercised scope, separately from [architecture](architecture.md) and [operations](development.md). They do not establish live Shopify behavior or production capacity.

## ENZ-37 convention administration and logo uploads — local verification (2026-10-23)

The feature is stacked above ENZ-35/36. The PR #58 follow-up verifies the revised logo contract; earlier feature results below are historical evidence. No live provider, production migration or deployment is asserted.

### Latest-target PR #58 conflict resolution

Integrated committed backend `78f1f43` into UI `467bdce`, preserving S3 logo handling/options and the backend's persisted variant display/inventory-policy snapshots. Both branches' contract/native/TCP test cases remain represented; catalogue rendering no longer expects the removed public product-status field.

| Check | Result |
| --- | --- |
| API | Owning check passes: 34 setup tests, formatting and strict lint. Full isolated recipe passes 449 native and 197 real-TCP tests, zero failures/ignored; TCP runtime708.35s |
| UI/shared contracts | Backoffice native/WASM check, 170 tests and web build pass; domain16 unit +8 contract tests and WASM check pass. Current GET/PUT response tests preserve metadata presence/null semantics, IDs, policy and exact prices |
| Other merged consumers | Client-ui check/test8, report-generator native/WASM check/test7, root workflow/shell/Vercel checks and111 Node tests pass. Postman lint: zero errors and one existing unrelated cron preview warning |
| Actual merged API browser | Catalogue search at location1 shows titles, SKU, actual loaded variant image and stock7/0; selections survive pages with exact overrides17.29/23.45. Controlled provider failure writes no rows and retains price/metadata drafts; retry persists both rows and leaves the independent metadata draft intact |
| Actual persistence/reload | PostgreSQL rows contain corresponding titles, SKU, variant image URL, DENY policy and exact prices. Fresh browser reload with the controlled provider unavailable retains the metadata/images/prices, without loading catalogue results. No response proxy or metadata injection was used |
| Layout | Desktop and375px screenshots inspected; actual images load and remain at most96×96. Narrow document360px fits viewport375px;640px table scrolls within302px container. Temporary viewport override reset |
| Review/cleanup | Independent API/domain/docs and UI reviews report no actionable regressions. Temporary E2E registration restored byte-for-byte; owned API/upstream/UI processes and Compose containers/network/volume removed |

Selected API responses still omit available quantity. In-session catalogue observations remain labeled by location; fresh saved rows truthfully show “Not available”. Older nullable display rows are not automatically backfilled. No live Shopify/S3/payment mutation occurred; only a public HTTPS synthetic image was read. Screenshots were inspected in browser output; no exported screenshot artifact is included. Source-bound logs, approval, manifests and controlled database evidence remain ignored under `.superpowers/pr58-conflict-20261023/`. Hosted publication/mergeability checks are recorded separately; local passes do not imply hosted CI success.

### Variant viewer and pinned-base conflict resolution

Merged committed backend `34e1e96` into UI `3e3db67`, preserving S3 logo locations, signed previews and attachment validation. Future backend metadata work and the later requested rebase remain excluded.

| Check | Result |
| --- | --- |
| API | Owning check passes: 34 setup tests, formatting and strict Clippy. Full isolated database/HTTP recipe passes: 395 native tests and 192 real-TCP tests, zero failures/ignored; HTTP runtime 702.87s. Compose resources removed afterward |
| Backoffice/contracts | Owning check/test pass: 170 backoffice, 14 domain unit and 8 contract tests, domain WASM check and strict native/WASM lint. Final web bundle builds successfully; client-ui check/test pass with 8 tests |
| Static integration | Independent API, UI and combined-source reviews report no actionable regressions. Resolved index has no unmerged entries; root formatting and whitespace checks pass |
| Real merged API browser | Synthetic login and convention; catalogue at location1 shows separate product/variant titles, SKU, loaded variant image and stock7/0. Select across pages, exact overrides17.29/23.45, controlled upstream save failure/retry, independent metadata draft retention and successful legacy-save detail retention pass. Editing unsubmitted location2 leaves observed stock labeled location1 |
| Explicit future-response simulation | Controlled proxy injects optional fields only in GET/PUT responses. Fresh enriched load/save shows titles/SKU/image/stock7/0 and exact prices. Explicit null clears details even while catalogue data exists; controlled503 blocks selection editing/save until retry loads all rows. Response enrichment never appears in request payloads: only product ID, variant ID and final_price |
| Layout/images | Desktop and375px screenshots inspected. Loaded HTTPS synthetic image has natural width72; selected thumbnails remain at most96×96. Narrow page has no horizontal document overflow (document360px within375px viewport); table640px scrolls inside302px container. Row actions remain on one line |
| Postman | Workspace lint: zero errors, one existing unrelated cron preview query-parameter warning. Catalogue path and create/options descriptions match the merged API |
| Cleanup | Temporary browser test registration restored byte-for-byte; owned API/upstream/UI/proxy processes and Compose containers/network/volume stopped. Other worktrees remain untouched |

Current pinned backend responses omit saved display metadata and selected quantity. Fresh legacy reload therefore displays explicit unavailable placeholders while retaining exact prices. Enriched reload/quantity browser evidence above is a **controlled future contract simulation**, not proof of backend persistence. The planned PR #57 metadata fields do not yet include selected quantity; final backend contract and the queued rebase remain required. Stock is informational, with no automatic refresh or invented saved location. No live Shopify/S3 mutations occurred; the public synthetic HTTPS image was read only. Screenshots were inspected in browser output; no exported screenshot artifact is included.

Source-bound logs, approval and response-simulation events are retained in ignored `.superpowers/variant-viewer-20261023/`. Hosted checks are recorded separately after publication; local passes do not imply hosted CI success.

### PR #58 logo review follow-up

| Check | Actual evidence |
| --- | --- |
| API complete recipes | Strict check passed (21 setup-helper tests, dprint, formatting, native/all-target/E2E Clippy). Full test passed: 360 native +191 real-TCP, zero failed/ignored; TCP701.52s. Owned PostgreSQL/RustFS services, network and volumes removed |
| Backoffice | Strict native/WASM checks,14 domain +8 contract +161 backoffice tests, domain WASM compilation and final web build passed |
| New endpoint and association | Seven upload and25 convention TCP cases pass. Actual RustFS accepts the returned signed image URL without Authorization and rejects query tampering403. Cases include body/path/auth boundaries, provider failures, complete image checks, location-only SQL, pre-insert/patch failure ordering and optional preview degradation |
| Browser creation and rendering | Optional68-byte PNG uploaded before creation. SQL stores `conventions/<slug>/<UUID>.png`; signed S3 image renders with RFC3339 expiry. Returned signed GET is200/image/png and SHA256 `431ced6916a2a21a156e38701afe55bbd7f88969fbbfc56d7fe099d47f265460`, identical to original and stored bytes |
| Browser picker and retry | Same absolute file works after discard and upload/clear, without editor reload. Native input clears after discard/success; upload failure retains candidate. Attach failure after successful upload preserves the previous DB key; retry saves the staged location with unchanged PUT count |
| Browser drafts and refresh | Unsaved name and13.99 price survive both resets, upload/attach failure and preview GET failure/recovery. Explicit refresh restores image/new expiry without replacing drafts. Both saved through real API/SQLx; SQL final price is exact13.99. Browser diagnostic query returned no warnings/errors |
| Independent review | Backend/domain/E2E, frontend, and final cross-layer source reviews accepted with no actionable findings. Original native-picker regressions failed before the fix and pass after adapting to upload-plus-attach |
| Postman |73 requests lint with zero errors and one existing unrelated cron-preview FMT209 warning. Shared runtime globals connect upload/location, create/PATCH and unauthenticated signed image GET; obsolete proxy requests removed |
| Isolation and limits | Controlled Shopify/S3 plus owned PostgreSQL/RustFS only. Final source hashes unchanged after checks. Temporary browser module removed byte-identically, helper/server stopped, browser tab closed and Compose resources removed. Browser tests cover image-error refresh rather than a five-minute wall-clock expiry wait; SDK expiry/signature behavior has separate automated checks. No live provider/deployment claim |

The revised contract stores locations and returns expiring previews. Old signed links may work until expiry; unreferenced private objects have no garbage collection in this scope. The approved stacked target remains `feat/ENZ-35`; GitHub CI only triggers main-target PRs, so local checks are not hosted CI evidence.

### Original feature verification

| Check | Actual evidence |
| --- | --- |
| API full recipe | 348 native + 185 real-TCP E2E passed, zero failed/ignored; TCP phase 699.37s. All nine new options/logo scenarios passed. Owned Compose PostgreSQL/S3 containers, network and S3 volume removed after the run |
| API strict check | Existing check recipe passed: 21 setup-helper tests, dprint, Rust formatting, strict native/all-target/E2E Clippy |
| Shared contracts | 20 domain unit/contract tests and database-free WASM check passed |
| Shared UI / client | Shared UI 9 tests, native/WASM strict check; client 8 tests, native/WASM check and web build passed. Both app bundles include the shared stylesheet |
| Backoffice | Native/WASM strict check, all 154 tests and web build passed after the browser corrections; the final CSS-only rebuild also passed |
| Coverage | Scoped UI LCOV: 42/43 lines (97.67%); scoped backoffice LCOV: 2,143/3,522 lines (60.85%). Both module validators passed. These reports precede the narrow browser corrections. Native coverage excludes browser/WASM runtime |
| Root tooling | Full existing just check passed: actionlint, shellcheck, Bash syntax, 111 Node tests and pinned Vercel config resolution. An initial existing two-second signal-fixture startup failure was rerun successfully; no unrelated test policy changed |
| Endpoint contracts | Real API routing/middleware, SQLx/Compose PostgreSQL and controlled provider TCP cover options/auth/origin/validation, raw upload/full image validation, durable managed URL, current-reference access, bounded HEAD/GET, corrupt/missing data and S3/database failure ordering |
| Postman | Changed convention and S3 request groups lint clean and use an actual tiny PNG fixture. Full internal collection lint exits 1 with the existing unmodified transactions FMT209 URL/query error (42 scanned; changed groups: 16 scanned, zero diagnostics); no cloud run uploaded |
| Final backend integration | Combined code `7bd20ac` is based on ENZ-35/36 `2fb0874`, including its upstream Shopify client integration: strict API check, 80 client tests, 23 Shopify model tests, 9 catalogue TCP cases (44.90s) and 28 convention/logo TCP cases (42.64s) pass, zero failed/ignored. Owned Compose cleanup independently confirmed. Rebase leaves apps/shared-contract source byte-identical to the tested browser-fix commit |
| Independent review | Shared UI, logo/API and whole feature source reviews completed. Session-refresh and metadata PATCH findings were corrected and tested. Fresh focused reviews also accepted the loading/currency/catalogue corrections. Reviews are source evidence, separate from runtime checks |

| Browser check | Actual evidence |
| --- | --- |
| Browser creation / metadata | Real cookie sign-in, invalid date-range rejection, create, edit and saved reload passed. Immutable currency shows THB and slug is read-only; Markdown scripts remain escaped and unsafe links are removed |
| Browser catalogue / prices | Numeric location + empty search, paging, source 19.99 THB, stock 7 versus 0, cross-page selection retention and truthful unloaded-source details passed. SQL persisted exact 12.99; forced Shopify failure retained draft 13.99 while SQL stayed 12.99; retry succeeded. Later saved 14.99 survives route reload. Explicit enable and disable passed |
| Browser logo | Real file chooser selected the 68-byte PNG. Actual S3 object and unauthenticated API image bytes match SHA256 `431ced6916a2a21a156e38701afe55bbd7f88969fbbfc56d7fe099d47f265460`; DB stores an API-relative current-logo URL. Image returns image/png, no-store and renders after reload. Failed S3 PUT retained previous image and selected file; retry, clear cancellation/confirmation and re-upload passed. Replaced URL returns 404 |
| Drafts / accessibility | In-app and native Back cancellation preserve metadata, exact price and selected-file drafts. Confirmed discard leaves saved data unchanged. Cancel initially receives focus; Escape cancels and restores invoker focus. Showing/refocusing the browser preserved all three drafts; the mounted with_session regression separately verifies same-owner refresh retains pending work and account/role changes reset ownership |
| Responsive / browser diagnostics | Desktop 1280 and phone 390 viewports inspected. Desktop panels use 1160px; phone document client/scroll width stays 375px with the 640px membership table inside a 317px scroll region. Temporary viewport/visibility overrides restored; no browser error/warning logs. Screenshots inspected in browser tool output; no exported screenshot artifacts claimed |
| Browser recovery regressions | Actual mounted list/detail tests failed before changing resource subscription ordering, then passed for asynchronous success/failure. Explicit selected currency and catalogue GID/optional-search wire regressions now pass; accepted test sockets use blocking reads to avoid the observed WouldBlock race |
| Manual isolation | Synthetic admin/conventions, real owned PostgreSQL and controlled Shopify/S3 HTTP only. Temporary test-module declaration removed; helper/server stopped and Compose containers/network/volume removed. No private environment settings or production providers used |

The approved stacked PR targets `feat/ENZ-35`; current CI triggers only main-target PRs, so no hosted CI is claimed for that stacked review.


## PR57 review feedback — 2026-10-23

These results apply to review-feedback commit `ea5346741e3ccd76454ed2fa058c250b00be6877`; older results below retain their original source bindings. The current API removes capability configuration, uses user-supplied database currencies, maps catalogue at `/api/v1/admin/conventions/catalogue`, and combines grouped raw search with `status:active` while returning only tracked variants. GET/list/PATCH own the existing reads and manual enable flag. SQL formatting preserves constraints, binds, transactions and locks; `TIMESTAMPTZ`/UTC DTOs preserve timestamp instants and event calendar dates.

| Check / contract | Actual evidence |
| --- | --- |
| Route/filter regression | Focused real-TCP catalogue test fails before correction (400 instead of 200), then passes with active/tracked filtering, grouped OR search, exact prices, tracked zero stock and unchanged empty-page cursor metadata |
| Configuration regression | Focused real-TCP test fails before correction with `Convention currency is not currently enabled`; after removal it creates THB, SGD, JPY and HUF conventions without a currency configuration section |
| UTC regression | Focused native PostgreSQL test passes: verifies the owned session renders `+07`, then exercises real repository creation/update and DTO conversion for convention/variant timestamps at offset zero; created timestamps/absolute instants/event dates preserved, updated timestamps advance, transaction rolls back |
| Strict/static checks | `just --no-dotenv --justfile services/api/justfile check` exits 0:21 setup-helper tests, dprint, API formatting and strict native/E2E Clippy. Workspace formatting and whitespace checks pass |
| Full API recipe | One actual `just test` run exits 0:357 native tests (22.50s) and179 real-TCP tests (689.51s), zero failed/ignored. Includes final currency precision/accounting rejection, UTC, moved-route, active/tracked catalogue, configuration removal, rollback/commit-fault and race regressions |
| Postman / SQL guidance | Workspace lint exits 0:67 entities, zero errors, one existing cron FMT209 warning. Four changed YAML requests parse; external query bytes/variable keys match source. Migration SQL differs only in whitespace; seven retained repository operations preserve SQL tokens/binds. Skill schema/example and independent representative SQL-guidance application pass; system Python validator lacks PyYAML, so equivalent Ruby Psych metadata validation was used |
| Isolation / cleanup | Owned Cargo target and Compose `enz35`, PostgreSQL15435/S315436. `JUST_NO_DOTENV=true` disables nested dotenv loading; `COMPOSE_DISABLE_ENV_FILE=1` disables Compose dotenv discovery. An initial numeric dotenv flag was rejected before tests/services ran; corrected actual run completed once. Recipe down-v and independent audit confirm no owned containers, volumes, network or API process. All31 changed-path hashes matched the source freeze before this evidence-only update |
| Evidence / delivery boundary | Ignored `.superpowers/review-comments/PR-57/2026-10-23/implement-report.md`, full logs, `source-freeze.json` and `full-cleanup-audit.json` retain actual commands, exits and source binding. No fresh hosted CI, live-provider, production or deployment proof is asserted. The later latest-main integration has separate local evidence below; subsequent exact-source hosted CI remains publication-owned |

### Latest-main integration — 2026-10-23

The integrated tree merges review-feedback commit `ea5346741e3ccd76454ed2fa058c250b00be6877` with main `10d5985a6c631551429863a23ff6eb75bb555524`. The merge has no conflicts and preserves the convention changes together with main's Stripe provider helpers and QA configuration/setup behavior. The earlier full 179-case TCP run remains bound to the review-feedback commit above.

| Integrated-source check | Actual evidence |
| --- | --- |
| Strict API check | Exit 0:34 setup-helper tests, dprint, API formatting and strict all-target/E2E Clippy |
| Native API suite | Exit 0:389 passed, zero failed/ignored (22.39s), including QA overlay/precedence, Stripe helpers, currency and non-UTC PostgreSQL/UTC DTO regressions |
| Focused real-TCP modules | Exit 0:10 catalogue cases (45.26s) and19 convention cases (29.18s), zero failed/ignored; existing local API harness and controlled provider connections |
| Isolation / cleanup | Same owned target and Compose `enz35`/ports15435/15436, no dotenv discovery. Corrected lifecycle exits 0; down-v exits 0 and independent Docker/process audit finds no owned resources. An earlier initializer consumed script stdin before any Cargo test; its logs are retained separately and it is not test-suite evidence |
| Evidence / limits | Ignored `integration-report.md`, `integration-tests.sh` and `integration-*.log` under the PR57 review evidence directory retain the exact commands, parents, exits, preservation audit and cleanup. No repeated full179-case local TCP run, fresh hosted CI, live-provider or deployment proof is asserted |

## ENZ-35 + ENZ-36 convention configuration and catalogue — local verification (2026-10-23)

Earlier combined source `a9bf7b0` on `feat/ENZ-35` merges `origin/main` `b3cc7a7` (single browser origin allowlist) with feature source `e57bfb4` and documentation `7597077`. Results are local. The complete API regression below covers this merged source, including the strict request-validation follow-up. Domain/frontend and outbound GraphQL query sources are unchanged from their separately verified revisions. The final review follow-up below has separate scoped evidence; the full recipe remains tied to frozen `a9bf7b0`.

| Check / contract | Actual evidence |
| --- | --- |
| API strict check | Existing `just check` exits 0 at merged `a9bf7b0`: 21 setup-helper tests, pinned dprint, Rust formatting and strict native/E2E Clippy |
| Full API recipe | Existing `just test` exits 0 at merged `a9bf7b0`: 345 native + 176 real-TCP tests, zero failed/ignored; TCP phase 703.61s. Includes all 18 convention cases and upstream allowlist/cookie/CORS regressions; recipe completes owned dependency cleanup |
| Domain boundary | Existing check/test recipes pass: 12 unit tests + 8 contracts, zero failed/ignored; strict native Clippy and `wasm32-unknown-unknown` compilation. Common PatchField cron compatibility and decimal-string wire types covered |
| Frontend consumers | Existing backoffice check/test pass: 131 app tests plus 20 shared-domain tests; client UI check/test pass: 8 app tests. Both native/WASM strict lint and domain database-free WASM compilation pass |
| Workspace/static | `cargo fmt --all -- --check` and whole-tree `git diff --check` exit 0 at merged `a9bf7b0`. Final verification-document fence/text and scoped whitespace checks also pass |
| Postman | Local workspace lint exit 0: 66 entities, zero errors, one pre-existing cron-preview FMT209 warning. Both new GraphQL documents match saved request bytes and executable variable keys. Seven convention examples and catalogue first/continuation examples parse and retain exact `"12.99"`, raw search and cursor variables |
| Report preservation | 22 existing report/provider request/query files remain byte-identical to base `884fdc0`; catalogue omits supplier cost and uses `available`, while inventory/report `on_hand`, sorting, paging and numeric totals remain unchanged |
| Controlled endpoint coverage at `a9bf7b0` | Eight catalogue and 18 convention E2E scenarios use real API TCP, SQLx/Compose PostgreSQL and controlled Shopify TCP. Assertions cover content/zero-vs-null availability/raw search/paging/overflow; metadata/default false, both roles, cookie origins/PUT preflight, strict query/DELETE body rejection, exact SQL NUMERIC 12.99, policy/eligibility failures, rollback/commit faults and concurrent enable/clear/save/PATCH behavior |
| Isolation | Owned worktree Cargo target; Compose project `enz35`, PostgreSQL port 15435 / S3 port 15436. Recipe removed owned PostgreSQL/S3 containers, S3 volume and network; independent post-recipe queries found no owned resources or API process remaining. No shared production database or provider was used |
| Detailed evidence | Ignored `.superpowers/sdd/ENZ-35-convention-configuration-variant-pricing/` retains task reports, full check logs and query hashes; `integration-report.md` records merged source, integration audit, command exits, test counts and independent cleanup proof |

### Final review follow-up

The scoped follow-up after `a9bf7b0` changes only the catalogue nested-overflow message and adds the missing extraction regressions. The full 345-native/176-TCP result above remains evidence for frozen `a9bf7b0`; the two added TCP scenarios were exercised by the focused runs below. The full suite was not repeated for this narrow follow-up.

| Check / contract | Actual evidence |
| --- | --- |
| Overflow red/green | New real-TCP regression first fails with the old generic error (exit 101), then passes after the catalogue-owned mapper returns exact 502 `Product variants exceed the supported catalogue browse limit of 250`; no partial products, original private cause retained, shared report/upstream mapper unchanged |
| Focused endpoint matrices | `cargo api-e2e catalogue`: 9 passed, zero failed/ignored, 44.23s; `cargo api-e2e conventions`: 19 passed, zero failed/ignored, 29.43s. Includes existing generic provider failures, both roles, cookie origins/PUT preflight, rollback and concurrency regressions |
| JSON extraction/no effects | POST/PATCH/PUT each reject malformed JSON with JSON Content-Type and valid JSON with absent/wrong Content-Type: all nine raw requests return exact safe 400. SQLx snapshots prove every owned parent/selection field unchanged and no new POST row; controlled Shopify records zero additional business calls |
| Strict/static | `just --no-dotenv --justfile services/api/justfile check` exits 0: 21 setup tests, dprint, Rust formatting and both strict Clippy targets. `cargo fmt --all -- --check` and final `git diff --check` exit 0 |
| Postman | Local lint exits 0: 66 entities, zero errors, same existing cron FMT209 warning. Both executable catalogue descriptions contain exact safe overflow/generic examples; both outbound GraphQL query bytes and variable sets remain identical to source |
| Isolation/cleanup | Reused owned target and synthetic `enz35` ports 15435/15436 after the full-suite owner released them. Compose used `--env-file /dev/null`; recipe check used `--no-dotenv`. `down -v` exits 0; independent queries find no owned containers, volumes, networks or API process |
| Detailed evidence | Ignored `final-fix-report.md` and `final-fix-*.log` beside the integration evidence record exact commands, exits and scoped changed paths |

### Publication refresh against upstream draft-order client

Merged source `ddaf159` integrates feature head `d2e337f` with upstream `1622b23` (provider draft-order proxy). The sole textual conflict appended Shopify client tests; both complete parent additions are retained. Production client/models auto-merged unchanged: existing operations keep retries, draft creation uses `execute_once`, and both catalogue identity checks and the upstream DraftOrder ID type remain intact. The fresh results below apply to this merged source. The earlier full 345-native/176-TCP run remains tied to `a9bf7b0`; no full 178-TCP local rerun is claimed.

| Check / contract | Actual evidence |
| --- | --- |
| Strict/static | `just --no-dotenv --justfile services/api/justfile check` exits 0: 21 setup-helper tests, dprint, API formatting and strict native/E2E Clippy. Workspace `cargo fmt --all -- --check` and final whitespace/document checks exit 0 |
| Full native API suite | `cargo test --locked -p api` exits 0 with Compose PostgreSQL ready: 355 passed, zero failed/ignored, 22.39s. Includes both merged Shopify test additions and database-dependent tests |
| Affected real-TCP matrices | `cargo api-e2e catalogue`: 9 passed / 169 filtered, 45.95s; `cargo api-e2e conventions`: 19 passed / 159 filtered, 29.40s; zero failed/ignored. Existing real API harness, SQLx and controlled Shopify connections retained |
| Postman parity | New upstream draft request plus both feature requests match source GraphQL bytes and executable variable keys exactly. Prior feature lint remains the separately recorded 66-entity result; no fresh workspace lint or provider execution is claimed |
| Isolation/cleanup | Owned target and Compose `enz35`, ports 15435/15436; synthetic credentials, `--env-file /dev/null`, no private environment file reads. EXIT-trapped `down -v` exits 0; independent queries confirm no owned containers, volumes, networks or API process |
| Remaining full-suite boundary | Mechanical test-append integration and unchanged endpoint production logic warranted focused TCP verification. Exact merged-head full 178-TCP CI remains publication-owned and is not established by these local results |
| Detailed evidence | Ignored `refresh-report.md`, `refresh-verify.sh` and `refresh-*.log` retain identity, exact commands/exits, 355/9/19 counts, conflict preservation proof and cleanup audit |

Controlled responses establish exercised transport/contracts, not live Shopify schema compatibility, merchant permissions/capabilities, query cost or a real payment. No production migration/deployment, live provider mutation/charge, manual browser UI acceptance or hosted CI is asserted. Public APIs (ENZ-38), routing (ENZ-41), quotes, checkout and historical order snapshots/protection remain future work; parent deletion currently cascades current memberships.

## ENZ-45 stateless completion remediation — 2026-10-23

This contract supersedes PR #56's earlier stateful checkout design. Verification results for the superseded design do not establish acceptance of this refactor. The initial stateless remediation at `66c03bd` and its continuation onto `d1ae724` passed the bounded checks below. The later dependency moves PayPal verification into its client module with typed errors and enum currency handling; no mounted endpoint changes in that continuation.

| Scope | Current implementation / verification boundary |
| --- | --- |
| Completion | Trusted server-side caller context, existing authoritative Stripe/PayPal verification, exact draft/order money, real Shopify linked-order readback and additive verified reference tags |
| Recovery | `CompleteIfOpen` versus `ReconcileOnly`; caller retains uncertainty and serialization. Reconciliation-only calls never issue completion. No local checkout/payment ledger, guest UUID endpoint, cache or exactly-once claim |
| Reporting | Actual Shopify order tags flow through existing matching, DTOs and CSV; no saved-order fallback, database mapping, extra provider lookup or cron change |
| API checks | Final continuation formatting, strict all-target/E2E Clippy, 34 setup-helper tests and all 430 native tests pass, including 14 completion and 47 PayPal cases. The earlier full API recipe passes 150 real TCP tests; endpoint code and the accounting regression remain unchanged by the client-only continuation, so that suite is not repeated. Zero failed or ignored; owned Compose containers, volumes and networks removed |
| Shared code and exports | Domain 18, report-generator 7 and backoffice 131 tests pass, including actual CSV/ZIP tag bytes. Native/WASM checks, root workflow/shell/configuration checks and workspace formatting pass |
| Contracts and review | Stripe 9, PayPal 7, Shopify 15 and internal 30 Postman files lint with zero diagnostics; all 10 Shopify query/variable pairs match source. Accounting tag regression fails when the tag assignment is removed and passes restored. Independent final review finds no actionable gaps; cron, routes, repositories and migrations match the dependency |
| Hosted checks | Main-only PR workflow does not run for the stacked `feat/ENZ-44` target; current-head hosted CI is waived by the approved plan. Historical CI and Vercel preview are separate evidence |
| Limits | No live Shopify/email proof, production migration or provider mutation. Trusted caller authorization/durable ownership, future public/browser/webhook integration and merchant-specific behavior remain outside this slice |

## ENZ-44 PayPal client boundary and currency enum — 2026-10-23

| Local evidence | Result |
| --- | --- |
| Current scope | Verification and evidence/money helpers live under `client::paypal::payment`, returning typed ClientError. Existing CurrencyCode variants govern precision with strict canonical spelling; wire fields and supported currencies are unchanged. |
| Complete API check | 34 setup-helper tests, formatting and strict API/all-target/E2E Clippy pass. |
| Full API test recipe | 412 native tests and 150 HTTP E2E tests pass; zero failed or ignored. Owned Compose containers, volumes, network and API processes are gone. |
| Focused PayPal regressions | All47 provider/payment/event tests pass, including accepted currency scales, unsupported/noncanonical inputs, checked money, GET-only verification and typed upstream error causes/locations. These tests are included in the native total. |
| Independent review and binding | No actionable findings. All509 source-manifest files stayed unchanged through verification; the final documentation result row is added afterward. |
| Unchanged-contract evidence | Domain/Postman/shared currency/wire/config files are unchanged from b9a7695; its18 domain tests, strict/WASM checks and external Postman32file lint remain the bounded receipts below. Hosted CI is recorded separately for the new published head. |
| Limits | No new Sandbox charge or genuine signed-webhook acceptance test. Earlier Sandbox receipts are historical; consuming flows still own application HTTP mapping, replay/state handling and Shopify completion. |

## Historical ENZ-44 PayPal provider boundary at b9a7695 — 2026-10-23

These receipts describe `b9a7695`, before moving payment verification into `client::paypal::payment` and reusing `CurrencyCode`. They do not verify the subsequent client-module, error-boundary or currency-enum changes; that continuation requires separate results.

| Local evidence | Result |
| --- | --- |
| Resolved parent merge | Integrates main `10d5985` after #55 merged. Latest Stripe client/payment implementation and tests match main; PayPal Sandbox QA override is retained. |
| Complete API check | 34 setup-helper tests, formatting and strict API/all-target/E2E Clippy pass. |
| Full API test recipe | 407 native tests and 150 HTTP E2E tests pass; zero failed or ignored. Isolated Compose containers, volumes and network are removed. |
| Provider/verifier coverage | Controlled HTTP cases verify approved-but-unpaid, final completed capture, exact money/identity, inconsistent evidence, dependency errors and authenticated typed events. Verification makes no payment mutation. |
| Domain and collections | Domain formatting, strict lint, WASM compilation and 18 tests pass. External Postman lint scans32 files with zero diagnostics. Internal lint retains the unchanged unrelated transaction FMT209 baseline. |
| Independent review | No actionable findings in the reduced provider slice or resolved merge. No public PayPal gateway, payment/event ledger or Shopify completion remains. |
| Limits | No new Sandbox charge. Historical receipts below establish approval/capture semantics only. Genuine signed-webhook acceptance is unverified because no webhook is registered. Hosted CI and Vercel results are separate published-head evidence. |

## Historical ENZ-44 PayPal gateway — 2026-10-23

These receipts describe the superseded database-backed gateway. The current provider slice removes its public routes, payment/event tables, capability policy and ingestion loop; these results do not verify the stateless refactor. Current refactor checks are recorded separately after execution.

| Local evidence | Result |
| --- | --- |
| Focused real-network endpoints |33 new payment/webhook tests pass together on final scoped source, zero failed/ignored;91.90s. Affected native service tests15/15 pass;0.20s. Actual API child, Compose PostgreSQL and controlled PayPal TCP server exercise routing, SQLx and outbound HTTP. Dedicated PostgreSQL15444/S315445; default checkout target cache. |
| Request and ownership boundaries | Strict body/path/header validation, exact money, canonical guest capability, ownership, idempotency and immutable inputs; status never initiates capture. |
| Payment and webhook evidence | Authoritative order/capture verification, optional redundant provider fields absent, pending/terminal/mismatched evidence, retries/response loss/retention, duplicate references, concurrent capture/event reconciliation and verified webhook deduplication. |
| Dependency failures | OAuth/order/capture/signature failures include HTTP rejection, malformed response, timeout and connection loss. Owned PostgreSQL triggers test write/transaction failures; isolated child-session search path tests SELECT failures without shared schema/data mutation. |
| Review regressions | Observed RED fixed and covered: busy/pending completed webhooks remain retryable; embedded identity conflicts reject; typed unusable create and zero/multiple captures reject. A later second capture on one payment retains both captures/MULTIPLE review flag and processes the valid event; fresh wrong capture ID still rejects. Retrieval faults leave PENDING receipts and identical-event retry completes them. |
| Static checks | Strict API all-target/E2E Clippy, API formatting and diff whitespace checks pass. Owned test containers/network/volume are removed with independent empty readbacks. All four new internal Postman requests lint without errors/warnings. Full internal lint has one pre-existing unrelated FMT209 queryParams/URL mismatch in the transactions request. |
| Limits | No live PayPal Sandbox, real approval UI/signatures, Shopify draft conversion or browser journey. Provider evidence-field permutations are not exhaustive; detailed cases/limits are in ignored task artifacts. Parent full API checks and exact-head hosted CI remain separate evidence. |


### Historical ENZ-44 PayPal Sandbox QA continuation

This later real-Sandbox evidence superseded the earlier no-Sandbox limit for the old gateway only. It confirms the approval/capture distinction, not acceptance of the current stateless provider slice. QA overrides the REST origin with `https://api-m.sandbox.paypal.com`; buyer approval uses `www.sandbox.paypal.com`. Existing ignored QA credentials were loaded by the ordinary settings reader.

| Real Sandbox check | Observed result |
| --- | --- |
| Create, approval and capture | Application TCP create201; buyer approved the same USD1.00 order; capture200/COMPLETED; real capture lookup and trusted verification both confirm USD1.00/COMPLETED. |
| Durable identity and ownership | Create replay200 reuses the order; capture replay200 reuses the same capture with one evidence record; wrong owner404; preapproval capture/trusted verification409; wrong expected amount409. |
| Webhook limit | No webhooks registered. Deliberately malformed signature request returns provider400/application502 without storing an event; genuine signed verification/ingestion remains untested. |
| QA configuration limit | Checked-in QA sets the API origin only. The smoke injected a temporary payment policy with the discovered merchant and a deliberately invalid webhook ID for negative tests; the old gateway required the complete merchant/webhook/currency/redirect policy, which has since been removed. Buyer return-page UI was a test placeholder. |
| Cleanup and evidence | Temporary Rust source restored exactly; owned test processes and Compose containers/volumes/networks removed; plaintext capability/resume payloads deleted. Sanitized details: `.superpowers/sdd/2026-10-23-ENZ-44-paypal/sandbox-qa/report.md`. No production payment, Shopify write or publication. |

## ENZ-27 static services and cron implementations — 2026-10-22 / 23

The approved scope uses static CSR frontends and a single application configuration path. Only the cron implementation differs between VPS and Vercel. Earlier BFF, frontend-container and application-runtime-profile results are superseded. All results are local; no cloud link, deployment, production migration or publication was performed.

| Evidence | Scope |
| --- | --- |
| Pinned Vercel 65.1.0 parser/resolver | Actual JSON resolves two `@vercel/static-build` services plus one `@vercel/container`, public API/app routes, zero bindings, service-local SPA/asset rules and one daily cron |
| API final checks | Existing check recipe passes: 19 Python setup tests, dprint, Rust formatting and strict Clippy. Existing full test recipe exits 0: 311 native tests and 135 real-TCP tests, zero failed/ignored; TCP phase 602.84s and owned Compose database/Blob/network/volume removed |
| Independent review | Final scoped source review found zero Important/Critical findings; all 271 backend freeze fingerprints match. Earlier build-host PATH finding was fixed and rechecked through the public helper and actual AL2023 builds |
| Postman | CLI lint passes all 49 requests and the 28-request internal collection. Final owned local health/cron execution passes 2 requests and 3 assertions, zero failed; its Compose project removed and no cloud results uploaded |
| Frontend checks | Both existing app check/test recipes and ordinary web-only builds pass; 117 backoffice tests, 8 client tests and 17 domain tests. No frontend API binding is required |
| Production CSR outputs | Both shared-script builds pass; actual JS/WASM/CSS/config paths use `/backoffice/assets/` or `/user/assets/`. Output contains only public files, production `/api/v1` and no native server. Local TOML restored after success and forced builder failure |
| Amazon Linux compatibility | AL2023, aarch64, glibc 2.34, Rust 1.99.0 and source-built dx 0.7.10. Under two-CPU/8GiB limits: fresh dx source compilation/install 5m43s, uncached backoffice 49.5s and client 28.4s; peak memory 3.08GiB, exit 0 and no OOM |
| Compatibility limits | Rust toolchain and registry sources were cached for the constrained run; dx executable and app builds were fresh. Local image installed compiler/development prerequisites. Vercel's exact image, hosted x86 build and deployment remain unverified |
| Actual unlinked services | Static roots/nested routes/config 200, every generated asset's HTTP bytes match local output, missing assets 404 and unknown `/api/*` 404. Development-only static commands read the same route configuration; deployed services remain static |
| Public API transport | Actual local cookie login/refresh/logout, HttpOnly/Path=/SameSite=Lax, bearer access/rotation, bad bearer 401, foreign-origin 403, ADMIN create 403; authenticated cron 204/no-store and missing/wrong authorization 401, POST 405. The same checks pass again against final cron-only API image sha256:249c1290f8534d7fa6a0d06b868d0b93efd7b8cf7444f29869a75bca81defc04 |
| Final API image | Non-root UID10001, CA/OpenSSL libraries resolved, common/prod TOML readable and RUN_ENV=prod; command api server connects without migrations. All 271 final backend source fingerprints match |
| Final v11 browser | Actual unlinked Vercel: SUPER_ADMIN login displays owned report metadata; nested history reload restores session and authenticated row; logout returns login. Prior static CSR checks include invalid configuration → Retry → restored configuration and direct download |
| Root checks | Final actionlint, shellcheck, Bash syntax, 92 Node policy tests and pinned Vercel parser/resolver pass; final diff whitespace check passes |
| Manual Release deployment | Nine controlled local regressions pass: separate release/deployment concurrency groups, manual/main/commit/permission isolation, all three missing secrets fail before any command, one root production CLI call for all services, and install/config/build failures propagate without a success summary. Existing release publication tests pass unchanged; no hosted dispatch/deployment, cloud link or secret setup was performed |
| Public schema boundary | Current public schema includes services but declares draft04; installed AJV cannot load its meta-schema. CLI validation passes independently; full public-schema validation is not asserted |
| Download | API metadata returns controlled direct storage URL; downloaded 33-byte CSV and attachment header match SHA256 d3967d5fadc4ac3b928d5f9c108c427ac411753891674dada51416d328e5b136 |
| Build limits | Official Basic machine: 2vCPU/8GB, 32GB disk; build step 45min. Local artifact footprint fits below 32GB, but disk quota was not enforced and hosted resource behavior is unverified |
| Root routing limit | Unlinked dev can serve repository directory HTML 200 at `/`; hosted production 404 remains unverified |
| Cleanup | Owned local Vercel/CSV processes stopped, browser database and resource-proof container removed; no owned verification ports/listeners remain. Synthetic root env and credentials removed only after hash checks; images, caches, artifacts and unrelated containers preserved |
| Evidence files | Commands, exits, source snapshots and detailed limits are retained under ignored `.superpowers/sdd/2026-10-22-ENZ-27-vercel-services/v10/platform/`; final API/cron suite and browser receipts are under v11/backend and v11/verification |

Compatibility boundaries follow the official [build-image documentation](https://vercel.com/docs/builds/build-image), [build machines](https://vercel.com/docs/builds/managing-builds) and [build limits](https://vercel.com/docs/limits). Local builds, parser validation and unlinked development do not establish hosted deployment behavior.

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
| Saved accounting request | Actual Postman HTTP collection request ran with inherited synthetic bearer token: populated200, empty200 and missing-PayPal503; each CLI exit 0/zero request failures. Separate real-HTTP assertions checked populated/empty shapes and exact safe provider error; collection itself contains no test scripts |
| Final local verification | Independent combined diff/consumer review found no gaps. Strict API check, workspace formatting, patch whitespace and Postman lint passed (19 internal/18 external, zero warnings/errors); six saved examples and all 11 external GraphQL query/variable pairs verified. CI for the final pushed source SHA is recorded separately |
| Limits | Controlled localhost providers only; no live provider compatibility, merchant scope, cost, load, email or deployment claim. Native picker save/cancellation/focus remain manual/unverified this run: permitted tab Escape did not dismiss the native dialog. Browser fallback was exercised only in a disposable capability-disabled bundle |


## ENZ-34 Accounting inside Reports (2026-10-22)

| Area | Observed result / coverage |
| --- | --- |
| Local implementation checks | Backoffice check/test/build passed at a2234ae: strict native/WASM lint and formatting, 70 backoffice tests, 11 domain tests, domain WASM check and browser bundle. Independent implementation review reported no actionable findings |
| Post-merge coverage recovery | Backoffice `check` and `test --coverage` pass: 115 backoffice tests, 9 domain unit tests, 8 domain contracts and domain WASM check. Three rendered native Reports flows exercise real TCP HTTP, Convention next/previous/All, Picklist results/edit clearing/empty state and failed generation/Retry recovery. Four realistic mutations fail the relevant regressions. |
| Local coverage estimate | Exact CI39736868921 artifact union is 7,798/9,222 lines (84.5587%). Replacing its backoffice LCOV with fresh local LCOV gives 7,856/9,222 (85.1876%); Reports improves 81/136 to 134/136. Deduplicated by source path/line; hosted Codecov at the new commit remains unverified. |
| Browser setup | Real built UI localhost:18084, real API localhost:33034 and isolated Compose PostgreSQL port 55434; synthetic cookie login and controlled external provider HTTP services; no internal endpoint stubs or production credentials |
| Navigation | One Reports heading; Convention/Picklist/Accounting selector defaults to Convention; no Accounting navbar entry. Browser reload returns Convention controls with no Accounting date field; former /accounting renders NotFound |
| Accounting generation | UTC+07:00 date controls; success shows Stripe 1 / PayPal 1 / Items 2. Reversed dates show Start must be before end. All-empty results disable ZIP; transaction-only Stripe 1 / PayPal 1 / Items 0 enables ZIP |
| Provider failure / retry | Controlled provider 503 produces safe Upstream request failed and Retry; subsequent generation recovers with transaction-only results |
| Downloaded ZIP | Actual browser download accounting-report.zip inspected with Python zipfile/csv: 1120 bytes, exactly transactions.csv/items.csv, 20/11 columns and 2 rows each; comma/quote-containing title Fixture, "Keychain" preserved |
| Type lifecycle | During a 5-second delayed Accounting request, Generating appears; switching to Picklist prevents a stale Stripe region after the response. Picklist FIXTURE generates 1 row; returning to Accounting clears dates/results and disables ZIP |
| Existing product reports | Convention location 1 first/next/previous pages verified; All awaited both Fixture Product 1 and Fixture Product 2 and reported 2 rows. Supported fallback save downloaded Picklist CSV (224 bytes, 14 columns, 1 row, quoted Fixture, "Keychain" title) and Convention CSV (317 bytes, 14 columns, 2 rows, Fixture Product 1/2); actual bytes parsed with Python csv. Save and Cancel close the fallback dialog; Save re-enables |
| Presentation | Light and actual system Dark appearance inspected at normal size and 390x844 narrow viewport with generated Accounting results. Dark background rgb(29,34,40), text rgb(237,242,246); controls fit (client width 375, start-input bounds 37–338). Original system Auto setting confirmed restored and viewport reset |

Item-only results cannot be produced through the unchanged real backend; existing native item-only export coverage remains. Existing native serialization/save and session ownership/refresh/date/archive regressions passed in the implementation suite; they do not establish browser coverage of those unexercised paths. Native FileSystem picker save remains unverified because the automation surface could not access its dialog. Product CSV fallback was exercised using a disposable serving-page injection disabling showSaveFilePicker before the unchanged app module, matching browsers without that capability; no product code changed. No live provider, production, deployment or hosted CI claim; confirm exact published-head CI separately.

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


## PR57 catalogue eligibility and country review follow-up — 2026-10-23

| Check | Local evidence on frozen v7 source, based on `78f1f434` |
| --- | --- |
| Full API recipe | `just --no-dotenv --justfile services/api/justfile test`: 446 native tests (22.22s) and 185 real-TCP E2E tests (687.34s), zero failed/ignored; recipe exit 0 |
| Strict/shared contracts | API check passes 34 setup tests, dprint, Rust formatting and strict native/E2E Clippy; domain native/WASM check and 15 unit + 8 contract tests pass |
| Provider and examples | Root Postman lint: 80 entities, zero errors, one existing cron FMT209 warning; both Shopify GraphQL operation bytes and variable keys match source |
| Review regressions | URL-only images, supported active/tracked search with unchanged cursors, CONTINUE save/read/enable/reprice, exact 249-country enum acceptance and malformed-country rejection pass; original CREATE migration/UTC and pricing policy repairs preserved |
| Isolation and limits | Compose `enz35` cleanup exit 0; no owned containers, volumes, networks or API processes remain. Full logs and frozen hashes are retained in ignored `.superpowers/review-comments/PR-57/2026-10-23/v7-*` artifacts. Hosted CI/coverage remains a separate delivered-source check; no live provider or deployment proof is asserted |


## PR #58 main conflict resolution — 2026-10-24

| Item | Result |
| --- | --- |
| Integration | Reconciled main `7a319c3` with `feat/ENZ-37`, preserving current catalogue filtering, CONTINUE policy, URL-only images, CountryCode and release behavior alongside convention administration |
| Logo persistence | Reuses the existing database `logo_url` column for S3 object locations; published migration unchanged and no additional logo column or migration. Legacy HTTPS values survive unrelated edits without signed previews |
| Completed local checks | API strict check, 34 setup tests and 454 native API tests passed. Unchanged UI source passed native/WASM checks, 173 backoffice tests and its build; domain18 unit/8 contract, other consumers/root and Postman checks passed |
| Review | Independent combined/API/UI reviews found no outstanding actionable issues; legacy-logo PATCH regression corrected with native and HTTP coverage |
| User-directed verification limit | User requested stopping tests and publishing the conflict resolution for implementation review. The full199-test TCP run was stopped (exit143), so it is not a full-suite pass. Current browser exercise and hosted CI completion were not required for this handoff |

Source-bound logs and approval/amendment evidence remain ignored under `.superpowers/pr58-conflict-20261024/`. No live provider mutation or deployment was performed. Earlier verification sections describe their own historical source only.


### Logo field naming follow-up — 2026-10-24

The stored value is now named `logo_url` in SQL, Rust models, request/response DTOs and UI consumers. Responses expose signed previews as `logo_preview_url` with `logo_preview_url_expires_at`; Postman requests and fixtures use the same names. This follow-up preserves the existing migration, upload validation and missing/null behavior. Tests and builds were not run for this naming change at the user's request; earlier pass results apply to their recorded source.


### PR #58 logo ownership review follow-up — 2026-10-24

The convention logo service now owns the existing image/path validation, upload, attachment and preview orchestration. The one-feature `storage` namespace is removed, and existing unit tests move to the matching service test path. Attachment requires a concrete S3 client; optional preview signing and operations without a new logo preserve their prior behavior. Generic S3 operations remain in the existing client. This refactor receives formatting, import/call-site review and independent static review only; tests, builds, lint, browser execution and hosted-CI waiting are explicitly waived for implementation review.


### Explicit upload destination and numeric catalogue location — 2026-10-24

The upload location includes its bucket: `conventions/ax-2026` produces bucket `conventions` and object prefix `ax-2026`, with the same full bucket/key location persisted and used for attachment and preview. Report operations explicitly retain their configured bucket. Previously misplaced objects in `reports/conventions/...` are not copied or read through a fallback. Local S3 setup initializes both buckets.

Catalogue requests now use numeric `location_id`; the backend constructs the Shopify Location GID. Existing example searches use `product_type` instead of collections. Raw search forwarding and provider GID inputs remain unchanged. Tests and builds are not run for these changes at the user's request; formatting and independent static review are the verification boundary.


### Convention administration UI enhancements — 2026-10-24

This change adds shared link/button affordances, compact list logos, options-backed country/currency selects, automatic list navigation after successful creation, and an existing-dialog catalogue picker with a fixed configured numeric location, escaped product-type/title AND filters and compact product/variant thumbnails. Label/value hierarchy and green/red state cues are applied to convention controls.

Verification is limited to formatting, scoped diff/import/contract review and independent static reviews under the approved no-test implementation-review plan. Regression sources and response/config fixtures are updated but tests, builds, lint, browser/Docker/provider execution and hosted-CI waiting are not run. No visual/runtime pass is claimed. Earlier verification results remain bound to their recorded historical source.


### PR #58 required clients and preview response follow-up — 2026-10-24

Convention services and projection now receive concrete S3 client references resolved at the HTTP boundary. Convention responses omit the stored `logo_url` and retain only signed preview URL/expiry; create/PATCH inputs and database persistence keep `logo_url`. The UI uses signed preview state without relying on the storage location and can clear a logo whose preview is unavailable. Current-head CI exposed a private CountryCode import; the fix uses the existing public re-export. Verification is limited to formatting and static source/diff review: tests, builds, lint, browser/provider execution and waiting for hosted CI remain waived by the user. Earlier execution results apply only to their recorded source.


### PR #58 deep review and shared presentation — 2026-10-24

| Scope | Evidence and limits |
| --- | --- |
| Review | Three focused reviews covered API/contracts, convention state/lifecycle and shared presentation; integrated static review found no remaining actionable issue |
| Fixes | Backoffice test targets now declare their JSON dev dependency; empty-logo pages stop polling. Shared heading wrapping handles long names. Stale cache-header assertions follow the intentional typed-JSON handler contract; Postman uploads use a committed PNG fixture and explicit Content-Type |
| Reuse | Create/detail share the complete LogoPicker. Page/section headings share one layout, and all three convention tables use existing DataTable with preserved reporting defaults |
| Compilation | Native all-target checks passed for backoffice, API, shared ui and client-ui; test targets compiled without executing tests |
| Bundle | Full backoffice just build passed, including WASM compilation, wasm-bindgen and asset bundling |
| Static checks | Internal Postman lint passed41files with zero errors/warnings; changed JavaScript parsed in its async eval context, and scoped diff checks passed |
| Unverified | No test suites, browser flows, real providers, database execution or current-head hosted CI completion were exercised |


### PR #58 files and validator boundaries — 2026-10-24

| Scope | Evidence and limits |
| --- | --- |
| Ownership | Generic upload orchestration moves to the files service; pure file/image/location validation has no service dependency. Convention services retain attachment ownership, preview projection and payment capability policy, including both PATCH merges |
| API checks | API just check passed: 34 setup tests, dprint, Rust formatting and strict all-target/E2E Clippy |
| API execution | Full API test recipe passed: 463 native tests and 200 real-TCP E2E tests, zero failures or ignored tests. New coverage preserves invalid payment capability errors before missing-S3 errors |
| Bundle and examples | Full backoffice build passed; internal Postman lint passed 41 files with zero errors/warnings |
| Agent guidance | Repository domain-driven-design and architecture-review skills clarify validators, shared upload orchestration, provider transport and convention attachment ownership. Structural/reference checks and fresh placement recognition passed; optional quick_validate.py could not run without PyYAML |
| Review and isolation | Independent connected-code review found no outstanding code regression; its documentation scope correction was applied. Owned pr58-domain Compose containers, volume and network were removed |
| Limits | No live providers, browser interaction or deployment exercised. Hosted CI is verified separately against the published commit; earlier verification sections remain bound to their historical source |

Logs and review evidence are retained under ignored `.superpowers/pr58-domain-feedback/`.


### Customer convention portal — ENZ-38–41 — 2026-10-24

| Scope | Evidence and limits |
| --- | --- |
| Source | One scoped branch from main `d677fc8`; public catalogue/quotes, customer portal/cart, shared Heading and root hosting; payment, backend checkout creation, stock reservations and deployment excluded |
| Shared regressions | Domain24 unit and8 contract tests, UI12 tests and backoffice183 tests passed. Domain/UI/backoffice formatting and strict native/WASM checks passed |
| Customer checks | Final exact client `just check` and `just test --coverage` passed: strict native/WASM Clippy,35 native model/HTTP/resource/event/context tests and8 JavaScript browser-bridge scenarios. Both production UI bundles built; root customer config/assets and restored local TOML verified |
| Root routing | Root `just check` passed116 Node tests plus actionlint/shellcheck/Bash syntax and pinned Vercel65.1.0 service/schema checks. HTTP/build fixtures cover root/catalogue/cart reloads, legacy308 paths, missing assets404, reserved/malformed paths and config restoration |
| API execution | Native API suite passed464 tests with Docker PostgreSQL ready; all217 real-TCP E2E tests passed, including17 new public-endpoint cases. Final exact API setup/format/native and E2E Clippy checks passed |
| Coverage recovery | Initial hosted test/build jobs passed; patch coverage68.69% missed90.17%. Added19 focused native HTTP/resource/event/bridge regressions. Local comparison of final client LCOV with unchanged hosted reports hit963/984 changed executable lines (97.87%); hosted status is reported separately on the PR |
| Postman | Checked-in VariantNodes query and variable parity passed. Local collection run against the actual synthetic API passed all4 anonymous directory/detail/products/quote requests and5 assertions, with remote reporting disabled. Workspace lint scanned88 files with zero errors; one existing admin cron-preview query warning remains |
| Browser | Actual built app and API used isolated Docker PostgreSQL/private S3 and a controlled Shopify HTTP server. Verified country-prioritized directory/safe Markdown, local text/type/page-size filtering and page reset, add/edit/remove, same-ID cart reload, separate convention currencies/carts, same-slug replacement identity rejection and a separate empty draft, server totals, stale-price acknowledgement, stock reduction, removed-item reconciliation, unknown-cart recovery, malformed storage and blocked-storage navigation, disabled metadata hiding, provider error/retry, narrow504px and desktop1265px layouts and keyboard focus |
| Review | Independent shared/hosting, API, portal and combined source reviews completed. Nullable-stock, checkout-route refresh and brand-navigation and recreated-convention identity findings were repaired and affected checks rerun; removed selections use customer-facing copy |
| Publication | Hosted CI is checked separately against the published PR head. No merge, live provider mutation or deployment performed |
| Hosting limits | A syntactically valid unknown slug receives an HTTP200 SPA shell before the app's unavailable state; API lookup returns404. Root route shapes/assets are restricted. No duplicate edge registry or permanent slug tombstones added. Custom helper query-preserving redirects do not prove equivalent pinned gateway query preservation; no URL-query filter contract exists |
| Provider/proxy limits | Controlled tests exercise actual database/provider client connections but do not establish live Shopify compatibility, live Vercel country-header injection, merchant tax/payment behavior or production cutover |

Task-scoped logs and full approval/source evidence remain outside the checkout under the local Plannotator plans directory. Historical sections above retain their original source and verification limits.


### Guest checkout, confirmation and recovery — ENZ-46 / ENZ-49 — 2026-10-24

| Scope | Evidence and limits |
| --- | --- |
| Integration | Based on the completed `feat/ENZ-38` portal, now squash-merged into main `9e789cfa`; product-detail/cart routes and standalone checkout return/cancel routes preserved. CI accepts feature-branch PR targets. PR69 contains checkout and ENZ-49 automatic recovery together |
| Checkout baseline | Before the recovery additions, strict API check passed 34 setup tests, dprint, Rust formatting and native/E2E Clippy. The owned Compose recipe passed 501 native tests and 252 real-TCP E2E tests, zero failures/ignored; those verification dependencies were removed. This is baseline evidence, not a full combined-source pass |
| Recovery persistence | Seven focused real-PostgreSQL tests passed for fair due claims, abandoned/capped failures, terminal/rearmed metadata and strict post-lock ownership fencing; independent review found no outstanding finding |
| Recovery business | Focused checkout (36), payment (23) and Shopify (21) tests and strict Clippy passed. A review finding about wrong-kind linked order identity was fixed and verified with 18 completion and four classifier regressions; existing public502 behavior is preserved |
| Recovery runtime | Four configuration and six runtime tests and strict Clippy passed; independent runtime review found no outstanding finding. The 17 new real-TCP recovery scenarios passed (26.35s), including in-flight VPS shutdown; all 35 existing order scenarios passed (124.33s). Strict E2E Clippy passed and the owned dependencies were removed. The combined isolated coverage recipe passed 531 native and 269 real-TCP tests (zero failures/ignored), with canonical API line coverage 97.01%. This full run predates the subsequent actual-provider tag corrections; their focused final-source evidence is recorded separately |
| Recovery demonstrations | Running API with isolated PostgreSQL and controlled providers: authenticated Vercel `GET /_cron` returned204 and recovered the existing Stripe attempt; invalid token returned401. An independent VPS timer recovered and captured the approved PayPal attempt without cron/verify/webhook requests. Each kept the original draft/payment IDs and made one Shopify completion; repeats created no duplicate. Normal reservation lifetime preserved; fixture due time forced into the past and VPS wake shortened to1s. Owned API/provider/database resources removed |
| Customer and shared checks | Client strict native/WASM lint and 74 native tests passed; final JavaScript bridge suite 20/20. The production bundle was rebuilt after allowing real Stripe hosted URL fragments only on the strictly validated Stripe HTTPS host; independent review found no issue. Domain checks, 26 unit and 8 contract tests, shared UI native/WASM checks and 12 tests, backoffice native/WASM strict checks and workspace formatting passed |
| Coverage recovery | All hosted tests/builds passed at `bb8f273`; changed-line coverage89.63% missed90.71%. Six focused mounted lifecycle/error tests cover45 additional production lines without production or threshold changes. Fresh client LCOV combined with unchanged hosted reports measures633/656 changed executable lines (96.49%); current-head hosted status remains separate |
| Bundle and tooling | Final production client bundle rebuilt after cart-comparison correction. Root tooling passed 123 Node tests, actionlint, shellcheck, Bash syntax and pinned Vercel 65.1.0 service/schema validation; canonical/legacy products/cart/return/cancel reload tests pass |
| Payment safety | Real TCP coverage includes null-URL Stripe processing/lost-create binding, stable original identities, late paid evidence with unsafe holds, duplicate callbacks, explicit PayPal capture, final-read expiry guards, exact purchase/alias validation and ambiguous Shopify completion without repeat mutation |
| Browser | Actual built client/real API with independent Docker DB/S3 and controlled providers: branded/disabled controls, both outbound provider selections, cancel/reload, PayPal capture/confirmation, Stripe processing/error/retry/confirmation, catalogue outage, blocked storage, terminal-unpaid return to cart, actual Shopify display names and narrow cart clearing verified. Browser-discovered JSON key-order clearing bug corrected and independently reviewed |
| Responsive/accessibility |390 px real-app iframe (375 px content when vertically scrolled) has no horizontal overflow, wallet marks/long text fit, keyboard Tab reaches card button. Receipt390 px client/scroll width. Browser viewport override was ineffective; no mobile-device wallet claim |
| Postman | Workspace lint: 92 entities, zero errors, one existing unrelated cron-preview warning. Standalone Postman runner exercised checked-in create/verify bodies and assertions for both providers: 4 local requests and 10 assertions passed, external reporting disabled. Collection mode required account login |
| Actual-provider corrections | Shopify rejected correlation and payment-reference tags longer than 40 characters. Compact 39-character UUID tags, Unicode-safe display-name truncation and 40-character hashed long payment references now fit the provider contract. Final reference-fix verification passed all 535 native tests plus 35 order and 17 cron TCP cases, strict Clippy and formatting; independent reviews found no concrete issue. The real-length Stripe reference regression reconciles an already-linked order in both modes without another completion mutation. Actual QA also identified the required Shopify `read_payment_terms` permission, now granted and confirmed by the exact query |
| QA delivery | A dedicated PostgreSQL/S3 runtime and one convention with one AUD4.00 variant exercised the production client. Stripe test and PayPal Sandbox payments returned to the real confirmation pages for `#5623` and `#5624`, respectively. Genuine authenticated webhooks completed exactly one Shopify order per checkout; PayPal made one capture, independently retrieved as completed for AUD4.00. Reports/recovery were disabled and browser verification blocked until each webhook completed. Both receipts display the actual order number and email/pickup instructions, and both purchased carts cleared |
| Exact PayPal event | Official postback comparison accepts the original real event JSON and rejects reordered JSON. The fix preserves its lexical event value through `RawValue`, with separate business decoding. Final native 536/536, order TCP 36/36 and recovery TCP 17/17 pass, zero ignored, plus strict native/E2E Clippy and formatting. Regressions cover unsorted, spaced and escaped JSON and rejected authentication without capture/completion; independent review found no issue |
| QA cleanup | Provider APIs confirm both original test/Sandbox webhook URLs restored. Temporary tunnel, local API/static/proxy and screenshot/report helpers stopped; owned QA convention disabled and data volumes retained with the two real Shopify orders for review. Isolated verification projects removed; the pre-existing user PostgreSQL on 5432 remains healthy |
| Limits | Controlled gateway scenarios retain their fixture-only limits; synthetic certificate warnings were left intact. The later Stripe QA flow uses the real test gateway and genuine event redelivery, with one real merchant Shopify order. Both gateways completed real test/Sandbox payment and signed-event acceptance. Live charges, wallet eligibility, Shopify email delivery and deployment are unverified; screenshots provide step evidence because video recording was unavailable. Current-head hosted CI is recorded separately on the PR |

Ignored logs/fixtures remain under `.superpowers/enz46/`; browser screenshots and detailed acceptance evidence are retained in the local Plannotator plans directory. Independent backend, frontend, integration and final comparison reviews found no outstanding concrete regression.

Final squash-merge reconciliation preserves all verified checkout changes and the portal’s upstream configuration composer. Production UI assets merge base settings with the selected override; backoffice retains its required base catalogue location with an API-only production override. The earlier temporary full-production-config restoration is superseded by that upstream composition fix. Checkout routes, browser tests and payment styles remain unchanged by conflict resolution.


### Checkout feedback and automatic customer recovery — 2026-10-25

| Scope | Evidence and limits |
| --- | --- |
| Final API | Full isolated recipe passed567 native tests and275 real-TCP endpoint tests, zero failures or ignored tests. Canonical LCOV hit13869/14343 executable lines (96.70%). Strict setup/format/native/E2E lint passed. API source hashes remain unchanged after this run; owned verification dependencies were removed |
| Final customer UI |96 Rust tests and20 JavaScript bridge tests passed; strict native/WASM lint and production build passed. Canonical LCOV hit1234/1268 lines (97.32%). Independent reviews of shared retry classification, read resources and checkout ownership found no outstanding finding after interrupted-body recovery was corrected |
| Automatic recovery | Configuration, directory, catalogue and complete cart refresh use serial5/10/20/30-second capped backoff for temporary HTTP/network/body-transfer errors. Complete malformed responses and local/security/identity failures remain terminal. Saved attempts and temporary initial-submit failures automatically open the original checkout; fresh hosted redirects retain one owner and cart editing stays locked. Intentional gateway continuation and price consent remain explicit |
| Customer presentation | Simple Quantity labels, smaller descriptions, aligned side-by-side total-bearing payment controls and branded processing/receipt presentation verified. Generic Retry/Check status controls removed; terminal unpaid guidance and specific operator-review instructions remain |
| Shopify/accounting controlled coverage | Versioned external-payment recording, exact namespaced convention slug, immutable legacy fingerprints, ambiguous-write read reconciliation and both cron entry modes are covered by the full TCP suite. Accounting tests exercise opaque Shopify payment IDs, source versus settlement currency, linked items and conflicting references |
| Actual new Stripe QA | One additional Stripe TEST payment of AUD4.00 succeeded. Its original draft converted to Shopify order#5625, but the named payment-recording mutation was rejected. Readback remains PENDING with AUD4.00 outstanding and one PENDING manual transaction. The checkout preserves the paid provider evidence and requests an operator check; no replacement charge or repeated recording mutation was issued |
| Financial QA hold | Per the approved failure boundary, the additional PayPal payment and publication of the gateway requirement are held. The rejected mutation's individual error details were not retained, so the exact prerequisite is not asserted. The active native Stripe/PayPal settings do not establish that the named manual-recording operation is supported. Earlier successful orders#5623/#5624 are retained unchanged and are historical evidence |
| Real accounting export | The actual accounting GET and native CSV encoders produced0 Stripe rows,1 historical PayPal row and0 item rows at collection time. Immediate Stripe paid evidence is reported separately from Shopify's pending state. No payout, settlement, fee, reconciliation row or PaymentSession was fabricated |
| Source bounds and limits | Hosted Stripe QA used the previously tested client bundle; the final automatic-recovery bundle was built afterward. Final browser recovery evidence is recorded separately in the local QA report. No new successful receipt is claimed for#5625. No live payment, new PayPal payment, wallet eligibility, email delivery, deployed cron, hosted CI for these unpushed changes or deployment is established |

Detailed local screenshots, source bindings, failure evidence and sanitized exports remain in the task's ignored verification artifacts and local Plannotator QA report. Historical verification sections retain their original source boundaries.

### Manual checkout accounting and fresh QA — 2026-10-25

The final source preserves the earlier ENZ-38 portal integration, customer recovery and ENZ-49 scheduler work. The user replaced the old-order repair with two fresh test/Sandbox payments and later explicitly kept the existing settlement timing. Payment completion and actual accounting-report inclusion are separate acceptance checks; the second remains pending provider availability.

| Scope | Actual evidence and limits |
| --- | --- |
| Final strict and full API | Exit0: 36 setup-helper tests, formatting and strict all-target/E2E Clippy; 608 native and278 real-TCP tests, zero failures or ignored tests. TCP939.80s. Canonical API LCOV15435/16231 executable lines (95.0958%). All305 recorded API/shared/configuration inputs remained unchanged through testing and the normal build |
| Customer source | All88 recorded frontend/shared/configuration inputs match the tested V6 source:96 Rust and20 JavaScript tests, strict native/WASM checks and production build. Current canonical frontend LCOV recomputes1361/1404 (96.94%); earlier recorded totals retain their historical source/evidence boundaries. Served WASM bytes match the local production artifact on both static and proxy origins |
| Correlation and manual recording | New version2 correlation uses one bare convention slug tag and singleton identity/provider-reference custom attributes. Existing correlation and completion versions retain independent replay semantics. Pending conversion, one default manual recording, strict paid readback and idempotent reference merge have separate recovery fences. Both cron modes cover normal and uncertain metadata writes |
| Accounting matching | Controlled TCP cases join both external manual sales and item rows using exact Stripe charge/PayPal capture references, canonical identity and source money. Native PaymentSession/legacy/refund paths remain. Source AUD/SGD totals are independent of settlement THB; manual labels and arbitrary Shopify payment IDs cannot establish association |
| Fresh Stripe | One authorized TEST AUD4.00 payment completed Shopify#5626. Original charge is paid/captured, unrefunded and undisputed; one successful manual SALE, zero outstanding, exact Session/PaymentIntent/Charge attributes and only qa-checkout-v7-c1cd486c as tag. The app automatically verified and displayed the real receipt; no local webhook arrived |
| Fresh PayPal | One authorized Sandbox AUD4.00 payment completed Shopify#5627. Original order has one completed final capture, exact Order/Capture attributes, one successful manual SALE, zero outstanding and the same sole convention slug. The user approved on PayPal; browser automation still showed sign-in, so the same approved checkout was navigated to its configured return URL. Automatic capture/verification/receipt then succeeded. This run does not establish the automatic PayPal approval-to-return redirect |
| Financial availability | Actual Stripe balance transaction is pending; no paid payout is available in the tested range. Actual PayPal capture is complete but reporting refresh predates it. Independent read-only inspection confirms both sets of original references and source money satisfy the production matching conditions. This is compatibility evidence, not actual exported-row acceptance |
| Empty PayPal report correction | Real HTTP200 empty first-page reports omit transaction_details. Only explicit page1/zero total_items/zero total_pages can normalize omission; null, wrong types, missing counts and omitted nonempty results remain errors. Two regressions failed before repair;82 PayPal-filtered tests passed afterward. The full suite additionally proves valid empty PayPal preserves populated Stripe output and nine malformed shapes remain sanitized502. The final real today-only accounting GET now succeeds with actual empty transactions/items |
| Actual export | Final two-day accounting GET and native CSV generation return0 Stripe rows,1 historical PayPal row and0 item rows; today-only GET returns0/0/0. Neither fresh payment is currently included. No settlement, fee, payout, PaymentSession or report row was fabricated, and no manual payout was created: Stripe TEST available balance is zero |
| Acceptance and delivery hold | Both new payments and all34 strict provider/Shopify checks pass. The user's requirement that both actual accounting entries link to these Shopify orders remains pending settlement/reporting availability. Preserve settlement-only timing; do not claim acceptance or publish the final PR changes/replies before this requirement is verified |
| Source and operational limits | Fresh browser payments used API binary18a1c3197b964370796221d21516f1dc1b93b252d9a7d5d88cf628c05cc51bcc and the unchanged production frontend. The decoder-only follow-up rebuilt API55b47606b1abf6ce8cd839b2d9e492b65e12375798d42a90171a417fb724291c for final read-only report/readbacks. Both original webhook registrations remain at qa.enzumeii.com; old orders#5623/#5624/#5625 remain unchanged. Test-owned dependencies were removed; no live payment, settings change, deployment, wallet eligibility or email-delivery claim |

Ignored source hashes, full logs, strict readbacks and redacted exports remain under the task verification artifacts and local V7 QA report. Screenshots capture actual UI steps; video recording was unavailable. The current-head hosted-CI/reply gate remains separate and has not been satisfied for these unpublished changes.


### Customer notes, contact footer and PayPal accounting refresh — 2026-10-25

| Scope | Evidence and limits |
| --- | --- |
| Customer notes | Optional note below email preserves meaningful text exactly in Shopify’s native draft note and the original session-only checkout request. Blank browser notes are omitted; at most 2,000 Unicode characters, allowing tab/CR/LF. Absent/null notes retain legacy request serialization. No note field is added to the checkout database, financial evidence or provider metadata |
| Contact footer | Shared layout renders five icon-only contact links across customer routes with exact destinations, accessible names, safe new-tab attributes and visible keyboard focus |
| Combined checks | Strict API checks passed, including 36 setup tests; full isolated recipe passed 610 native and 280 real-TCP scenarios, zero failures or ignored tests. Canonical API LCOV15448/16244 lines (95.0997%). Both providers cover exact note transport, same-attempt replay, changed-note rejection, original-draft completion and privacy. Domain 27 unit and 8 integration tests passed |
| Customer UI | Strict native/WASM checks, production build, 102 Rust and 21 JavaScript tests passed. Canonical client LCOV1389/1432 lines (96.9972%). Actual browser checks cover multiline Japanese/emoji/plain text, optional blank notes, 2,000/2,001-character boundaries, disabled empty-cart payment, 390px layout without horizontal overflow and footer keyboard focus. Six screenshots retained; no new checkout/payment submitted |
| Actual PayPal report | Refreshed accounting GET at 07:58 UTC includes capture94P10533NS108422M linked to Shopify7160459788459 /#5627 and one Gorou item with the same reference. AUD4.00 gross,0.73 fee,3.27 net and the sole convention-slug tag are preserved. Overall export contains0 Stripe,2 PayPal and1 item rows; the older unlinked PayPal row remains unchanged |
| Remaining accounting hold | All34 immediate provider/Shopify checks still pass for#5626/#5627. Stripe#5626 remains absent from the actual payout-based report. Keep existing settlement timing and final publication hold until its actual transaction/item linkage is verified; no pending/synthetic row or payout was added |
| Source and operational limits | Independent source review found no actionable regression. Controlled tests prove native note transport and completion of the same draft; no new merchant order was created to recheck Shopify’s note inheritance. Browser/source evidence and actual redacted report are retained in local V8 artifacts, preserving historical V7 results. QA conventions disabled, owned services/helpers stopped and original orders/data retained; no release, deployment or current unpublished-head hosted-CI claim |


### Checkout provider configuration and production defaults — ENZ-46 — 2026-10-25

| Check | Evidence and limits |
| --- | --- |
| Provider identity | Stripe account discovery precedes order mutations; invalid identity and account changes reject without rebinding an existing checkout. PayPal environment derives from the shared client URL and remains pinned to the original checkout. Numeric location settings produce Shopify Location GIDs |
| Controlled regression | Existing isolated coverage recipe passed613 native and284 real-TCP tests, zero failed/ignored; API LCOV15502/16299=95.1101%. Both cron modes, account discovery failures, switched accounts and webhook lookup failures are covered |
| Final configuration | User finalized production HTTPS/live/Vercel defaults in shared TOML and removed the redundant production overlay during the recipe. Production Rust and TCP sources were unchanged. Fresh final strict API checks and all613 native tests passed with Compose PostgreSQL; all267 final verification inputs stayed unchanged. Mode-aware fixtures exercise base plus local/QA/optional production configuration |
| Packaging and policy | API image copies the production base. Root check passed123 Node tests, actionlint, shellcheck, Bash syntax and pinned Vercel validation. New production-configuration skill and existing configuration skill passed metadata/relative-link validation; independent integration review found no actionable issue |
| Postman |94 entities, zero errors, one existing cron-preview warning. Current-account and Stripe webhook descriptions match discovered account identity; provider mutation requests were not executed as a collection |
| Actual payment evidence | Recorded V9 Stripe TEST card0077 payment completed Shopify#5628, including the exact native customer note and sole convention-slug tag. Existing PayPal Sandbox#5627 capture and its item appear linked in the native accounting export; both orders pass34 strict payment/order readback checks |
| Remaining financial acceptance | Stripe#5628 has available funds but no automatic paid payout in the recorded provider scan, so its actual accounting row/item remain absent. Settlement timing is unchanged. The user authorized publication before this acceptance step completes. No additional payment, manual payout, old-order edit, release or deployment accompanies this configuration follow-up |
