---
name: monolith-error-handling
description: Use when adding, changing, or reviewing API errors, HTTP status mappings, public error messages, or error source diagnostics in this repository.
---

# API error handling

Read root [AGENTS.md](../../../AGENTS.md), the shared [AppError](../../../services/api/src/error.rs), and the affected callers and tests before changing error behavior.

## Shared representation and descriptive messages

- Reuse the generic `AppError` status/message representation, optional typed cause, and response metadata. Different messages do not need new enum variants, error types, or endpoint-specific response wrappers. Keep typed client/protocol errors when retry or recovery logic consumes their distinctions.
- Choose a public message at the layer that understands the failure. Name the affected resource or invalid input and explain the problem, for example `Shopify location not found` rather than `Record Not found`. Preserve intentional authentication messages that conceal account existence.
- Encode public failures through the shared domain `ErrorResponse` and central `IntoResponse` implementation. Keep HTTP status and authentication headers explicit; never infer them by matching message text.
- Retain original SQLx, JWT, configuration and upstream causes privately. Unexpected failures return a safe `Internal server error`; raw driver/provider messages, credentials and tokens must not become public responses.

## HTTP status selection

Select status from the failure semantics using [HTTP Semantics](https://www.rfc-editor.org/rfc/rfc9110.html#section-15), then preserve the endpoint's agreed contract. Do not turn every failure into 400 or 500, return 200 for an error, or blindly forward an upstream status.

| Failure | Status |
| --- | --- |
| Malformed request or invalid input under the existing validation contract | 400 Bad Request |
| Missing, invalid or expired authentication; rejected login credentials | 401 Unauthorized |
| Authenticated caller lacks permission | 403 Forbidden |
| Requested resource does not exist | 404 Not Found |
| Known conflict with current state, such as an existing username | 409 Conflict |
| Unexpected application or database failure | 500 Internal Server Error |
| Upstream request failed or returned unusable data | 502 Bad Gateway |
| Dependency required by this operation is unavailable | 503 Service Unavailable |
| Upstream request timed out | 504 Gateway Timeout |

Current compatibility rules: missing admin targets in GET/PATCH/DELETE return 401 with `Not authorized` and `WWW-Authenticate: Bearer`; missing Shopify locations return 404 with `Shopify location not found`. Bearer authentication failures retain their challenge; login credential failures retain the existing response without that header. Treat these as existing contracts, not rules for all future resources or authentication schemes.

## Source diagnostics

- `AppError::new` and typed `From` conversions automatically capture file/line/column through `#[track_caller]` and `Location::caller()`. Preserve that first location plus ordered operation context; do not rewrap an existing AppError just to propagate it. Boundary logs use `origin` for the captured location and optional `client_origin` for retained typed client metadata. A conversion location is not necessarily the original failure location inside a dependency.
- Use direct construction or `?` for conversion-only propagation. For tracked mappers such as `upstream`, use an explicit closure at the mapping site; passing a tracked function directly as a callback can lose caller provenance. Keep needed context/status/header transformations in those closures. Capture inside async/blocking work where the error occurs; do not annotate async functions or expect tracking across task scheduling. Keep meaningful `ClientError::at(...)` metadata; AppError needs no manual `.at(...)` annotations.
- Emit once at the HTTP boundary or terminal background-task boundary. Keep ordinary logs and `Debug` sanitized; retain original causes privately and emit sanitized cause details and operation context in explicit DEBUG diagnostics. Continue sanitizing nested/shared client errors, including provider extensions and payloads.

## Verification

Assert exact status, public JSON and applicable authentication headers through the production mapping path. Verify retained causes/origins and redaction in public bodies, ordinary logs and DEBUG diagnostics. Cover the changed failure and nearby success/authorization behavior using the existing API check/test recipes; use [the E2E skill](../monolith-api-e2e/SKILL.md) for endpoint tests. Synchronize changed public API contracts in Postman using [the Postman skill](../monolith-api-postman/SKILL.md).
