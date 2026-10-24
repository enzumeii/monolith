# Development and operations

Use each package's README and justfile for prerequisites and commands. [Architecture](architecture.md) describes ownership and behavior; [verification](verification.md) records the current evidence.

## Configuration

| Owner | Source / setting | Behavior |
| --- | --- | --- |
| API | Common `application.toml` → `application.{RUN_ENV}.toml`; RUN_ENV defaults to local | One existing typed reader; scheduler.implementation selects vps/vercel from the selected TOML overlay |
| API listener | TOML; platform `PORT` in container server mode | Container listener uses the platform port |
| API browser | Required nonempty `browser.allowed_origins` and `browser.insecure_local` in TOML | CORS and cookie mutations share exact origin membership; HTTPS is allowed; explicit loopback HTTP requires insecure_local |
| Database | `DATABASE_URL` from ignored `.env` or environment | Secret PostgreSQL connection URL |
| Convention logos | Existing private S3 client and bucket; no public bucket policy | Bounded admin PNG/JPEG upload; database stores an object location and responses return a short-lived signed image URL |
| Report storage | Common/local/qa/prod client.s3 TOML; S3_ACCESS_KEY_ID and S3_SECRET_ACCESS_KEY from private environment | Private reports bucket, region ap-southeast-1; independent of database credentials; local setup supplies synthetic missing/empty values. No signed URL persistence. |
| Signing key | `JWT_SECRET` from ignored `.env` or environment | Hex-encoded random key of at least 32 bytes |
| Shopify URL | `client.shopify.admin_api_url` in `services/api/application.toml` | Full `https://<shop>.myshopify.com/admin/api/2026-10/graphql.json`; local overlay does not replace it |
| Shopify token | `SHOPIFY_ACCESS_TOKEN` from ignored API `.env` or environment | ConfigBuilder overrides client.shopify.access_token before one deserialization; standard file values apply when there is no environment override; Debug redacts credentials |
| Public UI settings | Each app’s `application.toml`, `[api] base_url` | Default `http://localhost:3000/v1`; each typed reader loads one published asset once at startup |
| Local browser | Backoffice `http://localhost:8084`, customer UI `http://localhost:8085`, API `http://localhost:3000` | Same hostname is required for exact origins and SameSite cookies |
| Production browser | `browser.allowed_origins` in production TOML | Explicit `https://convention.enzumeii.com` and `https://enzumeii.vercel.app`; secure cookies are enforced; production hostname metadata is not used; browser calls use same-origin `/api/v1` |
| Preview browser | `VERCEL_ENV=preview` and validated generated `VERCEL_URL` | Replaces the configured list with only the preview HTTPS origin and enforces secure cookies |
| Vercel local browser | `VERCEL_ENV=development` and explicit loopback `VERCEL_URL` host:port | Replaces the configured list with this gateway origin; permits HTTP/insecure cookies only for `localhost`, `127.0.0.1` or `[::1]` with a nonzero port |
| Frontend environments | Each app’s base `application.toml` → selected override (default `application.prod.toml`) at build time | Both UIs recursively merge tables; override arrays/scalars replace base values. Publish one `application.toml`; production uses `/api/v1` and backoffice inherits its base catalogue location. Restore source bytes after success or failure |
| UI build dependencies | Root `just setup`; explicit root npm install in CI and each Vercel static service | Pinned TOML parser and deployment tools include dev dependencies; the build composer never installs packages |
| Frontend hosting | Static customer CSR HTML/WASM/assets at `/`; backoffice under `/backoffice/`; legacy `/user` redirects | Browser loads public settings before application calls and session restoration |
| Cron authorization | Environment `CRON_SECRET` | Backend only; never embedded in browser assets |
| Local Vercel environment | Inherited environment → ignored root `.env` → ignored API `.env` | `just vercel-dev` checks DATABASE_URL, JWT_SECRET, CRON_SECRET and S3 credentials, starts/initializes local S3, sets local development metadata and uses the API native dev command through an owned temporary config. The API loads its local TOML overlay and cron choice; production still uses the container |

Inherited environment variables keep dotenvy's normal precedence, including `RUN_ENV` and provider credentials. Normal API settings remain file-based; container port and trusted preview/development host metadata are narrow platform exceptions. Absent Shopify configuration is optional when no token override is supplied. A token override requires the admin URL from file configuration; missing required fields produce normal configuration errors. Explicit client construction parses the trusted URL and validates the token/header.

| API development mode | Setup command / selected Bitwarden item | Runtime selection |
| --- | --- | --- |
| Local (default) | `just setup` or `just setup --local`; item `87323839-368f-4288-a4a5-9b0320699892` | Writes `RUN_ENV=local` in ignored `.env`; `just run` loads `application.local.toml` |
| QA | `just setup --qa`; item `0a291e2c-4ae4-436d-8730-ddf301a544d7` | Writes `RUN_ENV=qa` in the same `.env`; `just run` loads required `application.qa.toml` |
| Shared local services | PostgreSQL and S3 use the existing development lifecycle | Browser `allowed_origins = ["http://localhost:8084", "http://localhost:8085"]`, S3 endpoint `http://localhost:3001`, VPS scheduler by default |
| Stripe endpoint | `https://api.stripe.com/v1/` in both modes | QA explicitly retains this URL; use the selected item's sandbox credential for test payments |
| Vault fields | `JWT_SECRET`, `CRON_SECRET`, `SHOPIFY_ACCESS_TOKEN`, `RESEND_API_KEY`, `PAYPAL_CLIENT_ID`, `PAYPAL_CLIENT_SECRET`, `STRIPE_SECRET_KEY` | No database, S3 or private sender profile import |
| Same-mode setup | Preserve nonempty values; fill missing/empty values | Secure atomic `.env` write with mode `0600`; RUN_ENV always reflects the selected mode |
| Mode switch | Replace only imported fields with selected-item values or defaults | Clear missing optional provider fields; generate missing signing/cron defaults; preserve database, S3, sender, unrelated values and dotenv records |
| Missing Bitwarden CLI | Apply the same preservation/switch policy using defaults | Optional providers may remain unconfigured; vault retrieval failures leave the file unchanged |

QA mode alone does not establish payment readiness. QA overrides PayPal with `https://api-m.sandbox.paypal.com`; supply matching Sandbox credentials. Stripe retains its common URL and uses the selected credential to select its account/environment. Keep credentials and private profiles out of tracked files. The runner retains ordinary inherited-environment precedence and uses the single `just run` command.

```mermaid
flowchart LR
    TOML["API TOML + optional overlay"] --> SETTINGS["config.rs typed settings"]
    SECRETS["Ignored .env / environment"] --> SETTINGS
    SETTINGS --> API["Axum listener and database pool"]
    SETTINGS --> CLIENT["AppState ShopifyClient initialization"]
    ASSET["Public frontend TOML asset"] --> LOADER["Fetch and parse once"]
    LOADER --> UI["Pages and session restoration"]
    UI -->|"Credentials and Origin"| API
```

| Backoffice startup state | Behavior |
| --- | --- |
| Loading | Wait before rendering application pages or making API calls |
| Loaded | Provide settings and restore session |
| Fetch/parse error | Show error and Retry; resume only after valid settings load |

## Package commands

Run recipes from the package directory.

| Package | Recipes |
| --- | --- |
| API | `just setup [--local/--qa]`, `just run`, `just test`, `just check`, `just fix`, `just db-down` |
| Backoffice | `just setup`, `just serve`, `just build`, `just test`, `just check` |
| Domain | `just setup`, `just test`, `just check`; library has no server |
| Shared UI | `just check`, `just test`, `just test --coverage`; shared library with no server |
| Report generator | `just check`, `just test`; shared library with no setup recipe or server |
| Client UI | `just setup`, `just serve`, `just test`, `just check`; customer convention portal |
| Workspace formatting | `cargo fmt --all -- --check` from the repository root |

| API verification mode | Dependencies and scope |
| --- | --- |
| `cargo test --locked -p api --lib` | Library tests and loopback transport fixtures; configured Docker PostgreSQL must be ready; no Shopify credentials |
| `just test` | Compose PostgreSQL/private S3/init, native Cargo tests once, `cargo api-e2e` for TCP endpoint tests only, and Compose down with volumes removed even on failure |
| `just check` | Formatting and strict Clippy on all API targets |
| API E2E | `e2e-test/healthz.rs`, `e2e-test/v1/{auth,admins,conventions}.rs`, `e2e-test/v1/admin/{catalogue,conventions,inventories,orders,transactions}.rs` and `e2e-test/v1/admin/cron/{configs,report}.rs`; real TCP endpoints |
| Process lifecycle | `e2e-test/process.rs` exercises owned-child teardown |
| Shared harness | `e2e-test/common.rs` owns temporary TOML, local API child, readiness, and teardown |
| Fixtures | Unique owned records; shared Compose database during a run; cleanup only test-owned data |
| Unit layout | `src/tests/` mirrors implementation paths; test-gated child modules preserve private access |
| Domain contracts | `crates/domain/tests/contracts.rs` verifies shared public serialization |

Do not substitute loopback client fixtures or in-process handler tests for deployed-service E2E. Shopify transport fixtures are native integration tests; routine validation does not call Shopify or change store inventory.

## Using the Shopify client

Handlers resolve the shared state.shopify client initialized once by app_state and pass concrete client references to the dependent services. Services, clients and repositories do not access AppState. Middleware receives only browser/JWT configuration. Request/result models are exported through api::shopify_models; construct IDs with ShopifyId::new. Paginated proxy methods accept a PageRequest with first (1–250) and nullable after, return one page, and never fetch another page automatically.

| Operation | Caller responsibility |
| --- | --- |
| Product listing | products_with_variants(request, page): location/statuses; one product page with fixed250 variants and completeness metadata |
| Selection catalogue | `catalogue_products(location, search, page)`: one product page; raw search and root cursor unchanged; fixed 250 complete variants, current content/exact contextual price/location available, no supplier cost |
| Convention selection validation | `convention_variant(id)`: direct ProductVariant lookup for association, ACTIVE/tracked rules and trusted display snapshots; no catalogue scanning or provider mutation |
| Accounting enrichment | accounting_orders(search, page): one order page including actual order tags for the accounting service; retains the OrdersByPayment document and nested completeness metadata |
| Tag lookup | orders_by_tag(request, page): literal tags combined with AND; one order page including fixed250 report lines |
| Inventory discovery | find_inventory_to_activate(request, page): one raw level page; caller decides eligibility from tracked/isActive/on_hand |
| Inventory activation | activate_existing_inventory(request): item/location IDs and stable UUID only; no quantity arguments; inspect userErrors/optional level |
| Nested bounds | Fixed250 variants/order lines is the agreed store workload; report services reject unexpected hasNextPage instead of silently truncating |
| Credentials/scopes | Appropriate product/inventory/location/order permissions and approved older-order/protected-customer access where applicable |
| Failure handling | ClientError automatically captures its construction/conversion file:line:column and retains its typed failure and underlying source. Convert to AppError at the response boundary or use log_background_error for one ERROR; returning/propagating a Result does not log |
| Privacy | Do not log token values, request/response bodies, raw upstream fields, payment references or customer information |

Inventory returns one caller-selected product page with active inventory levels; order reports exhaust matching order pages before returning grouped totals. Source models transform their own values; services own pagination and grouping order. Client methods do not interpret mutation userErrors.

| Outbound request behavior | Policy |
| --- | --- |
| Timeout / redirects | client.shopify.timeout_secs in application.toml, checked-in value3 seconds per attempt; redirects disabled |
| Retryable rejection | HTTP 4xx or data-less GraphQL-only `THROTTLED`, sharing one HttpClient retry budget |
| Backoff | At most three retries after the initial attempt; 1/2/4 seconds plus jitter and applicable Retry-After |
| Budget | Shared HTTP/GraphQL attempt count; 30-second total sleep budget |
| Fail without retry | HTTP 5xx and other errors, ambiguous network failures, decoding/partial-data errors, mutation user errors |
| Stored inventory | Enablement sends no initial quantity arguments; live provider quantity behavior remains unverified |

## Stripe-hosted Checkout

The existing Stripe client exposes provider operations to trusted Rust callers. Postman requests in `postman/collections/external/stripe/` call Stripe directly; guest checkout below consumes these operations with durable state and Shopify binding. Direct provider requests do not require Shopify.

| Input / operation | Operator contract |
| --- | --- |
| Credentials / version | Existing `stripe` environment URL includes `/v1`; inherited `vault:stripe_secret_key` supplies the selected credential. For a sandbox exercise, select the QA item’s test secret explicitly in Postman; API `.env` is not automatically loaded by Postman. No Connect `Stripe-Account` or `Stripe-Version` override: requests use the direct account and its default API version. Record that effective version for any manual exercise; signed events expose their own `api_version` where supplied. Fixtures describe tested object shapes, not live version compatibility |
| Create Checkout Session | Set folder variables for positive integer minor-unit amount, lowercase currency, fixed success/cancel URLs, non-personal `correlation_id`, product name and operation idempotency key. One line item with quantity1 and card payment method; account/device support determines wallet presentation. Keep tax, shipping, discounts and adaptive pricing disabled |
| Retrieve resources | Copy the actual Session ID into `stripe_checkout_session_id`, its PaymentIntent ID into `stripe_payment_intent_id`, and the actual latest Charge ID into the existing Retrieve charge request's `charge_id`. Session `expand[]=payment_intent.latest_charge` and PaymentIntent `expand[]=latest_charge` can return expanded objects; string references remain valid |
| Expected identity | Trusted caller supplies Session ID, merchant account ID, environment, exact money and correlation independently. Retrieve current account must match that merchant; all payment resources must match the expected test/live mode and money. Session and PaymentIntent metadata must retain the correlation |
| Paid verification | Use the internal `services::payments::stripe::verify_payment` use case with trusted expectations for an application decision. Expected/outcome types live in `models::stripe::payment`; clients retain individual API operations. It returns `ClientError`; application callers own HTTP error mapping. It retrieves current provider objects and requires matching context, payment Session paid status, succeeded PaymentIntent with exact received amount and its successful captured Charge. A redirect, completed Session, event name, `no_payment_required` or valid signature alone does not prove paid |
| Creation ambiguity | New create operations are sent once without automatic retries. After an ambiguous create transport failure, reconcile the same operation; never blindly issue a new key. Stripe can prune keys after at least24 hours, so replay after retention needs external reconciliation |
| Errors / refunds | Provider rejections, malformed objects, unknown statuses and mismatched account/mode/money/correlation never produce verified paid. Original successful payment verification does not assert an unrefunded balance; refund policy belongs to a consumer |
| Signed events | Pass exact raw bytes, signature header, endpoint secret, trusted time and bounded tolerance to the Rust verifier before decoding. The endpoint invokes client authentication, then the notification service consumes the typed event. Provider models own decoded data. The result carries event type, environment, account, resource ID and consumed checkout-reference text; incoming event ID, optional API-version shape and object references are still validated. Signature authentication does not deduplicate events or establish payment success; use current resource verification and consumer-owned replay/state handling |

For a sandbox exercise, prepare and run the backend with `just setup --qa` then `just run`. In the existing Stripe Postman requests, select the QA test key, retrieve current account and compare the expected merchant → create a test Session → open the returned Stripe-hosted URL and complete the payer flow using [Stripe test payment details](https://docs.stripe.com/testing) → retrieve Session, PaymentIntent and Charge → require `livemode=false`, exact expected minor-unit money and actual returned references, then invoke the existing Rust payment-result verifier from a trusted Rust caller. Use the guest checkout requests below for the combined application flow. These requests mutate the selected account when creating; do not run the external collection wholesale against a provider. Example IDs and bodies are synthetic and must be replaced with returned values. Local tests do not verify the actual Bitwarden item, Stripe-hosted payment flow, wallet availability or live acceptance.

Provider references: [Session creation](https://docs.stripe.com/api/checkout/sessions/create), [Session retrieval](https://docs.stripe.com/api/checkout/sessions/retrieve), [PaymentIntent retrieval](https://docs.stripe.com/api/payment_intents/retrieve), [direct-account SDK operation](https://github.com/stripe/stripe-dotnet/blob/master/src/Stripe.net/Services/Accounts/AccountService.cs), [idempotency](https://docs.stripe.com/api/idempotent_requests).

## PayPal Orders

| Concern | Caller requirement |
| --- | --- |
| Configuration | Use the shared optional `PaypalClient` with TOML REST origin/timeout and environment credentials. `just setup --qa` selects the checked-in Sandbox origin; merchant, correlation and expected money are trusted consuming-flow inputs |
| Provider operations | Existing external Postman requests create/get/capture an order, get a capture and verify a webhook signature. Replace synthetic values with returned provider IDs; use a stable caller-owned `PayPal-Request-Id` for each create/capture operation |
| Approval and capture | Buyer approval is unpaid. The consuming flow explicitly captures an approved wallet order and owns retention-aware recovery if a write response is ambiguous; polling/webhooks alone do not collect the payment |
| Result verification | Invoke the internal `services::payments::paypal::verify_payment` use case (types and value conversion in `models::paypal::payment`) with trusted order ID, merchant, configured origin, reference/custom correlation, positive minor-unit amount and currency. It only retrieves current resources and checks their exact identity/money. Paid results return order/capture IDs and money; pending/unpaid outcomes carry only state |
| Currency | Precision uses the existing `CurrencyCode` enum and exact canonical spelling. Unsupported, unknown, lowercase, mixed-case and whitespace-bearing codes are rejected; the supported currency set is unchanged |
| Error handling | The payment verifier returns `ClientError`: `InvalidInput` for invalid expectations and `Protocol` for inconsistent evidence. Upstream errors preserve their source and original location; the checkout application service maps failures to HTTP |
| Webhooks | Supply the exact complete event, five transmission headers and trusted webhook registration ID to the authenticated-event helper. The endpoint invokes client authentication, then the notification service consumes typed event kind, related order ID and checkout-reference text. Provider models validate incoming event ID and resource/capture consistency. A successful signature postback establishes event authenticity; consumers still verify current payment resources and own durable replay/state handling |
| Application boundary | The provider library remains independent; guest checkout below owns authorization, capture, durable operation IDs, expected values, persistence, Shopify binding and serialization |
| Verification limits | Local controlled HTTP checks and collection lint do not establish provider acceptance. Historical Sandbox approval/capture receipts predate the stateless refactor; genuine signed-webhook acceptance remains unverified without an app webhook registration |

Run only selected external requests with intended Sandbox credentials; creating/capturing mutates that account. Do not execute the external collection wholesale. Earlier local gateway/Sandbox results remain dated historical receipts in [verification](verification.md).

## Paid Shopify draft completion

| Input / operation | Trusted caller contract |
| --- | --- |
| Entry point | Call the Rust completion service with a typed DraftOrder ID and the existing Stripe or PayPal expected-payment context. Guest checkout below supplies the durable consuming flow; the library itself does not issue credentials or persist state |
| Trusted values | Bind provider payment to draft independently; provider-copied/browser totals and paid flags are insufficient. Existing verifiers establish account/merchant, environment/origin, correlation and exact money |
| Completion | Read the draft, compare exact presentment amount/currency, and use `CompleteIfOpen` only when the caller permits an eligible open draft to be completed. Verify the real linked paid/not-cancelled order and its money |
| Ambiguous outcome | Retain the durable operation identity and uncertainty in the consuming flow. Use `ReconcileOnly` after uncertain completion; this mode never completes an open draft. A still-open read does not authorize blind replay |
| Reference tags | Add verified references as `payment:stripe:session:<id>`, `payment:stripe:intent:<id>`, `payment:stripe:charge:<id>`, `payment:paypal:order:<id>` or `payment:paypal:capture:<id>`. Reject conflicting/invalid owned references, preserve unrelated tags and require actual tag readback |
| Replay and ownership | Read Shopify on every call; reconcile tags on an already completed linked order. The caller owns authorization, serialization, payment-to-draft uniqueness policy, polling and webhook dispatch. There is no local cache or exactly-once guarantee |
| Postman | Retained external Shopify requests exercise draft/order reads, completion and tag edits directly with synthetic inputs replaced by real returned IDs. Internal `/order` requests exercise orchestration; direct external mutations change the selected store |
| Email / limits | Shopify native completion is the confirmation-email mechanism. Fixture tests and Postman lint do not establish live merchant email delivery or end-to-end browser/webhook acceptance |
| Accounting | Existing collection/matching rules and financial values remain unchanged. Matched rows expose actual `shopify_order_tags`, unmatched rows expose `[]`; CSV preserves commas/quotes. No local mapping fallback, additional Charge/account request or cron change |

## Guest checkout API

| Setup / request | Contract |
| --- | --- |
| Enablement | Configure the complete commented `[checkout]` section in API `application.toml` through the existing loader. Without it, checkout returns 503 while unrelated endpoints remain available. Set actual Shopify location, trusted frontend origin/path templates, reservation/recovery bounds, expected Stripe account/mode and matching PayPal origin |
| Private environment | Supply `CHECKOUT_STRIPE_WEBHOOK_SECRET`, `CHECKOUT_PAYPAL_WEBHOOK_ID` and `CHECKOUT_PAYPAL_MERCHANT_ID` privately alongside existing provider credentials. These keys are not in the Bitwarden setup allowlist; configure explicitly. Missing gateway/webhook configuration affects that operation |
| Return URLs | Server expands configured `/{convention_slug}/...{checkout_id}...` templates against its trusted frontend origin. Return/cancel redirects do not prove payment. Frontend pages are not implemented here; configure deployed pages before a live flow |
| Create | `POST {{server}}/v1/order/checkout`, guest `Authorization: Bearer <64 hex characters>`. Before sending, save random v4 operation UUID and random 32-byte secret durably. `operation_id` becomes `checkout_id`; email is sent with intent to Shopify; it is never persisted in the checkout database. Retry exact payload/credential/ID; changed intent requires a new attempt |
| Inputs | `convention_id` and `items[].variant_id` are actual convention/selection UUIDs, not Shopify GIDs. Quantity 1–999; 1–100 distinct items; positive signed-64-bit expected unit price in minor units; uppercase currency. No client total/account/return URL/paid flag; unknown fields and nonempty queries return 400 |
| Verify / lost response | `POST {{server}}/v1/order/{checkout_id}/verify`, same guest bearer, empty body, no query. Recover by ID saved before create. Unknown/mismatched/expired credentials return 401 without contact/cart disclosure. Use bounded polling while processing; only completed permits confirmation |
| Response | 200 DTO: `checkout_id`, `status`, `money:{minor,currency}`, nullable `reservation_expires_at` and `next_action:{type:"redirect",url}`; completed adds `order:{id,name}`. Status: `requires_action`, `processing`, `unpaid`, `reconciliation_required`, `completed`. 200 alone does not mean paid/completed |
| Failures | 400 malformed input, 401 guest credential, 409 unavailable convention/variant or changed intent/price/eligibility, 503 optional dependency/configuration or retryable callback advancement, 502 upstream failure, 504 timeout, sanitized 500 database errors. Reconcile ambiguous writes on the same attempt |
| Webhooks | Deliver to `/api/v1/order/webhook/stripe` or `/api/v1/order/webhook/paypal`; native `/v1` aliases work. No admin bearer/Origin. Stripe authenticates exact bytes, signature, mode/account and timestamp within 300 seconds. PayPal requires full event and five transmission headers, verified through configured PayPal origin. Accepted 200: `{ "received": true }`; authentication rejection 400; retryable association/advancement 503 |
| Postman | `internal/webserver/v1/order` includes create, empty-body verify and both callbacks. Private vault `checkout_guest_secret` supplies guest bearer; folder variables document synthetic IDs and unchanged callback bytes/signatures. Replace with controlled fixtures or intended Sandbox values. Postman cannot synthesize a genuine PayPal delivery signature; no collection-wide external payment execution |
| Verification | Existing API `just check` / `just test` and `/Users/wphongphanpa/.local/bin/postman workspace lint`. Actual results belong in [verification](verification.md). Lint/fixtures do not prove hosted payment, genuine webhook delivery, browser recovery or merchant email delivery |

### Checkout configuration values

| Configuration | What it controls | Where to get or choose the value |
| --- | --- | --- |
| frontend_origin | Trusted frontend origin used to build gateway return URLs | Choose the actual public frontend origin, e.g. https://convention.enzumeii.com; it must also be in the existing browser origin allowlist. This is application/DNS configuration, not a provider-issued value. |
| return_path_template | Frontend path after hosted payment/approval | Choose a route the frontend implements, using {convention_slug} and {checkout_id}. The sample /{convention_slug}/checkout/return?checkout_id={checkout_id} is a contract for that page, not proof it is deployed. |
| cancel_path_template | Frontend path when leaving/canceling hosted checkout | Choose the implemented cancellation route with the same placeholders. Arrival never proves paid/unpaid; the page calls verification. |
| shopify_location_id | Shopify inventory location checked for available stock | Obtain the intended location's id through Shopify Admin GraphQL locations, then use the gid://shopify/Location/... form; the existing external Shopify Location Postman request validates a known ID. |
| reservation_secs | Requested inventory hold | Choose the policy in seconds; sample 3600 is one hour. Actual Shopify hold is read back; Stripe initiation needs at least 30 minutes remaining. |
| guest_recovery_secs | Guest access-credential lifetime | Choose the policy in seconds; sample 604800 is seven days. It does not erase orders or stop authentic callbacks. |
| stripe_expected_account | Separately trusted receiving Stripe account | Stripe Dashboard, Business settings → Account details → Account ID (acct_...). Confirm the intended merchant; do not automatically trust the account copied from an incoming event. |
| stripe_livemode | Expected Stripe resource environment | Choose false for the intended test/sandbox credentials/resources and true for live. This must match the configured Stripe key and returned resources. |
| paypal_api_origin | Expected PayPal REST environment | Select the documented endpoint matching the shared client: https://api-m.sandbox.paypal.com for Sandbox or https://api-m.paypal.com for live. |
| CHECKOUT_STRIPE_WEBHOOK_SECRET | Stripe webhook signature verification | Obtain the whsec_... signing secret from that endpoint's Stripe Workbench/Webhooks page using Reveal secret. It is not STRIPE_SECRET_KEY; CLI-forwarded and Dashboard endpoint secrets differ. Provide it privately to the backend environment. |
| CHECKOUT_PAYPAL_WEBHOOK_ID | PayPal verification for the registered callback | PayPal Developer Dashboard, Apps & Credentials → the correct Sandbox/Live REST app → its Webhooks subscription. Copy the Webhook ID for this listener; it is not the app's client ID. A subscription must exist before there is an ID. Provide it privately to the backend environment. |
| CHECKOUT_PAYPAL_MERCHANT_ID | Trusted receiving PayPal business | Intended PayPal business account: Account Settings → Business information → PayPal Merchant ID. Use the corresponding Sandbox business account in Sandbox. Provide it privately to the backend environment. |

The three CHECKOUT_* values are obtained from the provider account and supplied by the operator through backend environment settings or the ignored local .env. They are not invented by the application, frontend values, or values to paste in a PR/chat. This change does not create provider webhook registrations or retrieve secrets. The former rate_limit_attempts/rate_limit_window_secs are removed with the checkout SQL admission counter.

Sources: [Shopify locations](https://shopify.dev/docs/api/admin-graphql/2026-10/queries/locations), [Stripe account ID](https://docs.stripe.com/use-stripe-apps/netsuite/customer-payment-page-setup), [Stripe signing secret](https://docs.stripe.com/events/manage-webhook-endpoints#check-the-endpoint-secret), [PayPal webhook registration](https://developer.paypal.com/api/rest/webhooks/rest/), [PayPal merchant ID](https://www.paypal.com/au/cshelp/article/how-do-i-find-my-secure-merchant-id-in-my-paypal-account-help538), [PayPal environments](https://developer.paypal.com/api/rest/requests/).

### Webhooks on Vercel

Vercel accepts inbound third-party HTTPS webhooks. This repository rewrites `/api/(.*)` to its API container; use public callback URLs ending in `/api/v1/order/webhook/stripe` and `/api/v1/order/webhook/paypal`. Axum authenticates raw request bytes; callbacks do not use browser Origin or admin bearer authentication. Use a public production domain or an explicit deployment-protection exception. Interactive deployment protection can block providers; the current no-query validation also rejects query-string bypass secrets. Processing remains request-bound with retryable errors; no detached background continuation is assumed. This does not establish a registered listener or real merchant delivery. See [raw-body handling](https://vercel.com/kb/guide/how-do-i-get-the-raw-body-of-a-serverless-function), [containers](https://vercel.com/kb/guide/docker) and [deployment protection](https://vercel.com/docs/deployment-protection/methods-to-bypass-deployment-protection).

### Recovery limits

Shopify holds accepted email/items/money. Only control hashes, ownership and resource links are stored locally. If no draft was attempted, the original unchanged create body may resume; ID-only verification cannot rebuild the form. An attempted draft with a lost ID is resolved by paginated exact-tag/evidence lookup, and only a unique match binds. Zero/multiple/incomplete matches do not permit a new draft. Gateway retries retain the derived key, frozen return URLs, expiry and reconstructed-request fingerprint. When authoritative money is unavailable, safe 409/503 or upstream errors replace an invented 200 total. The removed SQL admission counter has no replacement throttle in this change.

## Runtime, failures, and administration

| Concern | Mechanism |
| --- | --- |
| Password operations | Argon2 through Tokio `spawn_blocking` |
| Shutdown | Native Axum graceful shutdown on Ctrl-C/SIGTERM |
| Refresh retention | Shared cleanup helper; hourly VPS task or reserved Vercel tick time; retained when report scheduling is disabled |
| API rejection | Invalid request400; unauthenticated401; forbidden403; missing Shopify location404; duplicate username409. Missing admin targets return401 with {"error":"Not authorized"} and WWW-Authenticate: Bearer |
| Accounting dependencies | `GET /v1/admin/transactions` requires initialized Stripe, PayPal and Shopify clients. Resolve all three before upstream work; missing/init-failed clients return provider-specific503 in that order. Startup and unrelated endpoints keep optional-provider behavior |
| Accounting collection | Query both payment providers even if one is empty; both empty returns empty transactions/items without Shopify enrichment. Actual upstream failures abort without partial results:502 or timeout504 with safe `Upstream request failed` |
| Internal failure | Generic public 500; underlying source retained internally |
| Normal logs | One boundary event with operation, HTTP status and automatic source location in the prefix; optional client_origin retains the client's original file:line:column. No error messages or causes |
| Error location | First AppError creation/conversion location survives propagation; external-library failures identify our mapping site, not an internal dependency frame |
| DEBUG logs | `RUST_LOG=info,api=debug` enables underlying API causes/context; diagnostic text may contain sensitive values |
| UI failures | Preserve API status; only 401 triggers restoration; malformed response 502, missing config/coordination 503, stale account state 409 |
| Initial administration | Intentional superadmin seed migration |
| Additional administrators | Authenticated `POST /v1/admins`; API validates credentials and stores hashes |
| Schema changes | Preserve admin/seed behavior and applied migration history; SQLx uses committed offline metadata |
| Report receipts | report_results stores audit fields; report_result_files stores ordered private object metadata with parent/filename, parent/position and bucket/key uniqueness. Checkpoint JSON remains worker recovery state; history reads canonical child rows in one SQL snapshot |
| Deployment | Root Vercel services configuration, two CSR builds, one API container and daily HTTP cron; Manually dispatched Release workflow publishes releases from main, then waits for production approval before deployment; automatic Vercel Git deployments are disabled |

## Local planning artifacts

| Path | Policy |
| --- | --- |
| `docs/` | Cohesive current architecture, operations, and separately recorded verification; no per-task appendices |
| `docs/superpowers/` | Ignored local plans; excluded from the published tree |
| `.superpowers/` | Ignored local execution scratch/ledgers; excluded from the published tree |
| Postman workspace | Existing local edits are preserved; carrying files to a worktree does not authorize publishing secrets |

## Inventory and order report requests

| Request | Input / result |
| --- | --- |
| `GET /v1/admin/inventories/{inventory_id}?page_size=50&cursor=abc` | Numeric Shopify location ID; page_size defaults50, range1–250, counts root products; cursor forwards unchanged to Shopify. Returns products and page_info (has_next_page/end_cursor) |
| Inventory rows | ACTIVE products and active inventory levels at the location; zero/negative on_hand retained. A product can produce multiple variant rows; no local row slicing |
| Inventory continuation | Products ordered by total inventory descending across all locations; fixed250 variants per product, no variant traversal. One upstream product page per API call, even if every row filters out at the requested location. Follow returned cursor when has_next_page is true, including empty pages; restart pagination after changing the ordering |
| Missing inventory location | HTTP404 with {"error":"Shopify location not found"}; no catalog query is sent after a failed location lookup |
| `GET /v1/admin/orders/report?tags=TAG` | Comma-separated literal tags, trimmed and combined with AND; empty entries rejected400; root batches250; merge compatible SKU/price/currency rows across every matching order page. Returns products only; page_size/cursor rejected400 |
| Credentials | Existing ADMIN/SUPER_ADMIN bearer or cookie credentials; Shopify URL in TOML and SHOPIFY_ACCESS_TOKEN environment secret |
| Shared helpers | utils/numbers.rs parses native f64 values and multiplies quantities, rounding calculated totals to two decimal places; product models assemble report rows and update totals |
| Upstream errors | Shared mapper returns Upstream request failed with502 for failures and504 for timeouts; original causes remain private |
| Postman | Internal inventories first/continuation and orders report requests use location_id and order_tag variables; replace the synthetic abc cursor with the returned end_cursor |
| Verification | API just test includes real TCP report routes with Docker PostgreSQL and the approved synthetic Shopify upstream; live store verification remains manual |
| Formatting | API just setup installs dprint 0.63.0 with Cargo under services/api/.tools and caches the pinned Rust-based GraphQL/JSON Wasm plugins there. just check checks API GraphQL/JSON, Rust formatting and strict lint; just fix prepares SQLx metadata before formatting. CI uses the same setup recipe. Update Postman query text after formatting GraphQL |

Report queries require product, inventory, location and order access; reading unitCost may also require View product costs permission. Historical order access remains subject to Shopify permissions. Missing product/variant metadata produces null values; quantities use original order lines, not refund-adjusted accounting.

## Convention administration and catalogue requests

| Setting / request | Usage |
| --- | --- |
| Convention currency | Supply the currency and payment methods in each convention create request; they are stored in the database with no currency configuration section. Intrinsic currency/method restrictions remain: PromptPay requires THB; PayPal supports its established currency and precision set |
| Provider readiness | Supply provider credentials privately and establish merchant support separately before later payment work; this feature performs no charge, reservation or Shopify mutation |
| Authenticated base | `/api/v1/admin` (native `/v1/admin` alias); ADMIN and SUPER_ADMIN bearer/cookie authentication, existing origin guard and CORS including PUT |
| Options / UI catalogue location | `GET /conventions/options` returns `countries` ISO-code strings and existing currency/payment capabilities. Backoffice country/currency selects consume this response. Public `[conventions].catalogue_location_id = "65761018027"` is loaded once from local/production backoffice TOML; the catalogue popup exposes product-type/title filters and combines them with AND |
| Create / read | `POST /conventions`, `GET /conventions`, `GET /conventions/{id}`. Create requires slug/name/country/event dates/description/currency/methods; optional validated `logo_url` from the generic image upload. Slug is lowercase hyphen-separated text, 1–63 bytes, excluding reserved paths. Country is assigned ISO alpha-2; dates are valid YYYY-MM-DD with start<=end |
| List / selections | Convention and variant lists use `page=1&per_page=25` defaults; page positive and `per_page` 1–100, responses include `has_next`. Conventions accept optional `preorder_enabled=true/false` |
| Patch / delete | `PATCH /conventions/{id}` changes supplied editable fields; absent fields stay unchanged, only `logo_url:null` clears. Empty/unknown/immutable fields are rejected. `DELETE /conventions/{id}` returns 204 and cascades current memberships |
| Strict request shapes | Only the convention/variant list routes accept their documented query parameters; create/detail/PATCH/DELETE/PUT reject extra query strings. DELETE requires an empty body, including no `{}` or whitespace payload |
| Selection replacement | `PUT /conventions/{id}/variants` with `{"items":[{"shopify_product_id":"gid://shopify/Product/1","shopify_variant_id":"gid://shopify/ProductVariant/1","final_price":"12.99"}]}`. Replaces the entire set atomically, at most 250 unique variants; `items:[]` clears only while disabled |
| Logo storage destination | `POST /files?location=conventions/<slug>` explicitly selects bucket `conventions` and folder `<slug>`; the returned/stored value includes bucket and key. Attachment and preview use that same destination. Reports keep their configured bucket. Old objects under `reports/conventions/...` are not copied or read through fallback |
| Convention migration | Keep published `20261023090000_conventions.sql` unchanged and reuse its existing `logo_url` column for S3 object locations. No additional logo column or migration is required. Legacy HTTPS values remain stored without S3 previews; attach an uploaded image to establish a location |
| Selected display | GET/PUT return nullable `product_title`, `variant_title`, `sku`, `product_image`, `variant_image` and `inventory_policy` alongside price. Saved images use the URL-only catalogue shape; the nullable DENY/CONTINUE policy is a snapshot and both policies are accepted. Existing validation refreshes new/changed or enabled PUT selections atomically; null clears obsolete fields. Unchanged disabled PUT and all GET pages use saved metadata without Shopify. Rows with missing display data or policy stay null until a validated save; PATCH enabling does not refresh metadata |
| Exact price | Send positive unsigned plain decimal strings, not JSON numbers/exponents/signs; <=24 input bytes, bounded to signed 64-bit minor units, with currency/method precision. Prices persist in major units without floating-point conversion; USD `"12.99"` round-trips as `"12.99"` |
| Manual flag | Create always stores false. PATCH `preorder_enabled:true` revalidates nonempty eligible selection; dates do not schedule opening/closing. Changing methods revalidates existing prices; disabling does not require Shopify availability |
| Catalogue request | `GET /conventions/catalogue?location_id=1&page_size=25`; `location_id` is a positive numeric Shopify location ID; the backend constructs the Location GID. `page_size` 1–250, default 25. Optional `search`/`cursor` must be nonblank and <=2048 UTF-8 bytes. Unknown/duplicate/malformed parameters are rejected 400 |
| Search / continuation | Pass raw Shopify syntax, e.g. `product_type:Sticker AND title:Sticker`; URL-encode the query value for transport. The service supplies `status:active AND tracks_inventory:true AND (<search>)`, or `status:active AND tracks_inventory:true` without user search. Follow returned `page_info.end_cursor` when `has_next_page`; cursors remain opaque. Changing search starts a new traversal. Filtering can return an empty page with `has_next_page=true`; no additional upstream page is fetched |
| Catalogue completeness | One root page and fixed 250 variants per product; nested `hasNextPage` or malformed required provider data returns 502, never a partial catalogue. The root query requires active products tracking inventory; nested variants retain local tracking and inventory-level checks because Product.variants has no corresponding query arguments. Untracked variants and variants with missing/inactive inventory levels at the requested location are omitted, along with empty products. Active tracked zero-stock DENY/CONTINUE variants remain visible with eligible=true; catalogue price/content are current Shopify values |
| Safe failures | Invalid input 400, authentication 401/role 403, missing convention/location 404, duplicate slug or changed validation assumptions 409, optional Shopify unavailable 503, upstream 502/timeout 504. Failed replacements preserve the prior membership |
| Postman examples | Internal `conventions/{list,create,get,patch,delete,list-variants,replace-variants}` and `conventions/catalogue/{products,continue}` use synthetic IDs/prices and workspace variables. External `catalogue_products` / `convention_variant` query text and variables match API documents; inherited credentials remain private |
| Local checks | Use package `just check` / `just test` recipes and root `cargo fmt --all -- --check`. Local-only `/Users/wphongphanpa/.local/bin/postman workspace lint` validates the checked-in workspace; do not run synthetic mutation examples against production |
| Isolation | Use an owned `CARGO_TARGET_DIR` per worktree. API full recipe accepts `E2E_COMPOSE_PROJECT`, `E2E_POSTGRES_PORT`, `E2E_S3_PORT` and `E2E_DATABASE_URL`; preserve its Compose cleanup. Controlled provider matrices may serialize behind the fixture advisory lock |
| Current limit | Public convention browsing, product detail and quotes use the anonymous APIs described in [conventions](conventions.md). The portal does not invoke payments; guest checkout retains linkage/evidence hashes while Shopify retains the accepted purchase |

## Inventory enablement

| Request / outcome | Behavior |
| --- | --- |
| POST /v1/admin/inventories/{inventory_id}/enable | Numeric Shopify location ID; no body or query parameters; existing ADMIN/SUPER_ADMIN bearer or cookie credentials |
| Discovery | Validate location, exhaust inventory level batches250 with inactive levels included, then activate unique tracked inactive items with positive on_hand |
| Quantities | Use on_hand for eligibility only; send no onHand or available mutation arguments. Live provider quantity behavior is unverified |
| Response200 | total/success/failed count variant inventory items, not unique products; omit errors if none fail; each failed item includes numeric product_id and a safe reason, so a product ID can repeat |
| Mutation failures | Continue independent items; timeouts report an unconfirmed outcome; prior successful activations are not rolled back |
| Discovery failures | No writes before full discovery: malformed input400, missing location404, unavailable client503, upstream502/timeout504 |
| Provider permission | Shopify write_inventory and permission to activate inventory; one activation enables one variant at one location |
| Manual tooling | Postman inventories/enable uses inherited credentials and location_id; it mutates store inventory when run against a live store |


## Order fulfillment configuration and contracts

| Area | Usage |
| --- | --- |
| Local setup | API `just setup` fills missing/empty SHIPPING_SENDER_* values with synthetic local data, preserving existing nonempty settings and generated secrets on rerun |
| Sender profile | Supply name, phone, email, address, sub_district, district, province, postcode and country_code through SHIPPING_SENDER_*; replace synthetic setup data before actual shipping use. No private sender profile belongs in TOML |
| Email deadline | `client.resend.timeout_secs = 3` in TOML; missing/zero retains an optional-client initialization error and affected calls return 503 |
| Order listing | GET `/v1/admin/orders` supports omitted tags, uppercase fulfillment values, or comma-separated AND tags; report still requires tags. Orders return id, number, nullable full_name (trimmed customer first/last name) and nullable order contact email and nullable shipping_country_code from shippingAddress.countryCodeV2, plus upstream page_info |
| Reship | POST `/v1/admin/orders/1085/reship`, replacing 1085 with the four-digit merchant order number; no body/query; one ListOrders name search (first1, includeContact=true); send before RETURNED tagging; success returns `success` and provider-accepted `email_id` after both operations |
| Fulfillment workload | One selling location, at most 250 order items; completed/cancelled fulfillment orders are skipped; one creation precedes SHIPPING removal |
| Shipping workload | ShippingOrders combines item and shipping-method reads, each fixed250 without child cursors; root orders still paginate. Extra shipping-label query parameters are ignored |
| Provider tooling | ListOrders requires explicit includeContact/includeTags Booleans: listing true/false, merchant lookup true/false, tag lookup false/true. EditOrderTags selects add=true or false; services resolve stored casing and skip no-ops. Read/mutation is not atomic against concurrent case-variant inserts; unrelated tags are preserved |
| Listing cursor | Optional: omission forwards after:null. Supplied cursors must be nonblank; the literal string null is an opaque string, not a JSON null |
| Shipping declaration | Apply the legacy mock rule to any selected price currency: max(sum of non-Tip prices ×5,100). Stock quantities are excluded; stock prices remain included |

## Coverage and releases

| Command / setting | Behavior |
| --- | --- |
| Module `just test` | Existing suite, no coverage tooling required |
| Module `just test --coverage` | Native tests with an isolated report at repository `target/coverage/<crate>/lcov.info` |
| API coverage | Includes TCP API child-process execution; graceful termination flushes profiles before bounded forced cleanup |
| API test isolation | Optional E2E_COMPOSE_PROJECT, E2E_POSTGRES_PORT and E2E_DATABASE_URL allow independent worktrees; defaults remain e2e/5433; E2E_S3_PORT defaults5434 and dedicated synthetic E2E_S3_ACCESS_KEY_ID/E2E_S3_SECRET_ACCESS_KEY isolate storage credentials |
| Coverage CI | The existing check job runs report-generator check/test and the five instrumented module suites sequentially, retains five scoped reports, and uploads them together once to Codecov |
| CODECOV_TOKEN | Add the Codecov repository upload token in GitHub Actions secrets; unavailable secrets skip upload with a visible notice and retain artifacts |
| Root `just setup` / `just check` | Node 24+; setup installs the pinned Conventional Commits reference parser; check runs Node tests, actionlint 1.7.12 and shellcheck |
| Coverage build directory | Local runs isolate builds per crate; CI sets CARGO_LLVM_COV_TARGET_DIR to target and reuses builds sequentially |
| Runtime limits | Native coverage only; WASM compilation remains uninstrumented; browser runtime and doctest coverage are not measured |

| PR title | Version increment |
| --- | --- |
| Any type with `!`, for example `feat(api)!: remove field`, or an uppercase `BREAKING CHANGE:` / `BREAKING-CHANGE:` footer in any landed commit for the PR | Major, including 0.x → 1.0.0 |
| `feat: ...` | Minor |
| `fix: ...` | Patch |
| Other types, including `chore`, `ci`, `docs` | No implicit version increment |

Titles follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/), including case-insensitive types, optional scope and a nonempty description after colon-space. CI and releases share the pinned reference parser. Release classification uses the PR title plus the landed commit bodies and footers for that PR.

| Release rule | Behavior |
| --- | --- |
| Initial baseline | Virtual version 0.1.0 at commit 2f9f9101ad5348c327c75c4504a5827975497ada; earlier history is not released |
| Version-release trigger | GitHub Actions → Release → Run workflow with branch main only; trusted main code reconciles landed first-parent history in order, including merged fork PRs and native stacks with main as effective trunk |
| Native stacks | Release only merged layers landing on main, once per PR in landed order. Merge/squash layers target their landed merge SHA; rebase commits group by PR and target its final landed SHA. Unmerged upper layers are excluded; ambiguous membership fails |
| Version storage | GitHub vX.Y.Z tags and release notes; Cargo package versions stay unchanged |
| Concurrency/retry | Release jobs serialize in main-releases; approval-gated deployments serialize separately in vercel-production-deployments, so deployment queues cannot replace queued release jobs. Running jobs are never cancelled. Release replay catches queued merges, reuses existing matching tags/releases, and finishes a tag-only partial attempt |
| Conflicts | Invalid titles, direct commits, wrong-trunk or ambiguous associations, conflicting targets, or API errors fail visibly; tags are never moved |
| Changelog | GitHub generated notes bounded by prior tag and final landed PR SHA; bootstrap release links qualifying and preceding unreleased PRs |
| First live release | Requires merged changes followed by an explicit Release workflow run on main; local fixtures/dry runs do not verify hosted write permissions |
| Merge blocking | Repository rules must require the CI check names to enforce title validation before merge; workflow code does not change host rules |

## Vercel services

| Service | Root / builder | Public paths |
| --- | --- | --- |
| `backoffice` | `apps/backoffice/vercel` / `@vercel/static-build` | `/backoffice`, `/backoffice/(.*)` |
| `client-ui` | `apps/client-ui/vercel` / `@vercel/static-build` | `/`, `/<slug>`, `/<slug>/products/<product-id>`, `/<slug>/cart/<UUID>` and `/assets/*`; legacy `/user` redirects |
| `api` | `.` / `@vercel/container`; `Dockerfile.api.vercel` | `/api/(.*)` and exact `/_cron` |
| Bindings | None | Browser requests use `/api/v1` directly |

| Root command / file | Purpose |
| --- | --- |
| `just setup` | Install pinned local Vercel CLI and existing CI parser dependencies |
| `just check` | Workflow, shell, policy tests and pinned CLI configuration/service resolution |
| `just vercel-build` | Build both static CSR bundles and the API Docker image |
| `just vercel-dev` | Load local API secrets, start/init persistent local S3, prebuild static bundles, then run all three services on localhost:18027 (optional --port/--queue-port/--runtime-cache-port/--schedules-port); local TOML selects VPS polling or manual Vercel cron |
| `vercel.json` | Static filesystem-first routes, asset404 guards, app route-shape SPA fallbacks, legacy redirects, public API and daily cron |
| `scripts/install-vercel-dx.sh` | Compile pinned Dioxus 0.7.10 with the repository Rust toolchain into writable `.tools/vercel-dx`; reuse verified cached version |
| `scripts/build-vercel-ui.sh` | Select public production TOML, build web-only WASM, restore local TOML and copy only public assets (backoffice prefix; customer root) |
| `scripts/serve-vercel-ui.mjs` | Development-only static command for unlinked CLI, reading each service's existing routes and generated public directory; excluded from published frontend output |

| Native static build prerequisite | Requirement |
| --- | --- |
| Build host | Linux compiler/linker, pkg-config, OpenSSL development libraries, curl, tar and a writable Rust/cache directory; no sudo, Docker or just required by cloud commands |
| Rust | Repository `rust-toolchain.toml`; install pinned toolchain and wasm32-unknown-unknown target |
| Dioxus | Source-installed 0.7.10 matches host libc; official Linux release executable requires newer glibc than Amazon Linux 2023 |
| Cache | `.tools/vercel-dx`, Cargo registry/toolchain and each app's independent `target/vercel`; ignore generated `vercel/public` |
| Compatibility proof | Amazon Linux 2023 is a local approximation, since Vercel does not publish its exact build image; see verification evidence |

| API invocation | Behavior |
| --- | --- |
| `api` | Existing local configuration and migration behavior; local configuration selects VPS or Vercel cron |
| `api server` | Existing container command; connect only, no automatic migrations; RUN_ENV=prod uses the scheduler selected by application.prod.toml (checked in as vercel) |
| Configuration files | Common application.toml plus application.local.toml, required application.qa.toml or application.prod.toml selected by RUN_ENV; API image copies common/prod files and sets RUN_ENV=prod |
| Cron lifecycle | VPS owns periodic scheduler and independent hourly retention; Vercel awaits authenticated HTTP ticks without background tasks |

The daily `0 0 * * *` schedule is UTC: Hobby execution falls approximately between 07:00 and 07:59 Bangkok. It wakes stored Bangkok cron expressions. Production deployments alone receive scheduled calls; local dev does not schedule them. The backend authenticates `Authorization: Bearer CRON_SECRET`, awaits a bounded 240-second pass and reserves five seconds for retention. Missed/failed invocations await later recovery. Oversized jobs can repeat and exhaust attempts; backlog duration depends on workload.

Static services have no frontend executable or runtime dependency. Their build-only package metadata avoids Cargo backend detection. The root Vercel project includes workspace crates/scripts; backoffice output is `vercel/public/backoffice` and customer output is `vercel/public`. Service routes check existing files, reject missing assets, then fall back only for their configured route shapes. The customer shell resolves enabled slugs through the public API before rendering metadata; a syntactically valid unknown slug can receive an HTTP200 shell followed by a generic unavailable state. Arbitrary subpaths and missing assets return404. The API owns `/api/v1` directly and returns its ordinary errors for unknown `/api/*` routes. Native `/v1` and `/healthz` aliases receive no public Vercel rewrite. The customer root serves the convention directory. Legacy `/user` entry paths redirect to the corresponding root routes. Actual hosted geo-header propagation and domain cutover remain separate deployment verification.

| Manual production deployment | Operation |
| --- | --- |
| Approval | GitHub Actions → **Release** → **Run workflow** → select **main**: publish releases → **Review deployments** → select production → **wichayutdew** approves → SQLx migrates → all three services deploy in that same run |
| Target | Deploy requires a successful release job. It checks out the exact trusted main HEAD captured by that job as deployment_sha, including when main advances while release or approval is queued |
| Production environment | Configure GitHub environment `production` with required reviewer `wichayutdew`, self-review allowed and a main-only deployment branch policy. The environment gate holds every deployment step, including credential validation and migrations, until approval; the workflow reference alone does not configure these host rules |
| GitHub Actions secrets | `VERCEL_TOKEN`, `VERCEL_ORG_ID` and `VERCEL_PROJECT_ID` for the existing Vercel project, plus `SUPABASE_MIGRATION_DATABASE_URL`; after approval, missing values fail before checkout or tool installation |
| Execution | Install pinned Vercel CLI 65.1.0, validate services, install SQLx CLI 0.9.0 with PostgreSQL/rustls, apply committed migrations, then make one root `vercel deploy --prod --yes` call; normal remote builds include backoffice, client-ui and API in the same project/deployment |
| Completion | Wait for the deployment/build result; failure fails the job, success records the deployment URL in its summary |
| Automation boundary | Only an explicit Release workflow run on main publishes versions/tags/releases, then offers deployment approval in the same run. Pushes and PR merges do not start Release; git.deploymentEnabled=false in root vercel.json disables automatic Vercel Git deployments on all branches containing that configuration. Rejected or unapproved deployment does not undo publication; failed release publication skips deployment |
| Runtime secrets | Keep database, signing, provider and cron credentials in the existing Vercel project's production environment; CI does not download them. Runtime `DATABASE_URL` must target the same database as the migration URL; this workflow does not set or change it |
| Database | Before deployment, `sqlx --no-dotenv migrate run --source services/api/migrations` applies pending migrations from the selected commit; reruns skip already-applied migrations. The dedicated secret becomes `DATABASE_URL` only for SQLx; preflight checks its presence, and install/build/deploy commands do not receive it. Container startup still connects without migrating |
| Migration failure | Fails the job before any Vercel deployment. Successfully committed migrations remain if a later deployment fails; migrations must remain compatible with the currently running application |
| Verification boundary | Controlled local checks verify workflow contracts and shell behavior; host environment settings and actual approval waiting require separate GitHub readback. Manual-only triggering applies after this workflow is merged; old branches must include the updated vercel.json to disable their Vercel Git deployments, and already queued/running jobs are unaffected |

Never copy `.env` or local credentials into images or static output. Local validation does not provision, link, migrate or deploy a cloud project.

| Supabase migration connection | Setting |
| --- | --- |
| GitHub secret | Set `SUPABASE_MIGRATION_DATABASE_URL` in repository Actions secrets; do not commit it or paste its password into logs |
| Supplied direct endpoint | `postgresql://postgres:<percent-encoded-password>@db.jbcvsgdvhvzitweteqfo.supabase.co:5432/postgres?sslmode=require` |
| Network choice | The direct endpoint uses IPv6 unless the Supabase IPv4 add-on is enabled. For an IPv4-only runner, copy the **Session pooler** connection from the project's Dashboard Connect panel: port `5432`, database `postgres`, username `postgres.jbcvsgdvhvzitweteqfo`; the pooler host is project/region-specific and must be copied, not guessed |
| Pooler mode | Use the direct connection or session pooler for migrations; avoid transaction pooler port `6543` because SQLx migration locking needs session semantics |
| TLS and escaping | Keep `sslmode=require` in the URL and percent-encode reserved password characters; use the project's database password, not an API key |
| Production runtime | Configure Vercel's production `DATABASE_URL` separately for the same Supabase database; the migration secret is not copied to Vercel |

Connection modes and network support follow [Supabase's PostgreSQL connection guide](https://supabase.com/docs/guides/database/connecting-to-postgres). These local checks do not establish live Supabase connectivity or permissions.
