# Client App

## Prerequisites

Install Rust and Node.js 24, then run `../../init.sh` for pinned tools. `just setup` checks Dioxus and the WASM target.

## Run and verify

Run `just serve`, then open http://localhost:8085. Local public API settings are in `application.local.toml`; the command publishes them with the shared baseline without modifying source files.

| Command | Purpose |
| --- | --- |
| `just setup` | Check CLI and installed WASM target |
| `just build` | Build the local browser bundle |
| `just build prod` | Build with production public settings |
| `just serve qa` | Serve with a configured QA override |
| `just test` | Run portal, cart, configuration and browser-bridge tests |
| `just check` | Check formatting and strict native/WASM lint |
| `just fix` | Apply formatting and lint fixes |
| `just test --coverage` | Write target/coverage/client-ui/lcov.info from repository root |

### Local configuration

Run root `just setup` with Node 24+ installed to prepare TOML composition dependencies.
`just serve` and `just build` read `RUN_ENV` from `.env`, defaulting to `local`.
An explicit argument such as `just serve qa` selects `application.qa.toml` when that file exists.
The production baseline `application.toml` is merged with the selected environment override;
source files are never rewritten. Release builds default to production and require no
production override unless production has additional differences.
