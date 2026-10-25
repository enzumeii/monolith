# Backoffice App

## Prerequisites

Run `../../init.sh` to install the CLI packages after installing Rust.

## Run locally

- `just setup` to initiate `.env`
- Start the API using its [README](../../services/api/README.md) or `cd ../../services/api && just run`
- Start Backoffice client `just serve`
- Open [http://localhost:8084](http://localhost:8084).

## Useful commands

| Command | Purpose |
| --- | --- |
| `just` | List recipes |
| `just build` | Build the client web bundle |
| `just test` | Shared contracts, domain WASM check and native backoffice tests |
| `just check` | Formatting and strict native/WASM lint |
| `just fix` | Apply formatting and native/WASM lint fixes |

## Coverage

`just test` runs the normal suite. `just test --coverage` writes `target/coverage/backoffice/lcov.info` relative to the repository root. Install pinned cargo-llvm-cov via the root `init.sh`; the pinned Rust toolchain includes LLVM tools.

### Local configuration

Run root `just setup` with Node 24+ installed to prepare TOML composition dependencies.
`just serve` and `just build` read `RUN_ENV` from `.env`, defaulting to `local`.
An explicit argument such as `just serve qa` selects `application.qa.toml` when that file exists.
The production baseline `application.toml` is merged with the selected environment override;
source files are never rewritten. Release builds default to production and require no
production override unless production has additional differences.
