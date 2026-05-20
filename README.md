# Jarvis

Jarvis is a local-first macOS assistant foundation. The current repo implements
the Rust core described in [DESIGN.md](DESIGN.md): durable tasks, auditability,
policy-gated actions, local-first model routing, plugin contracts, scheduler
state, and an IPC surface for a future Swift shell.

## Current Scope

This repository is intentionally v1 foundation work, not a Marvel/JARVIS clone
and not an autonomous external-communication system. Risky side effects must be
blocked or require approval, and every meaningful decision should be auditable.

## Build

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

## Docs

- [Architecture map](docs/architecture-map.md)
- [Plugin contract](docs/plugin-contract.md)
- [Safety rules](docs/safety-rules.md)
- [Build and test commands](docs/build-test-commands.md)
- [Release checklist](docs/release-checklist.md)
- [Knowledge-base facts](docs/knowledge-base/jarvis-project-facts.md)
