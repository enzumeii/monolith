# Scheduled work

## Ownership and endpoints

```mermaid
flowchart TD
  A["GET/POST /v1/admin/cron/configs<br/>GET/PATCH/DELETE /{id}<br/>GET /preview"] --> H["Handlers → validations"]
  B["GET /v1/admin/cron/reports<br/>GET /{id}<br/>GET /{id}/files/{filename}/download-url"] --> H
  H --> C["services/cron/config.rs"]
  H --> R["services/cron/report.rs"]
  TRIGGER["Vercel: daily authenticated GET /_cron"] --> VERCEL["VercelCron: bounded awaited tick"]
  VPS["VPS: periodic scheduler"] --> VPSCRON["VpsCron: normal tick"]
  VERCEL --> REC["cron/checkout.rs<br/>bounded checkout recovery first"]
  REC --> W["Report dispatch · leases · retries · executor"]
  VPS --> REC
  REC --> CHECKOUT["services/checkout.rs<br/>claimed business advancement"]
  CHECKOUT --> CHECKOUTDB["repositories/checkout.rs<br/>checkouts · fenced persistence"]
  CHECKOUT --> PAYMENTS["Shopify · Stripe · PayPal clients"]
  VPSCRON --> W
  W --> J["cron/job.rs<br/>claim · checkpoint · delivery"]
  W --> C
  J --> R
  J --> E["services/cron/email.rs<br/>pure report email content"]
  J --> RC["ResendClient"]
  R --> P["Provider clients + pagination"]
  J --> G["report-generator<br/>shared tables + CSV"]
  G --> UI["Backoffice manual reports"]
  C --> CR["repositories/cron_configurations.rs"]
  J --> JR["repositories/cron_jobs.rs"]
  J --> AR["repositories/report_results.rs"]
  R --> AR
  JR --> FR["repositories/report_result_files.rs"]
  AR --> FR
  J --> S["cron/publication.rs<br/>bounded bytes · checksum · publication"]
  S --> LB["client/s3.rs<br/>SigV4 PUT · HEAD · bounded GET"]
  LB --> V["Private reports bucket<br/>RustFS locally · Supabase in production"]
  H --> SIGN["Validate saved receipt · HEAD · presign"]
  SIGN --> V
  SIGN --> U["Fresh short-lived attachment URL · no-store"]
  U --> Browser["Admin browser → storage host"]
```

| Contract | Meaning |
| --- | --- |
| Invocation | Reports share their existing queue/executor across VPS and Vercel. VPS report polling is every 10 seconds; checkout recovery has an independent 60-second wake-up and auth retention runs hourly. Vercel uses one authenticated daily UTC wake-up with a 240-second budget and five-second retention reserve. Report schedules use Bangkok time; checkout eligibility uses UTC instants. |
| Config parameter | Transaction recurrence policy, or decimal inventory location ID. |
| Report parameters | Resolved Bangkok range, or inventory ID. Range text is converted to a half-open interval internally. |
| Status | One PENDING/SUCCESS/FAILED value per job/audit. SUCCESS requires generation, email provider acceptance and publication. |
| Error | Latest safe failure names the step; unconfirmed mail may have been sent. |
| Files | Each verified receipt appears immediately, even if another file or email fails. History exposes filename/content_type only; an authenticated click requests a fresh private S3 download URL. |
| Formats | Scheduled output is CSV only. Backoffice manual XLSX remains a separate wrapper over shared projected rows. |
| Config edits | Every edit starts a future revision; undispatched older backlog is discarded. Unstarted jobs use current configuration, started reports retain frozen context. |
| Deletion | Cascade deletes jobs and cancels incomplete audits. Independent audits/published files remain. In-flight external sends cannot be recalled. |

## Cron implementation selection

| Cron contract | Behavior |
| --- | --- |
| Configuration | One existing typed reader composes application.toml and the local/prod overlay with trusted metadata and secret overrides; local configuration can select either cron implementation, while production selects Vercel |
| VPS lifecycle | Startup owns independent report, checkout recovery and retention task handles. Graceful shutdown signals checkout cancellation, cancels its owned work and awaits all tasks before closing the database |
| Vercel lifecycle | No background loops; Vercel-only `GET /_cron` awaits checkout recovery first, reports while work time remains, then refresh-token retention within its reserved time. Recovery workers are awaited before the response |
| Disabled reports | `scheduler.enabled = false` skips report dispatch/drain. Checkout recovery still runs when `[checkout]` exists and `checkout_recovery.enabled = true`; refresh-token retention continues |
| Settings | scheduler.implementation selects vps/vercel; poll_interval_secs, retention_interval_secs, invocation_budget_secs and retention_reserve_secs remain in the existing scheduler configuration; no extra application runtime/profile files |
| Capacity | No new claims after deadline; owned async work is cancelled/joined. Partial provider acquisition is not resumable; repeated oversized jobs can exhaust attempts |

## Checkout recovery

```mermaid
flowchart TD
  DUE["Oldest due checkout · initial 60-second grace"] --> CLAIM["Atomic SKIP LOCKED claim · renewable lease"]
  CLAIM --> LIMIT{"Abandoned claim or failure cap?"}
  LIMIT -->|"cap reached"| MANUAL["Manual review · no next due time"]
  LIMIT -->|"abandoned"| BACKOFF["Count once · bounded backoff"]
  LIMIT -->|"ready"| ADVANCE["Verify draft and payment evidence<br/>advance original attempt"]
  ADVANCE -->|"pending or actionable"| OBSERVE["Reset failures · check after wake interval"]
  ADVANCE -->|"transient read or uncertain result"| BACKOFF
  ADVANCE -->|"conflicting evidence or unsafe mutation"| MANUAL
  ADVANCE -->|"completed or terminal unpaid"| STOP["Clear recovery metadata · stop polling"]
  BACKOFF --> DUE
  OBSERVE --> DUE
```

| Concern | Contract |
| --- | --- |
| Ownership | `cron/checkout.rs` owns dispatch, deadlines, renewal, backoff and cancellation. The checkout service owns business advancement. The checkout repository owns SQL claims and persistence. Recovery uses existing `checkouts`; it creates no report job or separate payment queue |
| Selection | Claim oldest `(next_recovery_at, id)` among due nonmanual, nonterminal rows with no live owner. `FOR UPDATE SKIP LOCKED` permits concurrent invocations without claiming the same row. New rows become due after 60 seconds |
| Capacity | At most `batch_size` claim attempts per pass and `concurrency` active workers; no claims queued behind waiting tasks. The lane stops at the earlier of its own budget and the invocation work deadline |
| Lease | Worker lease renews every third of `lease_secs`. Save, renewal, finish and release require the matching token and an unexpired lease checked after acquiring the row lock. Loss cancels business work. Already-transmitted writes remain uncertain and require reconciliation. Public create/verify/callback claims retain their ten-minute lease, so recovery can wait that long after a public request crashes |
| Outcomes | A successful pending/actionable observation resets consecutive failures. Retry doubles from 60 seconds to at most 900 seconds. An abandoned expired claim counts once before backoff. Twelve consecutive failures stop automatic attempts by default; normal waiting does not consume this budget |
| Manual state | Contradictory payment/order evidence, missing original purchase, ambiguous draft lookup, unsafe reservation or expired mutation window can require operator review. Read failures and missing completion readback retry within the failure budget. `recovery_manual` pauses the worker; it does not establish unpaid status |
| Identity and mutation | Keep the original resource, key, return URLs and request fingerprint. Same-key gateway creation is bounded to 23 hours for Stripe and five hours for PayPal. PayPal capture requires approval, a safe hold and the original five-hour window. Never mint another payment key, reconstruct purchase details from hashes or repeat ambiguous Shopify completion |
| Lost Shopify response | Recover a draft only through a unique exact-tag/evidence match. After an ambiguous completion, reconcile the linked draft/order without another completion mutation. Read known payment truth despite unsafe/expired holds; an already-linked paid Shopify order can reconcile after expiry |
| Guest expiry | Internal recovery uses persisted trusted identity without a guest bearer or extending its lifetime. Public guest authentication remains unchanged. The worker does not create browser redirects or hydrate customer response details |
| Terminal and later evidence | Completed or authoritative unpaid state clears retry/manual metadata and stops polling. Later authenticated advancement can supersede unpaid evidence; a save back to active state rearms recovery after 60 seconds with a fresh failure count |
| Privacy | Recovery metadata contains due time, consecutive failure count, a safe typed code and a manual flag. Worker logs contain checkout UUID, provider, states and safe codes; no email, guest secrets, request/response payloads or raw provider errors |
| HTTP outcome | `/_cron` returns 204 when work and retention finish, including a business retry/manual outcome saved durably. Checkout claim, persistence, lease or capacity failure returns safe 503 after the other invocation lanes and reserved retention are attempted. Existing report/retention errors retain their own mappings |
| Deployment cadence | Checked-in Vercel Hobby cron remains daily. Frequent Vercel recovery requires an owner-configured authenticated external trigger of `/_cron`; this change does not configure that service or deploy it. VPS has its own configured wake interval |

Non-sensitive values belong in the existing base `application.toml` and typed reader; secrets remain in the private environment.

| `[checkout_recovery]` setting | Default | Valid range / behavior |
| --- | --- | --- |
| `enabled` | `true` | Effective only when `[checkout]` is configured; independent of `scheduler.enabled` |
| `wake_secs` | `60` | 1–86400; VPS wake interval and delay after a successful observation |
| `batch_size` | `10` | 1–100 claim attempts per pass |
| `concurrency` | `2` | 1–32 and no greater than `batch_size` |
| `budget_secs` | `60` | 1–240; also clamped by the shared invocation work deadline |
| `lease_secs` | `90` | 1–86400; renewable worker ownership |
| `retry_backoff_secs` | `60` | 1–86400; first retry delay |
| `max_retry_backoff_secs` | `900` | At least `retry_backoff_secs`, at most 86400 |
| `max_failures` | `12` | 1–100 consecutive failures; reaching the cap atomically marks manual review |

Read-only inspection can identify due work and manual cases without reading purchase data or credentials:

```sql
SELECT id,
       provider,
       draft_state,
       payment_state,
       completion_state,
       next_recovery_at,
       recovery_failures,
       recovery_code,
       recovery_manual,
       claim_expires_at
FROM checkouts
WHERE recovery_manual
   OR next_recovery_at IS NOT NULL
ORDER BY next_recovery_at NULLS FIRST, id;
```

| Operator observation | Next action |
| --- | --- |
| Due row has no live claim | Confirm the selected runtime, recovery enablement and recent authenticated trigger; inspect safe outcome codes |
| `claim_abandoned` or transient failure | Check dependency availability and next due time; preserve the original checkout identity |
| `attempts_exhausted`, evidence conflict or unsafe mutation | Compare authoritative provider and Shopify records for that checkout before a support decision; do not clear flags or replay a mutation blindly |
| Customer has paid but browser credential expired | Recover through authenticated provider/Shopify evidence and support; never instruct another payment solely because public verification returns 401 |

There is no automatic refund, operator reset endpoint or new administration UI. Verification results and actual provider-delivery limits belong in [verification](verification.md).

## Report database fields

```mermaid
erDiagram
  cron_configurations ||--o{ cron_jobs : "ON DELETE CASCADE"
  cron_configurations {
    UUID id PK
    TEXT name
    TEXT report_type
    TEXT cron_expression
    TEXT receiver_email
    TEXT parameter
    BOOLEAN enabled
    TIMESTAMP created_at
    TIMESTAMP updated_at
  }
  cron_jobs {
    UUID id PK
    UUID cron_configuration_id FK
    TIMESTAMP scheduled_for
    TEXT status
    INTEGER attempts
    TEXT error
    TIMESTAMP published_at
    UUID lease_token
    TIMESTAMP lease_until
    JSONB execution_checkpoint
  }
  report_results ||--o{ report_result_files : "ON DELETE CASCADE"
  report_results {
    UUID id PK "run ID by convention; no FK"
    TEXT report_type
    TIMESTAMP attempted_at
    TIMESTAMP generated_at
    TEXT status
    TEXT error
    TEXT parameters
  }
  report_result_files {
    UUID report_result_id PK,FK
    TEXT filename PK
    INTEGER position UK
    TEXT content_type
    TEXT bucket UK
    TEXT storage_key UK
    BIGINT size_bytes
    TEXT sha256
  }
```

The tables contain 9 configuration fields, 10 job fields, 7 audit fields and 8 receipt fields. All SQL timestamps are UTC-normalized TIMESTAMP WITHOUT TIME ZONE. Receipt constraints require nonempty metadata, nonnegative position/size and 64 lowercase hexadecimal hash digits. Report/filename, report/position and bucket/key identities are unique.

| Table | Field | Usage |
| --- | --- | --- |
| cron_configurations | id | Configuration identity. |
| | name | Validated admin label. |
| | report_type | TRANSACTION or INVENTORY, validated by Rust. |
| | cron_expression | Five-field Bangkok recurrence. |
| | receiver_email | Current recipient. |
| | parameter | PREVIOUS_MONTH/FIRST_HALF_MONTH or inventory ID. |
| | enabled | Controls future dispatch. |
| | created_at | Creation time. |
| | updated_at | Edit time and schedule revision boundary; advances for conditional updates. |
| cron_jobs | id | Run identity, audit/file namespace and idempotency key. |
| | cron_configuration_id | Configuration FK, ON DELETE CASCADE. |
| | scheduled_for | Occurrence; unique with configuration ID. |
| | status | Whole workflow status. |
| | attempts | One total claim/attempt budget. |
| | error | Safe failed-step description. |
| | published_at | Whole workflow completion after publication and email acceptance. |
| | lease_token | Ownership fence. |
| | lease_until | Active expiry or token-free retry eligibility; null on terminal failure. |
| | execution_checkpoint | Nullable private metadata: frozen generation date/keys, expected sizes/SHA-256, verified receipts and frozen sending/accepted mail intent. No CSV bytes; deleted with the job and excluded from API DTOs. |
| report_results | id | Independent audit; no configuration/job FK. |
| | report_type | Actual type frozen at first claim. |
| | attempted_at | First actual attempt. |
| | generated_at | Generation completion, nullable even on failed workflow. |
| | status | Overall outcome. |
| | error | Current safe failure/uncertainty/cancellation message. |
| | parameters | Frozen range or inventory ID. |
| report_result_files | report_result_id, filename | Canonical immutable receipt; parent deletion cascades its receipts. No URL column. |
| | position | Frozen file order: inventory.csv / transactions.csv = 0, items.csv = 1, even when items publishes first. |
| | content_type, bucket, storage_key, size_bytes, sha256 | Verified private object metadata; exact retries are idempotent and conflicting receipts are rejected. |
| History reads | list/detail | One SQL snapshot pages audit parents before joining ordered receipts; empty audits remain visible. |

## Retry and storage

```mermaid
flowchart LR
  Q["Claim + increment attempts"] --> F["Generate or reuse saved CSVs"]
  F --> P["Upload/verify CSVs<br/>save each receipt + audit metadata"]
  P --> M["Only after every file is durable:<br/>freeze/send mail or reuse acceptance"]
  M --> D["Atomic job/audit SUCCESS"]
  F --> X["Any failure → FAILED + error"]
  M --> X
  P --> X
  X --> L["Bounded backoff in lease_until"]
  L --> Q
```

| Concern | Mechanism |
| --- | --- |
| Deduplication | Configuration locking and UNIQUE(configuration, scheduled_for). |
| Upcoming execution | Calculated by GET preview; no stored next_run_at. |
| Restart | Durable job metadata and verified S3 bytes; no report filesystem persistence. |
| Accepted email | Reused without resending. Unconfirmed sends retry with the same request/key only inside the provider's 24-hour window. |
| Private state | cron_jobs.execution_checkpoint JSONB stores metadata only. Lease/token/PENDING/unexpired checks and compare-and-update fence every transition. Audit generation date and canonical child receipts commit in the same transaction after checkpoint compare-and-update; completion verifies these receipts before SUCCESS. |
| Object identity | Keys use `type/Bangkok-YYYYMMDD/report-UUID/sha256/filename`. The concrete AWS S3 SDK client uses the same operations locally and in production. Validate report/bucket/key/hash before storage I/O. |
| S3 protocol | SigV4 HEAD/PUT/GET; bounded reads/deadlines and one SDK attempt. PUT is an upsert; publication permits replacements only with identical frozen bytes at the same hash-bearing key. |
| Recovery | Expected keys, sizes and SHA-256 reconcile accepted uploads after a lost response. A process crash before all CSV uploads finish can permanently lose missing bytes: mark failure, retain available receipts and never regenerate or email an incomplete report. Transport uncertainty consumes the ordinary whole-job retry budget. |
| Scheduled email | Verified CSV bytes remain attachments; email does not depend on expiring links. New intents use reporting@enzumeii.com with report type, frozen inventory ID or inclusive Bangkok range, and saved generation timestamp in subject and HTML. Generation timestamps use Asia/Bangkok (UTC+07:00). Existing intents retain sender, recipient, content, time and idempotency key. |
| Authorization | History/detail/signing require existing admin authentication. The `reports` bucket stays private. Signing validates saved metadata and HEAD, then returns URL plus UTC expires_at with Cache-Control no-store. A URL is a temporary bearer credential: anyone holding it can download until expiry; never persist or log it. |
| Integrity | Checksums during prepare/recovery/publication. File hosting after publication belongs to storage. |

## Local commands

| Command / setting | Local behavior |
| --- | --- |
| API `just setup` | Fill missing/empty S3_ACCESS_KEY_ID and S3_SECRET_ACCESS_KEY with synthetic local defaults; preserve all nonempty values and existing secrets. |
| API `just run` | Start Compose PostgreSQL/local-s3, initialize private reports bucket, migrate the local database and run API; stop only owned S3 on exit. |
| Root `just vercel-dev` | Load local API settings, start/init S3 independently, prebuild UI bundles and run existing Vercel services; stop only owned S3 on exit. Optional `--port`, `--queue-port`, `--runtime-cache-port`, `--schedules-port` preserve defaults 18027/55461/55462/55463. |
| Ordinary restart | Pinned RustFS1.0.3 and local_s3_data persist across stop/start and `just db-down`; do not delete development volumes. |
| Local settings | Existing application.local.toml `client.s3.endpoint` must match LOCAL_S3_PORT (default3001); common region ap-southeast-1 and bucket reports. Keep S3 credentials separate from DATABASE_URL/JWT_SECRET. |
| API `just test` | Dedicated Compose PostgreSQL/local-s3/init → native Cargo once → registered TCP suite → owned Compose down -v, including failure. |
| Test isolation | Unique E2E_COMPOSE_PROJECT, E2E_POSTGRES_PORT, E2E_S3_PORT and matching E2E_DATABASE_URL; dedicated E2E_S3_ACCESS_KEY_ID/E2E_S3_SECRET_ACCESS_KEY default to fixed synthetic credentials, never production S3 keys. |
| Cron selection | The selected application.local.toml or application.prod.toml chooses VPS polling or Vercel cron; the checked-in production overlay selects Vercel. |
| Postman | The three external S3 requests exercise SigV4 PUT/HEAD/GET; supply credentials privately through environment values. Internal reports/download-url requests a fresh signed URL under the authenticated no-store contract; fetching that URL is a separate manual step. |

| Production readiness | Boundary |
| --- | --- |
| Target | Supabase S3 endpoint is checked into common/prod TOML, region ap-southeast-1, bucket reports, path-style addressing. |
| Private access | An authorized operator must separately create/confirm private reports bucket and supply S3_ACCESS_KEY_ID/S3_SECRET_ACCESS_KEY through Vercel secrets. Database passwords/API keys are not S3 access keys. |
| Evidence | Local RustFS and controlled network fixtures verify application behavior. Dashboard privacy, actual Supabase credentials/authorization, provider compatibility, smoke and deployment require separately authorized live verification. |
| Existing objects | No legacy copy is required. Missing checkpointed objects fail safely; regeneration is disabled. Existing user containers/data are untouched. |
