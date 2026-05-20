# Jarvis

Jarvis is a local-first macOS assistant foundation. The current repo implements
the Rust core described in [DESIGN.md](DESIGN.md): durable task/audit
primitives, policy-gated first-party plugin commands, bounded model-planned
first-party tool orchestration, local-first model routing evidence, plugin
contracts, scheduler state, redacted diagnostics export, a loopback IPC
surface, and CLI smoke paths for the Swift shell scaffold and future packaged
app.
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
plugin installation, and packaged Mac smoke evidence are still target
architecture.

## Production Work Protocol

This public repository is being advanced through isolated worktrees, topic
branches, and reviewable PR slices. During the autonomous production sweep,
agents should stay inside their assigned ownership, preserve unrelated edits,
and treat cross-process E2E plus the local release gate as the evidence bar.
Passing the local gate supports only the implemented Rust/Swift foundation
claim; it is not proof of a finished packaged assistant.

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
current durable state, redacted diagnostics, and first-party plugin manifests
over IPC.

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
