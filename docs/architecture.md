# Service architecture

The application consists of an Axum API, a client-only Dioxus backoffice, shared wire contracts, and a placeholder client UI. [Development and operations](development.md) describes configuration and commands; [verification](verification.md) records measured results and limits.

## Ownership and dependencies

```mermaid
flowchart LR
    UI["Dioxus backoffice"] -->|"HTTP, cookies, Origin"| API["Axum routes and handlers"]
    API --> SERVICES["Services: use cases and transactions"]
    SERVICES --> REPO["Repositories: SQLx queries"]
    REPO --> DB[("PostgreSQL")]
    DOMAIN["Shared request/response DTOs"] -.-> UI
    DOMAIN -.-> API
    CALLER["Future caller"] -.-> SHOPIFY["ShopifyClient: automatic query pagination"]
    SHOPIFY --> GQL["GraphqlClient"]
    GQL --> HTTP["HttpClient: reqwest pool"]
    HTTP --> REMOTE["Shopify Admin GraphQL"]
    CONFIG["API typed configuration"] --> API
    CONFIG --> SHOPIFY
```

| Package | Responsibility | Boundary |
| --- | --- | --- |
| `services/api` | HTTP endpoints, authentication, authorization, persistence, outbound clients | Database objects, principals, JWT claims, and Shopify wire DTOs remain API-owned |
| `crates/domain` | Shared request/response DTOs and wire enums | No database models, SQLx, JWT claims, or transactions |
| `apps/backoffice` | Browser pages, forms, direct API requests, session coordination | Public configuration only; Axum owns cookies, CORS, and origin checks |
| `apps/client-ui` | Placeholder client application | No implemented service integration |

The product report service explicitly constructs ShopifyClient from state.settings.client.shopify. Routes delegate report work to the service; startup, scheduled jobs and persistence do not invoke Shopify.

## Module responsibilities

| API path | Responsibility |
| --- | --- |
| `src/lib.rs` | Module declarations and public re-exports, including `shopify_models` |
| `src/main.rs` | Configuration, database startup, listener, maintenance task, graceful shutdown |
| `src/config.rs` / `src/state.rs` | Typed settings; pool, settings, and signing key in application state |
| `src/routes/` / `src/handlers/` | Route composition; HTTP inputs, responses, and transport |
| `src/middleware/cookies.rs` | Cookie helpers, session transport, origin enforcement |
| `src/middleware/token.rs` | JWT validation and role checks |
| `src/services/` / `src/repositories/` | Use cases and transaction boundaries / SQL queries |
| `src/models/` | Persistence/authentication models and API-owned Shopify contracts |
| `src/validations/` / `src/utils/password.rs` | Input rules / Argon2 operations |
| `src/error.rs` | Public error mapping and centralized boundary logging |
| `src/client.rs` | Generic HTTP/GraphQL request envelopes, decoding, failures, throttling backoff |
| `src/client/shopify.rs` / `src/client/shopify/*.graphql` | Shopify authentication, seven automatically paginated queries and two raw mutations / matching query documents |
| `src/client/shopify/report.rs` / `report/*.graphql` | Complete report catalog/variant pagination and tagged-order metadata lines |
| `src/tests/` | Tests mirroring implementation paths through test-gated child modules |
| `e2e-test/` | Real TCP endpoint tests and shared application/database fixtures |
| `migrations/` / `.sqlx/` | Database migrations / offline SQLx metadata |

| Backoffice path | Responsibility |
| --- | --- |
| `src/lib.rs` / `src/main.rs` | Module declarations / startup and root App |
| `src/config.rs` / `application.toml` | Single typed startup reader / published public settings |
| `src/api.rs` / `src/api/{auth,admins}.rs` | HTTP construction, decoding, errors / endpoint calls |
| `src/session/` | Viewer state, response ownership, browser synchronization |
| `src/route.rs`, `src/pages/`, `src/components/` | Navigation, pages, forms, confirmation, reusable UI |
| `src/tests/` | Configuration, HTTP boundary, and session tests |

## HTTP and authentication

```mermaid
sequenceDiagram
    participant UI as Browser
    participant API as Axum API
    participant DB as PostgreSQL
    UI->>API: Login with cookie mode and Origin
    API->>DB: Verify account and create refresh family
    API-->>UI: Safe identity response and HttpOnly cookies
    UI->>API: Authenticated request with cookies
    API->>API: Validate JWT and authorize operation
    API-->>UI: Response DTO
    UI->>API: Refresh with cookie and Origin
    API->>DB: Atomically consume token and insert replacement
    API-->>UI: Rotated credentials
```

| Concern | Behavior |
| --- | --- |
| Login | Argon2 verification; independent refresh family for each login |
| Access token | JWT validation; 10-minute lifetime and 60-second validation leeway |
| Refresh | Single-use consumption and replacement in one transaction; seven-day refresh lifetime |
| Logout | Revoke the current authenticated account's refresh family |
| Account/password changes | Update and account-wide refresh revocation in one transaction |
| Account deletion | Foreign-key cascade removes refresh credentials |
| Revocation limit | Existing access JWTs remain valid until expiry; no immediate access-token revocation |
| Concurrent issuance | An in-flight login may issue after a concurrent password change; no cross-request global locking |
| Write authorization | Route checks and current actor lookup in services |
| Retention | Startup/hourly cleanup; at most 500 expired/revoked rows per batch with `SKIP LOCKED` |

| Browser transport | Policy |
| --- | --- |
| Cookie mode | `x-session-mode: cookie`; credentialed requests; identity responses exclude token pairs |
| Cookie attributes | HttpOnly, SameSite=Lax, Path=/; secure `__Host-backoffice_auth` in HTTPS mode |
| Local mode | Explicit loopback HTTP permits `backoffice_auth`; UI and API use `localhost` |
| Bearer clients | Token JSON remains supported; an invalid supplied bearer token does not fall back to cookies |
| CORS/origin | Exact configured browser origin; reject missing/null/foreign origins on guarded cookie mutations |
| Caching | Cookie-guarded responses use `Cache-Control: no-store` |

## Browser state and configuration

```mermaid
flowchart LR
    FILE["Public application.toml"] --> LOAD["Load typed settings once"]
    LOAD --> RESTORE["Restore session"]
    EVENTS["Login, logout, profile change"] --> LOCK["Browser Web Lock"]
    LOCK --> REQUEST["Direct API request"]
    REQUEST --> OWNER["Epoch and sequence ownership"]
    OWNER --> VIEWER["Safe viewer signals"]
    VIEWER --> SYNC["BroadcastChannel"]
    SYNC --> RESTORE
```

| Concern | Mechanism |
| --- | --- |
| Startup | Resolve the configuration asset against the current page URL; render pages and call APIs only after loading |
| Failure | Configuration error/Retry view; no configured application calls until recovery |
| Cookie-changing operations | Same-origin Web Locks serialize browser operations |
| Stale responses | Epoch/sequence checks reject results from an earlier account state |
| Other tabs/focus | Broadcast notifications and refresh-based restoration |
| Pagination | Dioxus resources own fetch lifecycle; disable navigation while pending |
| Deployment configuration | Publishing selected local/prod source as one `application.toml` is planned; selection is not implemented |

## Outbound Shopify operations

| Layer | Contract |
| --- | --- |
| `HttpClient` | Reusable reqwest pool; per-request generic headers; 30-second per-attempt timeout; redirects disabled |
| `GraphqlClient` | Typed variables; raw envelope error classification before successful data decoding; reject partial-data errors |
| `ShopifyClient` | General workflow methods and report-specific queries use aligned operation/document names and collect continuation pages internally; constructor supplies the token header and trusted configured Admin API URL, version 2026-10 |
| Request types | Own operation validation, resource-kind checks and search/variable construction; the client supplies internal continuation cursors |
| Shopify models | Connection and PageInfo live beside Shopify contracts; the shared collector imports them |
| DTOs | Public through `api::shopify_models`; shop-only money sets retain decimal amounts and currency; nullable references remain nullable; queries return complete Vec node collections, nested product variants contain nodes only, mutations retain raw userErrors and optional results |
| Configuration | `config.rs` uses one ConfigBuilder build/deserialization; dotenv/environment DATABASE_URL, JWT_SECRET and SHOPIFY_ACCESS_TOKEN override matching file values before deserialization; `[client.shopify]` in TOML contains only `admin_api_url` |
| Initialization | Absent Shopify configuration remains optional without a token override; a token override requires its file-configured admin URL; client construction requires a nonblank token |

| Client method / GraphQL filename stem | GraphQL operation | Result |
| --- | --- | --- |
| products_with_variants | ProductsWithVariants | Vec<Product>; all products, each with first250 variants, SKU/images/location quantities and contextual price/currency |
| orders_by_payment | OrdersByPayment | Vec<Order>; all payment-ID matches, contacts/transactions and money |
| orders_by_tag | OrdersByTag | Vec<TaggedOrderHeader>; all newest-first matching headers, tags and shop money |
| sold_variants_in_range | SoldVariantsInRange | Vec<SalesOrderHeader>; all offset-aware half-open date matches, statuses/test/cancellation |
| sales_order_lines | SalesOrderLines | Vec<SalesLine>; every line page for the supplied order, quantities and original money |
| tagged_order_lines | TaggedOrderLines | Vec<TaggedOrderLineItem>; every line page for the supplied order |
| replace_order_tags | ReplaceOrderTags | Raw userErrors and optional updated order; empty tags clears the complete list |
| find_inventory_to_activate | FindInventoryToActivate | Vec<InventoryActivationCandidate>; all raw location levels, including active/untracked/zero stock |
| activate_existing_inventory | ActivateExistingInventory | Raw userErrors/optional inventory level; supplied UUID; quantity arguments absent |

Queries accept requests without cursor arguments and preserve response order across pages. General header and line-item methods remain independent; the product report service assembles tagged orders and selects inventory report rows. Callers inspect raw inventory and mutation results.

| Request policy | Guarantee / limit |
| --- | --- |
| Pagination | General workflow queries use 250 nodes per request; report queries use smaller pages and collect nested variant continuations. Every method call starts without a cursor; line methods use the supplied order ID and reject missing/mismatched parents. Missing/repeated/cyclic cursors fail. Products retain first:250 variants as nodes only, without nested pageInfo or a variant cursor; truncation is not reported |
| Throttling | HttpClient owns one retry loop for HTTP 4xx and data-less all-THROTTLED GraphQL responses; GraphqlClient supplies the envelope classifier |
| Budget | Four total attempts; 1/2/4-second backoff plus 0–250 ms jitter; total retry sleep at most 30 seconds |
| Retry-After | Honor seconds/HTTP-date minimums; stop rather than exceed the sleep budget or overflow duration arithmetic |
| No replay | HTTP 3xx/5xx, network ambiguity/timeouts, decoding errors, partial data, and mutation user errors |
| Mutations | Preserve exact variables and idempotency key during throttling retries; discovery never activates automatically |
| Sales semantics | Callers calculate/filter; current quantities can reflect refunds after the selected order-creation range |
| Platform limits | No snapshot isolation, bulk-export guarantee, or bypass of Shopify limits. Nested 250-by-250 query cost remains unverified against the 1,000-point maximum; non-throttle cost errors propagate without retry |
| Compatibility | Discovery retains Shopify's supported deprecated singular inventory-item `variant` field |

## Errors and operational boundaries

| Area | Contract |
| --- | --- |
| API failures | Sanitized public responses; one WARN/ERROR summary at the handling boundary with operation, category, and innermost origin |
| API DEBUG diagnostics | Underlying causes/context are opt-in; conversions do not log |
| Client failures | Shopify entry-point failures carry origin api::client::shopify through ClientError::Origin, retaining typed causes and lower-level transport/decode origins. Generic unwrapped client errors retain their existing origins. One ERROR at AppError response/background handling; no events during creation/conversion/propagation; safe Display/Debug |
| Sensitive fields | Configuration Debug redacts secrets; do not log raw requests, upstream bodies, or client error fields |
| Persistence | Transactions remain service-owned, SQL remains repository-owned; intentional admin/seed migrations are preserved |
| Runtime | Standalone Axum listener and Tokio maintenance; no serverless adapter or production deployment mechanism |
| Capacity | Default database pool has one connection; throughput and deployment guarantees require separate evidence |

## Product reports

| Concern | Behavior |
| --- | --- |
| Route | `GET /v1/products?type=INVENTORY\|ORDER_TAG&id=...`, existing ADMIN/SUPER_ADMIN bearer or cookie protection |
| Contract | Domain product report DTOs only; API owns Shopify models, validation and report orchestration |
| Inventory | Numeric location ID; active products and stocked variants; on_hand includes zero/negative quantities; missing location is 404 |
| Metadata | Category = productType; collection = SKU characters 4–9 inclusive, null for short/missing SKU; cost and weight are current inventory metadata |
| Orders | Literal tag; original quantities and original unit shop-money prices; merge SKU + price + currency with compatible cost/weight/category; missing SKUs stay separate |
| Completeness | Report-specific product and variant queries paginate independently; order headers and report lines paginate independently; existing general catalog method retains its first250 variant contract |
| Precision | API checked decimal arithmetic; domain JSON numbers preserve decimal precision; overflow/invalid upstream numbers fail the whole report |
| Failures | 400 invalid input, 401 authentication, 404 location absent, 503 missing configuration, 502 upstream failure, 504 upstream timeout; sanitized responses |

```mermaid
flowchart LR
    HTTP[GET /v1/products] --> AUTH[Existing authentication]
    AUTH --> VALIDATE[Report query validation]
    VALIDATE --> REPORT[Product report service]
    REPORT --> INVENTORY[Paginated products and variants]
    REPORT --> TAG[Paginated tagged orders and lines]
    INVENTORY --> ROWS[Metadata and exact totals]
    TAG --> ROWS
    ROWS --> JSON[Products response]
```
