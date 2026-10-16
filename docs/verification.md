# Verification

Results describe the tested revision and scope; they do not establish live Shopify behavior or production capacity. [Architecture](architecture.md) and [operations](development.md) describe the current system separately from these results.

## Verified revision and checks

| Evidence | Result |
| --- | --- |
| Implementation revision | `6e47148d9689989cdec73fc6fba6b5692ebf9102` |
| Local native API tests | `just test-native`: 66 passed, 0 failed, 2 pre-existing Docker-dependent tests ignored |
| Local API format/lint | `just check`: passed, including strict Clippy for all API targets |
| Local workspace formatting | `cargo fmt --all -- --check`: passed |
| Published implementation CI | [CI run 38902076195](https://github.com/enzumeii/monolith/actions/runs/38902076195): success for the implementation revision |
| CI API native phase | 66 passed, 2 ignored |
| CI PostgreSQL library phase | 68 passed, 0 ignored |
| CI real TCP endpoints | 36 passed |
| CI workspace formatting | Passed |
| Backoffice/client UI checks | Not run locally for this change; CI skipped their unchanged-package steps |
| Live Shopify execution | Not run; no store mutations performed |
| Documentation-only follow-up | Reconciles complete documents and excludes planning artifacts; its own commit checks are reported on the PR |

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
| Order pagination | Multiple orders and line-item pages; date offsets and exclusive boundary; test/cancelled/unpaid statuses retained |
| Cursor safety | Missing, repeated, and cyclic continuations rejected |
| Payment/tag behavior | Search-literal escaping, multiple matches, empty tag replacement, mutation errors |
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
| Platform pagination | Client follows available cursors; platform access/pagination caps still apply |
| Concurrent changes | No snapshot isolation; parent identity changes during nested pagination fail explicitly |
| Browser behavior | No new browser verification, rendered E2E, or HTTPS cookie acceptance evidence |
| Coverage percentage | Not measured; no percentage claim |
| Performance/deployment | No load test, production capacity result, deployment, or merge |
| Postman preservation | Copied tracked/untracked workspace files match the original checkout byte for byte and remain uncommitted |
