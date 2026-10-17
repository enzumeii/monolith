# Verification

Results describe the tested revision and scope; they do not establish live Shopify behavior or production capacity. [Architecture](architecture.md) and [operations](development.md) describe the current system separately from these results.

## Verified revision and checks

| Evidence | Result |
| --- | --- |
| Implementation revision | Review corrections following `25ac174`; exact published head is recorded in PR checks |
| Local native API tests | `cargo test --locked -p api --lib`: 89 passed, 0 failed, 2 pre-existing Docker-dependent tests ignored |
| Local API format/lint | `just check`: passed, including strict Clippy for all API targets |
| Local workspace formatting | `cargo fmt --all -- --check`: passed |
| Current published CI | Exact published-head results are recorded in [PR #6 checks](https://github.com/enzumeii/monolith/pull/6/checks); local checks do not establish a CI pass |
| Local PostgreSQL / real TCP endpoints | Not run for this change; unchanged CI executes its existing database and endpoint suite |
| Backoffice/client UI checks | Not run locally for this change; CI skipped their unchanged-package steps |
| Live Shopify execution | Not run; no store mutations performed |

The existing CI workflow runs API E2E even though this client change required no new E2E tests or local E2E execution. Local Shopify fixtures exercise HTTP over loopback and are not described as real Shopify E2E.

## Exercised contracts

| Area | Evidence |
| --- | --- |
| JSON transport | Typed decoding, generic headers/body, malformed JSON, HTTP failures, timeout, redirect rejection |
| GraphQL errors | Errors classified before typed data decoding; null mutation results retain upstream errors/metadata; partial-data errors rejected |
| Backoff | HTTP 4xx and GraphQL throttling use one HttpClient loop/budget; unchanged replay payload; mixed-rejection four-attempt exhaustion; Retry-After minimum and budget; oversized-delay overflow regression |
| No replay | HTTP 500/502/503/504, unrelated GraphQL errors, mutation user errors, and ambiguous timeout |
| Configuration | Immutable runtime settings with direct environment token injection, TOML token rejection, optional Shopify configuration with one conditional token assignment, explicit caller construction, redacted Debug, configured URL preserved through overlays |
| Money and IDs | Decimal strings paired with currency; malformed/wrong-resource IDs rejected |
| Proxy boundary | Nine operation-named methods/documents; root and child requests independent; supplied cursors forwarded, pageInfo/raw parent IDs retained; unfiltered inventory and mutation userErrors/optional results returned |
| Catalog pagination | Multiple products per page and multiple outer pages; first 250 variants per product; nested hasNextPage retained without overflow error or variant cursor |
| Order pagination | Sales service traverses all outer order pages with multiple parents per page; 501 ordered lines across three 250-node child pages; independent parent cursors; quoted date offsets and exclusive end; statuses/test/cancellation fields retained |
| Tagged orders | Multiple order pages preserve response order; empty results, literal tag escaping, blank-input rejection, cursor failures, HTTP/GraphQL failure propagation, returned tags, precise shop money and currency; 501 ordered line items collected across 250-node child requests; second parent starts without a cursor |
| Error integration | All eight ClientError families preserve typed source/origin and emit exactly one ERROR at response/background boundaries under INFO/DEBUG; conversion is silent and fixture payloads remain private |
| Setup/Postman | Isolated setup preserves existing .env and scaffolds an empty token once; five edited YAML requests parse and preserve non-query data |
| Cursor safety | Missing, repeated, and cyclic continuations rejected |
| Payment/tag behavior | Search-literal escaping for payment ID and single-tag lookup, multiple matches, empty tag replacement, mutation errors |
| Inventory discovery | All location pages; include only tracked inactive positive-on-hand levels; missing quantities fail |
| Activation | Quantity arguments absent; available/on-hand retained in fixtures; stable idempotency key across throttling |
| Existing API | CI exercises authentication, authorization, cookie/origin handling, database operations, and registered endpoint tests |

## Review and limits

| Area | Result / limit |
| --- | --- |
| Independent implementation review | Read-only final review found no actionable defects in proxy/service separation, constructor/configuration, raw DTOs, tests or current architecture docs; earlier regressions remain covered |
| Additional fix-pass check | Oversized Retry-After reproduced a duration overflow; regression passes after budget validation precedes arithmetic |
| Deferred test minor | Generic HTTP fixture checks headers/body but does not explicitly assert the HTTP method |
| Shopify compatibility | Official docs and local fixtures checked; no live schema/request execution; nested 250-by-250 query cost unverified against the single-query maximum |
| Platform pagination | Service follows outer cursors and all sales/tag child cursors; each client proxy returns one page; product variants intentionally stop at first:250 with pageInfo retained |
| Concurrent changes | No snapshot isolation; missing/mismatched order and child-page failures return errors rather than partial success. Product variants remain intentionally bounded |
| Browser behavior | No new browser verification, rendered E2E, or HTTPS cookie acceptance evidence |
| Coverage percentage | Not measured; no percentage claim |
| Performance/deployment | No load test, production capacity result, deployment, or merge |
| Postman preservation | User-owned collection changes are preserved in `06a8738`; review edits only corresponding query bodies; non-query data preserved |
