# Jarvis

Jarvis is a local-first macOS assistant foundation. The current repo implements
the Rust core described in [DESIGN.md](DESIGN.md): durable task/audit
primitives, policy-gated first-party plugin commands, bounded model-planned
first-party tool orchestration, local-first model routing evidence, plugin
contracts, metadata-only local plugin installation, scheduler state, redacted
diagnostics export, a loopback IPC surface, and CLI smoke paths for the Swift
shell scaffold and future packaged app.
It also includes the first buildable Swift/SwiftUI Mac shell scaffold under
`apps/mac`, with a tested IPC client, command-console state model,
activity/audit panel for command evidence, management surfaces, degraded-mode
handling, and a core supervisor abstraction.

## Current Scope

This repository is intentionally v1 foundation work, not a Marvel/JARVIS clone
and not an autonomous external-communication system. Risky side effects must be
blocked or require approval, and every meaningful decision should be auditable.
The current implementation should not be described as a finished production
assistant: signed packaging, real model providers, approval UI, voice support,
installed plugin execution, and packaged Mac smoke evidence are still target
architecture. Local plugin installation currently stores validated manifest
metadata only and does not create an execution path.

## Build

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
swift test --package-path apps/mac
swift build --package-path apps/mac
```

For the current IPC smoke path, start the local server and run CLI commands
from a second terminal:

```sh
cargo run -p jarvis-cli -- serve
cargo run -p jarvis-cli -- health
```

Use `cargo run -p jarvis-cli -- serve --db-path /tmp/jarvis.sqlite` when you
want manual IPC commands to persist task and audit state locally.

With a repository-backed server running, `jarvis tasks`, `jarvis memory`,
`jarvis scheduler`, `jarvis diagnostics`, and `jarvis plugins` expose the
current durable state, redacted diagnostics, first-party plugin manifests, and
disabled installed-plugin registry metadata over IPC.

## Docs

- [Architecture map](docs/architecture-map.md)
- [Plugin contract](docs/plugin-contract.md)
- [Safety rules](docs/safety-rules.md)
- [Build and test commands](docs/build-test-commands.md)
- [Release checklist](docs/release-checklist.md)
- [Knowledge-base facts](docs/knowledge-base/jarvis-project-facts.md)

The architecture map includes both the implemented current-state diagram and
the end-goal production diagram, plus a phase table that separates verified
foundation work from future production assistant requirements.
