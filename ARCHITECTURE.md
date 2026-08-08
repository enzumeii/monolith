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