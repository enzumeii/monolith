# Verification

Results describe the tested revision and scope; they do not establish live Shopify behavior or production capacity. [Architecture](architecture.md) and [operations](development.md) describe the current system separately from these results.

## Verified revision and checks

| Evidence | Result |
| --- | --- |
| Implementation revision | Review corrections following `d795386`; exact published head is recorded in PR checks |
| Local native API tests | `cargo test --locked -p api --lib`: 84 passed, 0 failed, 2 pre-existing Docker-dependent tests ignored |
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
| Configuration | Literal ConfigBuilder token override and standard file/env precedence; no construction helper or skipped token field; URL retained through overlays; optional absent section and ordinary missing-URL errors; redacted Debug |
| Money and IDs | Decimal strings paired with currency; malformed/wrong-resource IDs rejected |
| Client boundary | Nine aligned operation/document names; queries automatically collect every page without caller cursors; root and child methods remain independent; raw inventory and mutation userErrors/optional results retained |
| Catalog pagination | Multiple products per page and multiple outer pages; first 250 variants per product; nested hasNextPage retained without overflow error or variant cursor |
| Order pagination | Header queries traverse all outer pages independently; line methods return501 ordered lines over250/250/1 pages; no root/line chaining; date offsets/status/test/cancellation retained |
| Tagged orders | Multiple order pages preserve response order; empty results, literal tag escaping, blank-input rejection, cursor failures, HTTP/GraphQL failure propagation, returned tags, precise shop money and currency; independent line lookup collects501 nodes; each query call starts without a cursor |
| Error source chain | Origin exposes the original ClientError through standard Error::source, retaining deeper reqwest/serde causes and the first client identity; focused regression reproduced the derive-generated box downcast defect and passes with explicit chaining |
| Error integration | Each original ClientError family retains its type/source when wrapped with the Shopify origin; real constructor/HTTP/decode/protocol failures log one ERROR under INFO/DEBUG at response/background boundaries; conversion/attachment silent, payloads private |
| Cursor safety | Missing, repeated, and cyclic continuations rejected |
| Payment/tag behavior | Search-literal escaping for payment ID and single-tag lookup, multiple matches, empty tag replacement, mutation errors |
| Inventory discovery | All location pages returned unfiltered, including active/untracked/zero/positive stock; missing location errors |
| Activation | Quantity arguments absent; available/on-hand retained in fixtures; stable idempotency key across throttling |
| Existing API | CI exercises authentication, authorization, cookie/origin handling, database operations, and registered endpoint tests |

## Review and limits

| Area | Result / limit |
| --- | --- |
| Independent implementation review | Read-only final review found no actionable defects after the source-chain repair; earlier configuration-overlay and typed-GraphQL-error regressions remain covered |
| Additional fix-pass check | Oversized Retry-After reproduced a duration overflow; regression passes after budget validation precedes arithmetic |
| Deferred test minor | Generic HTTP fixture checks headers/body but does not explicitly assert the HTTP method |
| Shopify compatibility | Official docs and local fixtures checked; no live schema/request execution; nested 250-by-250 query cost unverified against the single-query maximum |
| Platform pagination | Client automatically follows each operation's outer/line cursors; product variants intentionally stop at first250 with pageInfo retained |
| Concurrent changes | No snapshot isolation; missing/mismatched order and child-page failures return errors rather than partial success. Product variants remain intentionally bounded |
| Browser behavior | No new browser verification, rendered E2E, or HTTPS cookie acceptance evidence |
| Coverage percentage | Not measured; no percentage claim |
| Performance/deployment | No load test, production capacity result, deployment, or merge |
| Postman preservation | Existing Postman commit is preserved; no Postman edits or new Postman checks in this change |
