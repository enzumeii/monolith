# Architecture — service graph

Three runnable units + one shared types crate. All third-party credentials
(Shopify Admin API, Stripe, future integrations) live in **one** place:
`services/api`.

```mermaid
flowchart TB
    subgraph users["Browser"]
        shopper["👤 Customer browser"]
        admin["👤 Admin browser"]
    end

    subgraph apps["Dioxus fullstack apps (FE + BE in one crate)"]
        store["apps/client-ui · storefront<br/>SSR/SSG + public routes"]
        bff["apps/backoffice · dashboard<br/>session-auth routes"]
    end

    subgraph you["Your deployment"]
        api["services/api · sole credential holder<br/>/api/internal/* · /api/webhooks/*"]
        future["future integrations<br/>+ your own BE code"]
    end

    subgraph ext["External SaaS"]
        shopify["Shopify Admin API"]
        stripe["Stripe Checkout"]
        sweb["Stripe webhook"]
    end

        shopper -->|"browse · cart · checkout"| store
    admin -->|"login → dashboard"| bff

    store -->|"HTTP /api/internal/* · internal token"| api
    bff -->|"HTTP /api/internal/* · internal token"| api

    api -->|"REST — Admin key in env"| shopify
    api -->|"REST — secret key in env"| stripe
    stripe -.->|"checkout.session.completed"| sweb
    sweb -.->|"POST /api/webhooks/stripe"| api
    api -.->|"extend here"| future
```

## Rules that make this safe

1. **Credential isolation** — only the api service holds Shopify/Stripe secrets. A
   compromised storefront cannot reach the Admin API or charge cards.
2. **Apps talk to the api service, never to SaaS** — shared internal token over
   `/api/internal/*`. The api service translates into third-party calls.
3. **`crates/domain` is the contract** — Money, Product, Order, Report types
   serialized at the app↔api boundary; no other cross-crate data flow.
4. **Extending the api service** — add `integrations/<name>.rs` (shopify, stripe,
   then yours): one small namespace, one place to audit keys. See ROADMAP.

## `services/api` code style — functional core, imperative shell

The api service is written in a **pure-functional style**: no domain structs with
methods and no domain traits/interfaces. Everything is a free function; the only
effect (Postgres) is passed explicitly as `&PgPool`.

- **Pure core** (`services/*`): deterministic decisions — pagination shaping
  (`build_list_response`), delete authorization (`resolve_delete`), own-record
  authz (`ensure_self`), and token math (`generate_jwt`, `generate_random_token`,
  `hash_token`). These are plain functions over values and are unit-tested with no
  database, no mocks, no async.
- **Imperative shell** (`repositories/*`, the async parts of `services/*`):
  `pub async fn`s that take `&PgPool`, run the SQL, and hand results to the pure
  core. Tested against a real, disposable Postgres via `testcontainers`.
- **Handlers/middleware** are plain `pub async fn`s (no `AdminHandler`/
  `AuthHandler`/`Middleware` wrapper structs). Routers reference them directly,
  e.g. `get(admin::list)`.
- **`AppState`** is a plain data record `{ pool, settings }` built by
  `state::app_state`. There is no dependency-injection container and no
  `Arc<dyn Trait>` seam — with no mocking, a trait object would be dead weight.

What is kept and why: `impl IntoResponse for AppError`, `#[derive(FromRow)]`, and
`sqlx::Type` on domain types are **framework adapter typeclass instances**, not
OOP — they are required for axum/sqlx interop and carry no domain behavior.

### Testing

- Function-level tests: `cargo test -p api --lib` (no Docker, no Postgres). CI runs these.
- E2e tests: `cargo test -p api --test e2e` — one test per API endpoint against a real
  Postgres via testcontainers. Local only (needs Docker); not gated in CI.

`mockall` and hand-written fakes were removed. There is no mock store: services take
`&PgPool` directly and the e2e suite exercises the real database.