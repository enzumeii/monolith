# DTO Objexts

## Prerequisites and setup

Run `../../init.sh` to install the CLI packages after installing Rust.

## Coverage

`just test` runs the normal suite. `just test --coverage` writes `target/coverage/domain/lcov.info` relative to the repository root. Install pinned cargo-llvm-cov via the root `init.sh`; the pinned Rust toolchain includes LLVM tools.
