---
name: monolith-shopify-graphql
description: Use when adding, changing, removing, or reviewing Shopify GraphQL documents or their Postman requests in this repository.
---

# Shopify GraphQL and Postman

Read root [AGENTS.md](../../../AGENTS.md) and the affected Shopify client methods before editing. Application documents live under `services/api/src/client/shopify/`; matching Postman requests live under `postman/collections/external/shopify/`.

1. For every added, changed, or removed `.graphql` document, add, update, or remove its corresponding Postman request in the same change. Include child-pagination and supporting queries; updating only the public API request is insufficient.
2. Keep the Postman `query` identical to the application document, including operation name, variables, fields, pagination and directives. Keep example variable names/types aligned, supply required variables, and use null initial cursors. Use synthetic IDs and existing `{{shopify}}` configuration; never embed credentials or private store data.
3. Preserve unrelated request settings and local edits. Reuse and extend existing GraphQL documents, client methods and models when they can meet the requirement. Keep supporting pagination/location queries beside the existing documents. Preserve established Postman request names; remove obsolete duplicate requests when consolidating queries.
4. Parse the changed YAML and its JSON variables, compare each query with its source, and check that required GraphQL variables are supplied without obsolete keys. Run local Postman collection lint when available and distinguish baseline warnings from introduced failures. Static validation does not establish live Shopify execution.

During review, inspect the GraphQL diff and corresponding Postman diff together. A GraphQL change without its matching Postman update is incomplete.
