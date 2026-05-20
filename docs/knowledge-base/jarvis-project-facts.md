# Jarvis Project Facts

These notes capture durable facts for future agents working on this repository.

## Repository And Scope

- The repository is public at `https://github.com/malak333/Jarvis`.
- The product direction is a local-first macOS assistant foundation, legally
  distinct from Marvel/JARVIS branding and assets.
- The current repo contains a Rust workspace with `jarvis-core` and
  `jarvis-cli`.
- Implemented `jarvis-core` surfaces include shared task/audit/safety types,
  an Axum loopback IPC server, runtime-backed command execution with
  `FakeLocalModel`, emergency-pause state, in-memory scheduler state, a
  conversation runtime with SQLite task/audit persistence hooks, local-first
  model routing policy, SQLite repository migrations, memory item persistence,
  append-only audit table triggers, plugin manifest validation, and
  deterministic first-party test plugins.
- The planned Swift/SwiftUI Mac shell, packaged app, approval UI, local model
  provider integration, plugin installation flow, and autonomous model-router to
  plugin execution loop are not yet implemented in this worktree.
- The IPC `/commands` endpoint calls `ConversationRuntime` and can persist
  through `SqliteRepository` when repository-backed state is used. It does not
  currently compose `ModelRouter` or `PluginHost` into the command pipeline.

## Proof Boundaries

- Local Rust proof currently means `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo
  test --workspace -- --ignored`, `cargo build --workspace`, `cargo run -p
  jarvis-cli -- smoke`, and `cargo package --workspace`.
- Do not describe Jarvis as a finished desktop assistant until the Swift shell,
  packaged app, approval UI, real model providers, and Mac release smoke test
  exist.
- It is fair to describe the current repo as a Rust foundation with tested
  scaffolding for IPC, storage, policy, routing, runtime, scheduler, plugin
  contracts, and CLI behavior when the local gate passes.
- Do not claim autonomous external communication, smart-home control, or
  third-party plugin marketplace readiness for v1.
- Keep public-facing claims scoped to tested local behavior.

## Workflow

- Work in isolated worktrees and branches for reviewable slices.
- Do not revert or overwrite unrelated work from other agents.
- Keep branch work narrow and commit with clear evidence.
- Push the branch after local verification when requested.
- Treat validation as a merge gate; if a command cannot run, record the blocker
  instead of implying coverage.

## Safety Guardrails

- Local model routing is the default.
- ChatGPT is the only approved cloud model and requires explicit routing,
  sensitivity checks, minimized context, and audit evidence.
- Side effects pass through capability scopes plus risk tiers.
- High-risk or uncertain actions fail closed.
- Emergency pause, cancellation, and auditability are architectural
  requirements.
- Plugins must declare capabilities, scopes, risk tiers, schemas, proactive
  behavior, memory access, model access, audit fields, timeout behavior, and
  cancellation behavior before execution.
