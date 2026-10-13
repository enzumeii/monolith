# Implementation and operations

## Ownership

`crates/domain` contains Serde request/response contracts and the wire role enum. It has no PostgreSQL, SQLx, UUID, or database feature dependency. API row models, database roles, JWT claims, validated pagination and SQLx conversions belong to `services/api`.

The API separates routes, HTTP handlers, transactional services, repositories and validation. A service owns authorization rechecks, row locking and commit boundaries. Implementation modules have crate visibility; startup, configuration and explicit operator commands expose deliberate library entry points.

Backoffice registers modules in `src/lib.rs`; `main.rs` delegates launch. `api/` holds Dioxus server functions and upstream HTTP decoding. `session/` holds safe viewer state, response ownership, browser synchronization, HttpOnly cookie transport and in-flight refresh coordination. UI views and pagination ownership remain under `pages/`.

## Authentication

The API issues 600-second JWTs with 60 seconds of validation leeway and single-use refresh tokens. Access JWTs intentionally remain valid after logout, account changes or deletion until their expiry window ends. There is no auth_version or opaque browser-session system.

Each login creates an independent refresh family. Rotation preserves that family and atomically consumes/inserts credentials in one transaction. Logout deletes only the authenticated browser's family. Account/password changes revoke all target-account refresh families in the same transaction as the account update; deletion cascades credentials. Login verifies its observed password hash again under an account lock before issuing credentials. Stable account-first locking coordinates refresh and updates.

Backoffice stores the access/refresh pair in one HttpOnly, SameSite=Lax cookie with a seven-day lifetime. Production uses `__Host-backoffice_auth`, Secure and Path=/ with no Domain attribute. Explicit loopback HTTP development uses `backoffice_auth`. Server-function JSON and SSR contain only safe viewer identity/expiry. Legacy LocalStorage credentials are removed by browser initialization.

There is no `/me` endpoint. Restoration parses cookie JWT sub/exp only to construct an existing `GET /v1/admins/{id}` request; the API verifies its bearer before returning the profile. Browser-supplied IDs cannot authenticate a session.

Origin Web Locks serialize cookie-changing operations across tabs, and BroadcastChannel/focus events revalidate viewer state. Web Locks support is required; unsupported browsers receive a retryable UI error. BFF refresh coordination is process-local, bounded to 128 unfinished flights with 40-second abandoned-flight expiry; completed credentials are never cached for replay. Cancelling one waiter preserves a pending flight for another waiter. Browser/device logins have independent cookies and token families.

Logout waits for upstream success before clearing the cookie and authenticated UI. Failed logout retains credentials and shows an error. Session generations reject stale restoration results after login/logout/account switching. Pagination generations reject stale successes and errors; accepted response.page owns the visible page identity, and an emptied non-first page steps back once.

## Inputs and errors

Usernames have 1–20 Unicode characters with no surrounding/all whitespace. New passwords have 12–128 characters; existing shorter passwords remain accepted at login. Invalid updates return 400; duplicate usernames return 409. JWT signing keys must be hex-encoded random bytes of at least 32 bytes; `just setup` generates a key privately in `.env`.

All API application error/warning events originate in `error.rs`, once at HTTP/startup/maintenance boundaries. Error conversions are pure. Logs contain static operation/category context rather than raw causes, credentials or database URLs. Configuration Debug hides secrets; credential contracts and database rows omit Debug. Internal HTTP failures expose a generic 500 response.

## Runtime

The API defaults to two simultaneous Argon2 jobs. `PASSWORD_CONCURRENCY` must be positive; saturated admission waits at most one second and returns 503 with Retry-After: 1. Permits live inside blocking work, so cancelling a request does not release a still-running job's permit. Default Argon2 memory is approximately 19 MiB per job; account for process and database overhead separately.

The existing database pool remains one connection; its size has not been increased without workload evidence. Backoffice reuses a reqwest client with a ten-second upstream timeout. API bind overrides are `API_HOST`/`API_PORT`; BFF overrides are `BACKOFFICE_HOST`, `BACKOFFICE_PORT`, `UPSTREAM_API_BASE` and `PUBLIC_ORIGIN`.

API and standalone BFF launches handle Ctrl-C/SIGTERM and allow up to 30 seconds for HTTP draining. Dioxus CLI launch retains the CLI-owned lifecycle. Hourly API maintenance deletes at most 500 expired/revoked refresh rows per tick, skips locked rows and preserves live refresh families. It is idempotent across instances.

Production requires HTTPS with an exact browser public origin. Set BFF `session.insecure_local=false` in the production configuration overlay and `PUBLIC_ORIGIN` to that HTTPS origin. The origin guard rejects missing/null/foreign origins on mutating server-function requests and applies no-store to API responses. Keep the internal API behind the intended network boundary.

## Migrations and bootstrap

All original migration scripts and intentional seed/index behavior are retained. New migrations add refresh-family identity and retention indexes. Do not rewrite original SQLx migration history. The unpublished auth_version drafts were removed before production use.

Optional `just bootstrap-admin USERNAME ROLE` creates an additional account, validates inputs, reads a password on non-terminal stdin, and fails on duplicate names without overwriting. A private input mechanism such as an interactive non-echo password reader should feed stdin; avoid putting passwords in command arguments or shell history.

Apply migrations before compatible binaries. Configure the generated/rotated JWT key and production origin/cookie settings, deploy the compatible API and BFF together. Key rotation invalidates existing access JWTs; still-valid refresh credentials can obtain new JWTs. Legacy browser storage changes require signing in again. Old BFF binaries expect browser-readable token storage and incompatible logout semantics; roll back deliberately or prefer a forward fix. Keep the original intentional seed policy when recovering accounts.

## Verification

API E2E stays under `services/api/tests/e2e`, grouped as healthz and v1/auth/admins to match routes. `common.rs` contains shared fixtures and HTTP utilities; endpoint helpers live in their owning v1 modules. Tests send real TCP requests to a local API and use PostgreSQL bootstrapped by existing Docker Compose. There is no API Dockerfile or backoffice/browser E2E requirement.

Every endpoint fixture owns a unique username prefix and uses actual account IDs. Fixture cleanup removes only owned rows and trigger controls. A PostgreSQL advisory lock serializes whole-table and race scenarios across processes; neither databases nor shared records are reset. The retention integration test uses connection-owned temporary rows and checks batch size, idempotency and live-row preservation.

Native BFF tests cover actual Dioxus server-function transport, cookie attributes/no credential JSON, CSRF origins, logout failure/success, refresh overlap/cancellation and stale session/pagination ownership. They are native integration/unit tests with controlled upstream fixtures. They do not establish real-browser multi-tab behavior; browser E2E was explicitly excluded from scope.

Run commands through existing package justfiles. Formatting, strict native/WASM lint, domain db-free builds, SQLx metadata and a Dioxus fullstack bundle build complement behavior tests. Coverage is diagnostic; tested invariants matter more than a blanket percentage threshold.
