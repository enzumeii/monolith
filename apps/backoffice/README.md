# Backoffice

## Prerequisites and setup

Rust 1.99.0 with Clippy, rustfmt and `wasm32-unknown-unknown`; just 1.58.0; Dioxus CLI 0.7.10; a running API. Run `../../init.sh` to install the CLI packages after installing Rust.

From `apps/backoffice`:

```sh
just setup
```

Setup checks Dioxus CLI availability and the installed WASM target, then creates `.env` if missing. It does not install prerequisites.

## Run locally

Start the API using its [README](../../services/api/README.md). Set `[api] base_url` in `application.toml` to the API URL, including `/v1`; the local default is `http://127.0.0.1:3000/v1`. The TOML is public and must contain no secrets.

```sh
just serve
```

Open [http://127.0.0.1:8084](http://127.0.0.1:8084).

## Useful commands

| Command | Purpose |
| --- | --- |
| `just --list` | List recipes |
| `just build` | Build the client web bundle |
| `just test` | Shared contracts, domain WASM check and native backoffice tests |
| `just check` | Formatting and strict native/WASM lint |
| `just fix` | Apply formatting and native/WASM lint fixes |
