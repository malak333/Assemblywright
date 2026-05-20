# Build And Test Commands

Run commands from the repository root unless noted otherwise.

## Required Local Gate

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

## Current Health Check

```sh
cargo run -p jarvis-cli -- health
```

Expected output:

```text
jarvis-core: ok
```

## Useful Focused Commands

```sh
cargo test -p jarvis-core
cargo test -p jarvis-core --test e2e_scaffold
cargo test -p jarvis-cli
```

## Release Evidence Boundary

Passing these commands proves the current Rust workspace builds and its tests
pass. It does not prove the future Swift shell, app packaging, IPC lifecycle,
SQLite migrations, plugin runtime, model routing, memory store, scheduler, or
Mac release smoke test until those surfaces exist and are covered.
