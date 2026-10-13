# Client UI

## Prerequisites and setup

Rust 1.99.0 with Clippy and rustfmt; just 1.58.0. Run `../../init.sh` to install the CLI packages after installing Rust.

From `apps/client-ui`:

```sh
just setup
```

Setup checks that the current package and its dependencies compile; it does not install prerequisite CLI packages.

## Run locally

```sh
just run
```

The current executable prints `client-ui: hello stub` and exits. There is no browser URL yet.

## Useful commands

| Command | Purpose |
| --- | --- |
| `just --list` | List recipes |
| `just test` | Run package tests; currently verifies compilation with no test cases |
| `just check` | Formatting and strict lint |
| `just fix` | Apply formatting and lint fixes |
