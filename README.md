# Monolith

Prerequisites: Rust 1.99.0 with Clippy, rustfmt and `wasm32-unknown-unknown`; Docker Compose; just 1.58.0; SQLx CLI 0.9.0; Dioxus CLI 0.7.10.

```sh
./init.sh
just --justfile services/api/justfile setup
just --justfile services/api/justfile run
just --justfile apps/backoffice/justfile setup
just --justfile apps/backoffice/justfile serve
```

```sh
just --justfile services/api/justfile test
just --justfile services/api/justfile check
just --justfile apps/backoffice/justfile test
just --justfile apps/backoffice/justfile check
```
