# Backoffice

Prerequisites: Rust 1.99.0 with `wasm32-unknown-unknown`, just 1.58.0, Dioxus CLI 0.7.10, and a running API.

Run these commands from this directory:

```sh
just setup
just serve
just build
just run
just test
just check
```

The local browser URL is http://127.0.0.1:8084. Tests run natively; `just check` covers native and WASM builds. Backoffice has no E2E suite.
