# Verification

Results describe this revision's exercised scope, separately from [architecture](architecture.md) and [operations](development.md). They do not establish live Shopify behavior or production capacity.

## Checks

| Evidence | Result |
| --- | --- |
| Native API tests | 101 passed; 2 Docker-dependent tests ignored in the native phase |
| Docker PostgreSQL | 103 library tests and 38 real TCP E2E tests passed; Compose teardown completed |
| Formatting/lint | Workspace Rust formatting, strict API/domain/downstream checks and pinned GraphQL formatting passed |
| Shared contracts / WASM | 3 domain contracts passed; domain WASM and backoffice native/WASM checks passed |
| Downstream tests | Backoffice 6 tests passed; client-ui check/test passed with no tests |
| Postman | All 11 external GraphQL documents match their requests and variable sets; external11/internal-products2 lint passed with zero errors/warnings |
| Formatter behavior | Misformatted GraphQL rejected, write-format corrected it, second write identical |
| Independent review | Inventory child-failure coverage restored; removing the product-parent guard reproduced partial success, restored guard passed the regression |
| Published-head CI | Confirm separately against the pushed SHA in PR checks |

Product E2E uses real API TCP endpoints and Docker PostgreSQL with the explicitly approved synthetic Shopify HTTP upstream. No live Shopify requests or mutations ran.

## Exercised contracts

| Area | Evidence |
| --- | --- |
| Client transport | Typed JSON/GraphQL decoding, error/partial-data handling, timeout, redirect rejection and original failure causes |
| Retry policy | HTTP/GraphQL rejection shares a four-attempt budget; unchanged replay payloads, Retry-After bounds and no ambiguous timeout/5xx replay |
| Shopify proxy boundary | Explicit size/cursor forwarded; one operation per method; returned root/child metadata without automatic traversal; raw inventory/mutation outputs retained |
| Source models | Inventory and tagged-line conversion, SKU Unicode characters4–9, short/missing SKU, active levels, zero/negative stock and nullable metadata |
| Exact report values | Normalized prices, scientific weights, exact scaled/maximum totals, compatible metadata comparison and checked quantity/total overflow |
| Service pagination | Selected root page only; complete child variants/lines; missing/mismatched parents and missing/repeated/cyclic cursors fail without partial output |
| API pagination | page_size bounds/default, cursor validation, first/second product/order pages, root continuation metadata, empty page with continuation and page-local aggregation |
| Tagged orders | Literal search escaping, nested initial lines, no unnecessary child request, compatible rows merged in first-seen order and missing SKUs kept separate |
| Dependency lifecycle | Shared AppState client Arc, optional/invalid initialization retained; affected requests map to sanitized503 |
| Report failures | Real TCP400/401/404/502/503 scenarios; transport504/non-timeout502 regression; ordinary logs/public bodies sanitized, original causes available through DEBUG diagnostics |
| Existing API | Authentication, authorization, cookies/origins/CORS, refresh-family transactions, database operations and registered routes remain covered by the complete suite |

## Limits

| Area | Limit |
| --- | --- |
| Pagination unit | page_size counts products/orders, not output rows; selected parents can have many child pages |
| Aggregation | ORDER_TAG totals are page-local; complete reports require combining compatible rows across pages |
| Live Shopify | Schema compatibility, store permissions and actual requested query cost were not executed against Shopify |
| Concurrent changes | No snapshot isolation; parent/child failures are rejected, but complete multi-page reports can reflect changing store data |
| Performance | SKU indexing narrows comparisons; many incompatible groups for one SKU still scan that SKU's candidates; no load/capacity benchmark |
| Browser/deployment | No new rendered browser E2E, production deployment or merge |
| Coverage percentage | Not measured |
