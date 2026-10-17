# Verification

Results describe the tested revision and scope; they do not establish live Shopify behavior or production capacity. [Architecture](architecture.md) and [operations](development.md) describe the current system separately from these results.

## Verified revision and checks

| Evidence | Result |
| --- | --- |
| Implementation | ENZ-11 product reports; published head and CI recorded in this change's PR checks |
| Native API tests | 96 passed, 2 Docker-dependent tests ignored in native phase |
| API with Docker PostgreSQL | 98 library tests and 38 real TCP E2E tests passed; Compose teardown completed |
| Formatting/lint | Workspace formatting and strict API, domain, backoffice and client-ui checks passed |
| Shared contracts / WASM | 2 domain contracts passed; domain WASM and backoffice native/WASM lint passed |
| Downstream tests | Backoffice 6 tests passed; client-ui build/test passed with no tests |
| Live Shopify | Not executed, explicitly left to user; no Shopify mutations |

Product E2E uses real API TCP endpoints and Docker PostgreSQL with a mocked Shopify HTTP upstream, explicitly requested for ENZ-11. It verifies the API integration against deterministic upstream data, not the live Shopify schema, scopes or store contents. Published-head CI must be checked separately from these local results.

## Exercised contracts

| Area | Evidence |
| --- | --- |
| JSON transport | Typed decoding, generic headers/body, malformed JSON, HTTP failures, timeout, redirect rejection |
| GraphQL errors | Errors classified before typed data decoding; null mutation results retain upstream errors/metadata; partial-data errors rejected |
| Backoff | HTTP 4xx and GraphQL throttling use one HttpClient loop/budget; unchanged replay payload; mixed-rejection four-attempt exhaustion; Retry-After minimum and budget; oversized-delay overflow regression |
| No replay | HTTP 500/502/503/504, unrelated GraphQL errors, mutation user errors, and ambiguous timeout |
| Configuration | Literal ConfigBuilder token override and standard file/env precedence; no construction helper or skipped token field; URL retained through overlays; optional absent section and ordinary missing-URL errors; redacted Debug |
| Money and IDs | Decimal strings paired with currency; malformed/wrong-resource IDs rejected |
| Client boundary | Nine aligned operation/document names; queries automatically collect every page without caller cursors; root and child methods remain independent; raw inventory and mutation userErrors/optional results retained |
| Catalog pagination | Existing-method regression failed with one variant instead of two before consolidation; shared product/variant continuation now passes alongside root-page, empty-child and failure/cursor cases |
| Order pagination | Header queries traverse all outer pages independently; line methods return501 ordered lines over250/250/1 pages; no root/line chaining; date offsets/status/test/cancellation retained |
| Tagged orders | Multiple order pages preserve response order; empty results, literal tag escaping, blank-input rejection, cursor failures, HTTP/GraphQL failure propagation, returned tags, precise shop money and currency; independent line lookup collects501 nodes; each query call starts without a cursor |
| Error source chain | Origin exposes the original ClientError through standard Error::source, retaining deeper reqwest/serde causes and the first client identity; focused regression reproduced the derive-generated box downcast defect and passes with explicit chaining |
| Error integration | Each original ClientError family retains its type/source when wrapped with the Shopify origin; real constructor/HTTP/decode/protocol failures log one ERROR under INFO/DEBUG at response/background boundaries; conversion/attachment silent, payloads private |
| Cursor safety | Missing, repeated, and cyclic continuations rejected |
| Payment/tag behavior | Search-literal escaping for payment ID and single-tag lookup, multiple matches, empty tag replacement, mutation errors |
| Inventory discovery | All location pages returned unfiltered, including active/untracked/zero/positive stock; missing location errors |
| Activation | Quantity arguments absent; available/on-hand retained in fixtures; stable idempotency key across throttling |
| Existing API | CI exercises authentication, authorization, cookie/origin handling, database operations, and registered endpoint tests |

| Product report output | Exact decimal numeric totals, SKU characters4–9, active catalog filter, on_hand quantities including negative inventory, missing metadata and separate missing SKUs |
| Product report HTTP | ADMIN/SUPER_ADMIN access, missing/invalid tokens, invalid query enum/ID, unavailable configuration, inventory/tag success, all parent/child pagination, empty matches, location404, upstream HTTP/GraphQL/malformed JSON502, sanitized bodies |
| Product report grouping | Matching SKU + price + currency merges quantities; differing price/currency/metadata remains separate; overflow rejects the report |

## Review and limits

| Area | Result / limit |
| --- | --- |
| Independent implementation review | Independent review found scientific-number parsing, exact scaled-total overflow and private diagnostic defects; regression tests cover corrections, including constructor source retention |
| Additional fix-pass checks | Scientific weights, Decimal maximum exact totals, constructor causes and private DEBUG diagnostics reproduced failures and pass after correction; missing location is checked before catalog/pricing |
| Deferred minor | Tagged-line grouping scans accumulated rows linearly per input row; very large distinct-row reports may need an index. No load test or capacity guarantee |
| Shopify compatibility | Official docs and local fixtures checked; no live schema/request execution; actual Shopify query cost unverified against the single-query maximum |
| Platform pagination | Shared catalog traverses product and variant pages; shared tag and line methods traverse independent order/line pages |
| Concurrent changes | No snapshot isolation; missing/mismatched order and child-page failures return errors rather than partial success. The shared catalog follows variant continuations |
| Browser behavior | No new browser verification, rendered E2E, or HTTPS cookie acceptance evidence |
| Coverage percentage | Not measured; no percentage claim |
| Performance/deployment | No load test, production capacity result, deployment, or merge |
| Postman preservation | Product request omits unused nested variants.pageInfo; outer pageInfo is retained. No live Postman execution |
