---
name: monolith-dependency-injection
description: Use when adding or changing reusable runtime dependencies, outbound clients, or AppState initialization in this repository.
---

# Reusable runtime dependencies

Read root [AGENTS.md](../../../AGENTS.md), `services/api/src/state.rs` and the dependency's actual callers.

- Construct reusable runtime clients once in AppState startup. Handlers validate HTTP input, resolve shared clients and map optional-dependency availability failures, then pass only concrete client/pool/key/configuration references to services. Services, clients and repositories must not accept AppState; middleware receives only its browser/JWT dependencies. Reuse existing HTTP pools; do not construct clients per request. Share ownership with Arc only where needed; do not add a generic DI container.
- Make initialization failure behavior intentional. Required dependencies may fail startup; optional dependencies must retain their unavailable/initialization-error state and fail only affected operations. Keep public errors and ordinary logs sanitized; original causes belong in DEBUG diagnostics.
- Keep business use-case orchestration in services; technical scheduler, publication and recovery coordination follows the existing [domain ownership boundaries](../monolith-domain-driven-design/SKILL.md). Client methods proxy one operation; root-paginated methods accept explicit size/cursor and return page metadata; services exhaust them when a complete report is required. Bounded nested variants/lines follow the API/Postman skill. Models own transformations of their own values; shared domain DTOs remain wire-only.
- Verify shared state reuses the same instance and initialization failures preserve the agreed HTTP behavior. For HTTP/GraphQL dependencies, use [monolith-api-postman](../monolith-api-postman/SKILL.md).
