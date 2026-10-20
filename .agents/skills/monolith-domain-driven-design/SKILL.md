---
name: monolith-domain-driven-design
description: Use when adding or moving feature responsibilities, extracting components, or reviewing file cohesion and cross-domain dependencies in this Rust Axum and Dioxus repository.
---

# Domain ownership and file cohesion

Before placing new behavior, identify its owning capability and read the existing implementation, callers and repository rules. Use domain-driven design to preserve ownership and vocabulary; it does not require introducing domain layers or moving code into a directory named domain.

1. State which feature or shared concern owns each changed behavior. Follow existing consumers and dependencies rather than choosing the nearest convenient file. Apply the repository's reuse-existing-code skill before adding another implementation.
2. Keep routing declarations focused on route mapping and layout selection. Layout modules may compose the navbar, routed content and shared UI. Loading presentation, focus handling and keyboard guards belong with the loading component; feature-specific requests, result state and business decisions belong with the owning feature. A shared layout can host an overlay without owning inventory logic.
3. Group code that changes for the same reason. Extract a focused module when a file mixes independent responsibilities; do not split a small cohesive feature merely to make more files. Prefer feature or capability names over miscellaneous helpers, utils or catch-all services.
4. Keep cross-boundary interfaces narrow and explicit. In this repository, crates/domain contains shared request/response DTOs and wire enums only. Database models, SQLx mappings and queries stay in services/api; the backoffice consumes wire contracts through its API modules. A shared type is not a reason to share persistence or provider mechanics.
5. Reuse existing framework lifecycle and composition mechanisms. Do not add entities, aggregates, repositories, adapters or generic infrastructure solely to satisfy a DDD label. Add an abstraction only when an agreed requirement and actual consumers justify it.
6. Before completion, trace imports and callers through the final diff. Verify each file still owns one cohesive responsibility, feature behavior has not leaked into routing or shared UI, and extraction preserves lifecycle and observable behavior. Record any necessary cross-boundary dependency or justified new module briefly in the existing plan or review notes; do not create another approval workflow.

Repository-specific placement:

| Responsibility | Owner |
| --- | --- |
| Shopify operation methods | `services/api/src/client/shopify.rs`; supporting GraphQL documents in `client/shopify/`; services retain pagination and multi-call orchestration |
| Carrier-specific label builders | The shipping-label feature and its focused children, rather than miscellaneous shared utils |
| Static API templates | `services/api/assets/`, referenced by the owning feature |

For a broad architecture audit, use the existing monolith-architecture-review skill. This skill guides ownership decisions within the requested scope; it does not authorize unrelated restructuring.
