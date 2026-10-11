# Enzumeii Monolith

A Rust monolith: one HTTP API service plus two Dioxus fullstack apps over a shared domain crate.

- **`services/api`** — the backend. See [README](services/api/README.md)
- **`apps/`** — Dioxus fullstack apps.
- **`crates/domain`** — shared types at the app ↔ api boundary.

## Quick start
```bash
./init.sh                          # installs just, sqlx-cli
```
