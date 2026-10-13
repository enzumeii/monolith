# Verification — 2026-10-13

The approved implementation and later scope corrections were verified in the existing worktree.

| Check | Result |
|---|---|
| API native unit tests | 34 passed |
| API TCP E2E, local API + existing Docker PostgreSQL | 29 passed |
| PostgreSQL retention integration, owned temporary rows | 1 passed |
| Backoffice native behavior tests | 11 passed |
| Shared contract test and db-free native/WASM builds | Passed |
| Formatting and strict native/WASM Clippy | Passed |
| SQLx offline metadata against configured PostgreSQL | Passed |
| Dioxus fullstack client/server bundle | Passed |

The original admin and seed migrations are unchanged. The user-requested pre-production consolidation puts family and retention additions in the original refresh-token migration; both later draft files are removed. Original seed/index behavior remains intentional. E2E creates no databases and resets no shared records; it cleans up owned rows and trigger controls. Backoffice/browser E2E is excluded by user request.

Diagnostic LLVM line coverage included the instrumented local API process, native tests and inline test code. API services: admin 98.3%, auth 96.3%; cookies 91.7%, refresh coordinator 94.9%, pagination controller 97.7%. Overall measured source coverage was 72.3%; browser views/synchronization remain unmeasured by native tests. These figures are diagnostic rather than a release percentage gate.

A one-time debug workload accepted 8/8 concurrent logins (p50 478 ms, p95 941 ms); 15 mixed create/update/delete operations measured p50 7 ms and p95 235 ms. This is a local sample, not a production capacity claim. The existing one-connection pool was retained; the benchmark was removed from the permanent E2E suite.

Fresh independent review found one important fixture-cleanup issue: a pooled session advisory lock could survive a failed race test and block cleanup. The control connection is now detached so unwind closes it. The API suite and strict lint passed after the fix; no other important review findings were reported.

No production deployment, push or merge was performed. Keep the branch/worktree for user integration.
