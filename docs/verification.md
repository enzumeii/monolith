# Verification

Results describe the tested revision and scope; they do not establish live Shopify behavior or production capacity. [Architecture](architecture.md) and [operations](development.md) describe the current system separately from these results.

## Verified revision and checks

| Evidence | Result |
| --- | --- |
| Implementation revision | `34adeb5` (orders-by-tag code and tests) |
| Local native API tests | `just test-native`: 72 passed, 0 failed, 2 pre-existing Docker-dependent tests ignored |
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
| Backoff | HTTP 429 and GraphQL throttling; unchanged replay payload; four-attempt exhaustion; Retry-After minimum and budget; oversized-delay overflow regression |
| No replay | HTTP 500/502/503/504, unrelated GraphQL errors, mutation user errors, and ambiguous timeout |
| Configuration | dotenv/environment token loading in config.rs, secret-safe Debug, TOML token rejection, configured URL preserved through local overlay |
| Money and IDs | Decimal strings paired with currency; malformed/wrong-resource IDs rejected |
| Catalog pagination | Multiple products with multiple variant pages; nested cursors reset between products |
| Order pagination | Sales traverses orders and nested line-item pages; date offsets and exclusive boundary; test/cancelled/unpaid statuses retained |
| Tagged orders | Multiple order pages preserve response order; empty results, literal tag escaping, blank-input rejection, cursor failures, HTTP/GraphQL failure propagation, precise presentment money and currency; 100 line items returned without nested fetches |
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
| Shopify compatibility | Official documentation and local contract fixtures were checked; no real shop schema/request execution |
| Platform pagination | Client follows available cursors; platform access/pagination caps still apply; tag lookup intentionally returns at most 100 line items per order and cannot detect overflow |
| Concurrent changes | No snapshot isolation; parent identity changes during nested pagination fail explicitly |
| Browser behavior | No new browser verification, rendered E2E, or HTTPS cookie acceptance evidence |
| Coverage percentage | Not measured; no percentage claim |
| Performance/deployment | No load test, production capacity result, deployment, or merge |
| Postman preservation | User-owned collection changes are preserved in `2f9eaae`; tag lookup does not modify Postman files |
