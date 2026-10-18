# Verification

Results describe this revision's exercised scope, separately from [architecture](architecture.md) and [operations](development.md). They do not establish live Shopify behavior or production capacity.

## Checks

| Evidence | Result |
| --- | --- |
| Native API tests | 118 passed; 2 Docker-dependent tests ignored in the native phase |
| Docker PostgreSQL | 120 library tests and 42 real TCP E2E tests passed; Compose teardown completed |
| Formatting/lint | Workspace Rust formatting, strict API/domain/downstream checks and Cargo-installed GraphQL/JSON formatting passed |
| Shared contracts / WASM | 5 domain contracts passed; domain WASM and backoffice native/WASM checks passed |
| Downstream tests | Backoffice 6 tests passed; client-ui check/test passed with no tests |
| Postman | All 9 external GraphQL documents match their requests and variable sets; external9/internal-v1(11) lint passed with zero errors/warnings |
| Formatter behavior | Cargo-installed dprint 0.63.0 with pinned Rust-based plugins checks 9 GraphQL and 12 JSON files. Dirty GraphQL/JSON rejected; write-format corrected both; repeated writes identical. Malformed input rejected unchanged; service-local tool/cache files excluded. SQLx JSON values and GraphQL tokens unchanged; Postman queries synchronized. Existing just fix passed against Docker PostgreSQL, followed by just check |
| Split-contract regressions | Reproduced inventory overfetch and dropped page_info; inventory returns one page and orders retain full totals. Later-page order overflow/invalid amounts reject the summary |
| Model cleanup | Removed unreferenced `SalesOrder`; API error encoding and backoffice decoding use shared `ErrorResponse`. Existing error/status/header regressions and the complete API suite passed; frontend native/WASM checks passed |
| Error simplification | Shared status/message error preserves typed causes, first origin, ordered contexts and sanitized diagnostics, including wrapped/shared client causes. Exact missing-location404 and missing-admin GET/PATCH/DELETE401 bodies and Bearer headers passed; timeout504/non-timeout502 exercises the production mapper. API check, workspace formatting and internal Postman lint passed |
| Published-head CI | Confirm separately against the pushed SHA in PR checks |

Product E2E uses real API TCP endpoints and Docker PostgreSQL with the explicitly approved synthetic Shopify HTTP upstream. No live Shopify requests or mutations ran.

## Exercised contracts

| Area | Evidence |
| --- | --- |
| Client transport | Typed JSON/GraphQL decoding, error/partial-data handling, timeout, redirect rejection and original failure causes |
| Retry policy | HTTP/GraphQL rejection shares a four-attempt budget; unchanged replay payloads, Retry-After bounds and no ambiguous timeout/5xx replay |
| Shopify proxy boundary | Explicit size/cursor forwarded; one operation per method; returned root metadata without automatic traversal; bounded variants/lines use fixed250; raw inventory/mutation outputs retained |
| Source models | Inventory and tagged-line conversion, SKU Unicode characters4–9, short/missing SKU, active levels, zero/negative stock and nullable metadata |
| Exact report values | Normalized prices, scientific weights, exact scaled/maximum totals, compatible metadata comparison and checked quantity/total overflow |
| Service pagination | Inventory forwards size/cursor and preserves one page; orders consume each root page before fetching the next. Missing/repeated/cyclic cursors and later failures reject the order report |
| API pagination | Inventory first and continuation HTTP calls return page_info, including empty pages; order report rejects pagination, keeps products-only global totals; old products endpoint404 |
| Tagged orders | Literal search escaping, bounded nested lines, no child request, compatible rows merged in first-seen order and missing SKUs kept separate |
| Dependency lifecycle | Shared AppState client Arc, optional/invalid initialization retained; services resolve availability and map affected requests to sanitized503 |
| Report failures | Real TCP400/401/404/502/503 scenarios; transport504/non-timeout502 regression; ordinary logs/public bodies sanitized, original causes available through DEBUG diagnostics |
| Existing API | Authentication, authorization, cookies/origins/CORS, refresh-family transactions, database operations and registered routes remain covered by the complete suite |

## Limits

| Area | Limit |
| --- | --- |
| Retrieval | Inventory: one caller-sized root product page (default50); size counts products, not flattened rows. Orders: exhaust batches250 and return one summary; no snapshot isolation |
| Nested workload bound | At most250 variants/order lines per parent is the agreed store assumption; nested hasNextPage=true returns502, preventing silent truncation |
| Live Shopify | Schema compatibility, store permissions and actual requested query cost were not executed against Shopify |
| Concurrent changes | No snapshot isolation; traversal/incomplete nested-data failures are rejected, but complete reports can reflect changing store data across Shopify pages |
| Performance | SKU indexing narrows comparisons; many incompatible groups for one SKU still scan that SKU's candidates; no load/capacity benchmark |
| Browser/deployment | No new rendered browser E2E, production deployment or merge |
| Coverage percentage | Not measured |
