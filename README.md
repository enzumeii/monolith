# Enzumeii Monolith

A Rust monolith: one HTTP API service plus two Dioxus fullstack apps (currently
stubs) over a shared domain crate.

- **`services/api`** — the backend. See `services/api/README.md` for setup,
  running, testing, and the available `just` commands.
- **`apps/client-ui`**, **`apps/backoffice`** — Dioxus fullstack apps (stubs).
- **`crates/domain`** — shared types (DTOs + DB entities) at the app↔api
  boundary.
- **`ARCHITECTURE.md`** — service graph and credential-isolation rules.

## Quick start

```bash
./init.sh                 # installs just, sqlx-cli, bacon; scaffolds services/api/.env.local
cd services/api && just api   # start Postgres, run migrations, serve the API
```
