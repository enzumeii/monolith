# Backoffice [apps/backoffice]

Authenticated admin backoffice. Dioxus fullstack app (web frontend + axum
server) that proxies to the internal `services/api` over HTTP — it never talks
to the database directly.

## Pages

- **Login** — `/` while signed out; authenticates via `POST /v1/auth/login`.
- **Home** — `/` while signed in; empty apart from the top navigation.
- **Admin List** — `/admins`; super admins only. Paginated list of all admins
  with create, update, and delete (never self). Hidden from regular admins.
- **Profile** — `/profile`; any admin can update their own username/password.
  Saving signs you out (the API revokes your tokens), so you log in again.

## Auth & session

The session (access + refresh tokens) lives only in browser memory — reloading
the page signs you out. When the access token expires, the server transparently
refreshes it via `POST /v1/auth/refresh` (rotating the single-use refresh
token) before requests fail; if refresh is rejected, you're returned to login.

## Configuration

No value is defaulted in code. Configuration is read from `application.toml`
(committed, local default) or the environment (also loadable from a local
`.env`), with env vars taking highest precedence for deployment.

| Setting | `application.toml` | Env override | Default (local) |
| --- | --- | --- | --- |
| Upstream API base URL | `upstream.api_base` | `BACKOFFICE__UPSTREAM__API_BASE` | `http://127.0.0.1:3000/v1` |

`RUN_ENV` (default `local`) selects an optional `application.<RUN_ENV>.toml`
override file.

The web server's own bind address is controlled by the `IP`/`PORT` env vars
(set by `dx serve` for dev, or manually for deployment), per the Dioxus
fullstack convention — `dx serve --port <port>` is the easiest way in dev.

## Running locally

1. Start the internal API (Postgres + migrations + server on `:3000`):
   `cd services/api && just run`
2. Serve the backoffice (requires the Dioxus CLI, `cargo install dioxus-cli`,
   and the `wasm32-unknown-unknown` target):
   `cd apps/backoffice && just serve`
3. Open http://127.0.0.1:8080 (or the port shown by `dx serve`)

## Verification

- `just check` — format + strict Clippy for both the server and web (wasm) targets.
- `just build` / `just test` — build and test the server target.

## Deploying (Kubernetes)

Build a release bundle with `dx build --release`, then run the server binary.
Provide the upstream config and bind address via env vars, e.g.:

```sh
IP=0.0.0.0 \
PORT=8080 \
BACKOFFICE__UPSTREAM__API_BASE=http://api.internal.svc:3000/v1 \
./backoffice
```
