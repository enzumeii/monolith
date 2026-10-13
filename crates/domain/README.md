# Domain

## Prerequisites and setup

Rust 1.99.0 with Clippy, rustfmt and `wasm32-unknown-unknown`; just 1.58.0. Run `../../init.sh` to install the CLI packages after installing Rust.

From `crates/domain`:

```sh
just setup
```

Setup checks that the library and its dependencies compile; it does not install prerequisite CLI packages.

## Run locally

This library has no executable or server. Run its contract tests locally:

```sh
just test
```

## Useful commands

| Command | Purpose |
| --- | --- |
| `just --list` | List recipes |
| `just check` | Formatting, strict lint and WASM compilation |
| `just fix` | Apply formatting and lint fixes |
