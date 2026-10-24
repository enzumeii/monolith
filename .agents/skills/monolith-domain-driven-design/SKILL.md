---
name: monolith-domain-driven-design
description: Use when planning, implementing, moving, or reviewing responsibilities across services, clients, repositories, API wrappers, background runtimes, and UI features in this Rust Axum and Dioxus repository.
---

# Domain ownership and file cohesion

Before placing new behavior, identify its owning capability and read the existing implementation, callers and repository rules. Use domain-driven design to preserve ownership and vocabulary; it does not require introducing domain layers or moving code into a directory named domain.

1. State which feature or shared concern owns each changed behavior. Follow existing consumers and dependencies rather than choosing the nearest convenient file. Apply the repository's reuse-existing-code skill before adding another implementation.
2. Keep routing declarations focused on route mapping and layout selection. Layout modules may compose the navbar, routed content and shared UI. Loading presentation, focus handling and keyboard guards belong with the loading component; feature-specific requests, result state and business decisions belong with the owning feature. A shared layout can host an overlay without owning inventory logic.
3. Group code that changes for the same reason. Extract a focused module when a file mixes independent responsibilities; do not split a small cohesive feature merely to make more files. Prefer feature or capability names over miscellaneous helpers, utils or catch-all services.
4. Keep cross-boundary interfaces narrow and explicit. In this repository, crates/domain contains shared request/response DTOs and wire enums only. Database models, SQLx mappings and queries stay in services/api; the backoffice consumes wire contracts through its API modules. A shared type is not a reason to share persistence or provider mechanics.
5. Reuse existing framework lifecycle and composition mechanisms. Do not add entities, aggregates, repositories, adapters or generic infrastructure solely to satisfy a DDD label. Add an abstraction only when an agreed requirement and actual consumers justify it.
6. Before completion, trace imports and callers through the final diff. Verify each file still owns one cohesive responsibility, feature behavior has not leaked into routing or shared UI, and extraction preserves lifecycle and observable behavior. Record any necessary cross-boundary dependency or justified new module briefly in the existing plan or review notes; do not create another approval workflow.

## Layer responsibilities

Apply these boundaries to every child module. Classify code by its behavior and dependencies, not by its filename or feature grouping.

`services/api/` is the workspace application directory. The business-only rule applies to its service layer, `services/api/src/services/`; it does not require relocating the application.

| Layer | Owns | Keep elsewhere |
| --- | --- | --- |
| `services/` | Business rules, eligibility, calculations, business validation, business state transitions, and use-case orchestration through injected clients/repositories | Scheduler engines, transport implementations, SQL, direct filesystem operations, and technical recovery machinery |
| `client/` | Proxies to external APIs/SDKs: provider operations, authentication, timeouts, protocol encoding/decoding, and transport errors | Report policy, business payload decisions, aggregation, and multi-provider workflows |
| `repositories/` | Proxies to the database: queries, row mapping, persistence, and atomic database operations | HTTP handling, business policy, and background execution loops |
| `route/`, handlers | Route mapping, extraction, authentication context, service invocation, and response/status mapping | Business eligibility, domain decisions, provider calls, and SQL |
| `validations/` | Pure request shape/type/required-field checks, canonical parsing, and bounded content decoding | Service imports/calls, business eligibility/capability checks, provider calls, and SQL |
| `middleware/` | Cross-cutting request transport and authentication/authorization enforcement | Feature-specific business decisions |
| Top-level cron/runtime or worker modules | Cron parsing, next-fire calculation, timers, task lifecycle, polling, dispatch, leases, technical retry/backoff, and recovery coordination | Report calculations, eligibility, and business period rules |
| Storage/export components | File/Blob operations, paths, checksums, publication, and byte serialization such as CSV/ZIP/XLSX | Report eligibility, business calculations, and choice of report content |
| Configuration readers | Typed application settings and loading configuration | Feature execution and business decisions |
| Application startup / `AppState` | Dependency construction, shared instances, runtime startup and shutdown | Request-specific business logic |
| `crates/domain` | Shared request/response DTOs and wire enums | SQLx models, persistence, provider mechanics, and application runtime |
| Owning UI feature | Feature state, interactions, requests, and presentation | Shared routing/layout ownership of feature-specific behavior |

Services contain only business logic, including the sequence of a business use case. They may call injected clients and repositories; the proxy implementations remain in their owning layers. Services choose business transaction boundaries and pass transaction context to repository operations; SQL and concrete database operations stay in repositories. Do not introduce interfaces or generic frameworks solely to satisfy this separation.

Endpoint validation checks that an input can be interpreted. Services decide whether the interpreted operation is valid for the business. For example, parsing a date belongs at the endpoint boundary; deciding which reporting period is allowed belongs to the business service. When a service also needs a pure request parser, both callers reuse its validation owner; validators do not call a business service to reach that parser. Currency-dependent payment capabilities remain in the convention service, including revalidation after merging an update.

For scheduled reports:

- Keep report eligibility, report content, aggregation, and Bangkok reporting-period rules in business services.
- Keep cron expression parsing, timer loops, queue claims, lease renewal, technical retries, and worker recovery outside services.
- Keep provider HTTP/SDK calls in clients and SQL in repositories.
- A module called `services/cron/job.rs` is acceptable only for business logic. Its name does not authorize placing runtime machinery there.
- Keep business decisions about delivery and audit outcomes in the business use case; delegate actual email transport, audit persistence, and file publication to their owning components.

Before implementation, include the responsibility-to-file mapping in the existing plan. Before completion, inspect changed bodies, imports, helpers, and callers for responsibilities crossing these boundaries. Record a violation with its file/line and intended owner. Urgency, existing violations, feature cohesion, or fewer files do not justify adding technical components to services. Preserve observable behavior, audit, idempotency, cancellation, and recovery guarantees when moving code.

Reuse the repository's existing modules and names; add only the smallest missing component. Apply this guidance within the requested scope without triggering unrelated restructuring.

## Repository-specific placement

| Responsibility | Owner |
| --- | --- |
| Generic file upload use case | `services/api/src/services/files.rs` orchestrates destination selection, UUID naming and injected S3 presign/put operations; the generic files handler calls this capability rather than a convention-logo service |
| Upload request and image validation | `services/api/src/validations/files.rs` owns canonical upload-directory parsing and bounded image decoding; convention logo-location parsing belongs to `validations/conventions.rs`. Preserve the accepted namespace rather than widening it during extraction |
| Convention logo association | `services/api/src/services/conventions/logo.rs` owns attachment eligibility, convention/slug ownership and preview projection; reuse shared file validation without moving convention policy into it |
| S3 provider operations | `services/api/src/client/s3.rs` owns SDK calls, request encoding, network transport and timeouts. Upload use-case sequencing stays in the files service; it does not need a new top-level storage module |
| Shopify operation methods | `services/api/src/client/shopify.rs`; supporting GraphQL documents in `client/shopify/`; services retain pagination and multi-call orchestration |
| Carrier-specific label builders | The shipping-label feature and its focused children, rather than miscellaneous shared utils |
| Static API templates | `services/api/assets/`, referenced by the owning feature |

For a broad architecture audit, use the existing monolith-architecture-review skill. This skill guides ownership decisions within the requested scope; it does not authorize unrelated restructuring.
