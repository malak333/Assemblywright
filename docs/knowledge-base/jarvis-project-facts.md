# Jarvis Project Facts

These notes capture durable facts for future agents working on this repository.

## Repository And Scope

- The repository is public at `https://github.com/malak333/Jarvis`.
- The product direction is a local-first macOS assistant foundation, legally
  distinct from Marvel/JARVIS branding and assets.
- The current repo contains a Rust workspace foundation with `jarvis-core` and
  `jarvis-cli`.
- The planned Swift/SwiftUI Mac shell, IPC runtime, persistence, model routing,
  plugin host, memory store, scheduler, and packaged app smoke test are not yet
  implemented in this worktree.

## Proof Boundaries

- Local Rust proof currently means `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo
  build --workspace`, and `cargo run -p jarvis-cli -- health`.
- Do not describe Jarvis as a finished desktop assistant until the Swift shell,
  app/core IPC, persistence, safety gates, and release smoke test exist.
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
