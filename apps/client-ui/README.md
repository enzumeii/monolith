# Customer Facing App

## Prerequisites

Run `../../init.sh` to install the CLI packages after installing Rust.

## Useful commands

| Command | Purpose |
| --- | --- |
| `just` | List recipes |
| `just test` | Run package tests; currently verifies compilation with no test cases |
| `just check` | Formatting and strict lint |
| `just fix` | Apply formatting and lint fixes |

## Coverage

`just test` runs the normal suite. `just test --coverage` writes `target/coverage/client-ui/lcov.info` relative to the repository root. Install pinned cargo-llvm-cov via the root `init.sh`; the pinned Rust toolchain includes LLVM tools.
