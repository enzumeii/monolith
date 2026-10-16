# Verification

Results describe the tested revision and scope; they do not establish live Shopify behavior or production capacity. [Architecture](architecture.md) and [operations](development.md) describe the current system separately from these results.

## Verified revision and checks

| Evidence | Result |
| --- | --- |
| Implementation revision | Review corrections following `06a8738`; exact published head is recorded in PR checks |
| Local native API tests | `cargo test --locked -p api --lib`: 80 passed, 0 failed, 2 pre-existing Docker-dependent tests ignored |
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
| Configuration | Immutable settings from builder token overrides, TOML token rejection, optional Shopify configuration, state-owned lazy construction, redacted Debug, configured URL preserved through overlays |
| Money and IDs | Decimal strings paired with currency; malformed/wrong-resource IDs rejected |
| Catalog pagination | Multiple products per page and multiple outer pages; 250 variants per product; known overflow is a protocol error |
| Order pagination | Sales traverses all outer order pages with multiple parents per page; 250 lines per order; known overflow fails; quoted date offsets and exclusive end; statuses/test/cancellation fields retained |
| Tagged orders | Multiple order pages preserve response order; empty results, literal tag escaping, blank-input rejection, cursor failures, HTTP/GraphQL failure propagation, returned tags, precise shop money and currency; 250 line items returned without nested fetches |
| Error integration | ClientError converts to AppError with typed source and origin; public errors and INFO/DEBUG logs do not expose fixture customer payloads |
| Setup/Postman | Isolated setup preserves existing .env and scaffolds an empty token once; five edited YAML requests parse and preserve non-query data |
| Cursor safety | Missing, repeated, and cyclic continuations rejected |
| Payment/tag behavior | Search-literal escaping for payment ID and single-tag lookup, multiple matches, empty tag replacement, mutation errors |
| Inventory discovery | All location pages; include only tracked inactive positive-on-hand levels; missing quantities fail |
| Activation | Quantity arguments absent; available/on-hand retained in fixtures; stable idempotency key across throttling |
| Existing API | CI exercises authentication, authorization, cookie/origin handling, database operations, and registered endpoint tests |

## Review and limits

| Area | Result / limit |
| --- | --- |
| Independent implementation review | Configuration-overlay and typed-GraphQL-error defects reproduced and fixed with regression tests |
| Additional fix-pass check | Oversized Retry-After reproduced a duration overflow; regression passes after budget validation precedes arithmetic |
| Deferred test minor | Generic HTTP fixture checks headers/body but does not explicitly assert the HTTP method |
| Shopify compatibility | Official docs and local fixtures checked; no live schema/request execution; nested 250-by-250 query cost unverified against the single-query maximum |
| Platform pagination | Client follows outer cursors; nested connections use a store-specific 250-node bound, not a Shopify-wide limit; tag lookup cannot detect overflow |
| Concurrent changes | No snapshot isolation; catalog/sales known nested overflow fails rather than silently returning partial results |
| Browser behavior | No new browser verification, rendered E2E, or HTTPS cookie acceptance evidence |
| Coverage percentage | Not measured; no percentage claim |
| Performance/deployment | No load test, production capacity result, deployment, or merge |
| Postman preservation | User-owned collection changes are preserved in `06a8738`; review edits only corresponding query bodies; non-query data preserved |
