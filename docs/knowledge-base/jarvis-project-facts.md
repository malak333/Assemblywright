# Jarvis Project Facts

These notes capture durable facts for future agents working on this repository.

## Repository And Scope

- The repository is public at `https://github.com/malak333/Jarvis`.
- The product direction is a local-first macOS assistant foundation, legally
  distinct from Marvel/JARVIS branding and assets.
- The current repo contains a Rust workspace with `jarvis-core` and
  `jarvis-cli`, plus a Swift package scaffold under `apps/mac`.
- Implemented `jarvis-core` surfaces include shared task/audit/safety types,
  an Axum loopback IPC server, runtime-backed command execution with
  `FakeLocalModel`, emergency-pause state, inspectable scheduler state, a
  conversation runtime with SQLite task/audit persistence hooks, local-first
  model routing policy, SQLite repository migrations, memory item persistence,
  append-only audit table triggers, plugin manifest validation, and
  deterministic first-party test plugins.
- IPC `/commands` now uses repository-backed runtime storage when `IpcState` is
  constructed with `SqliteRepository`, records a local-first model-router audit
  entry, and can execute deterministic first-party plugin commands such as
  `plugin echo ...` and `status` through policy. `dry_run` skips plugin
  execution and records audit evidence.
- `ConversationRuntime` supports bounded fake-model planned first-party tool
  calls with schema validation, policy checks, approval stops, tool-result audit
  entries, and feedback of tool results into later model steps. This is not yet
  real-provider or installed-plugin orchestration.
- Repository-backed IPC state exposes task, audit, and memory inspection routes,
  persists scheduler jobs, restores them at startup, and all IPC states expose
  `/plugins/manifests` for deterministic first-party plugin manifests. The CLI
  has matching `tasks`, `memory`, `scheduler`, `diagnostics`, and `plugins`
  subcommands.
- The planned signed packaged app, approval UI, local model provider
  integration, plugin installation flow, and production real-provider tool
  orchestration are not yet implemented in this worktree. The first SwiftUI
  shell scaffold and IPC client live under `apps/mac`, including a command
  transcript, activity/audit panel, management tabs, degraded-mode handling, and
  a core supervisor abstraction for configured or bundled local core binaries.
- The architecture docs must preserve two diagrams: the current implemented
  Rust/Swift scaffold and the end-goal production architecture. Keep the
  current-vs-target phase table aligned with code before answering readiness
  questions.
- The Swift shell is currently a scaffold with a core supervisor abstraction,
  not a signed packaged app with bundled-core smoke evidence.
- The Swift shell now exposes production-facing scaffold tabs for approval
  evidence, runs/audit, scheduler create/inspect/cancel, redacted diagnostics,
  and voice degraded-mode state. Approval decisions remain inspection-only until
  the Rust IPC contract exposes approval mutation endpoints, and voice remains a
  text-only scaffold rather than real speech recognition.
- The scheduler is currently inspectable and cancellable. Scheduler jobs are
  in-memory without repository backing and durable when the IPC state is started
  with `SqliteRepository`. Proactive production trigger execution remains target
  architecture.

## Proof Boundaries

- Local release proof currently means `./scripts/release-local.sh`, which wraps
  `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo
  test --workspace -- --ignored`, `cargo build --workspace`, `cargo run -p
  jarvis-cli -- smoke`, `cargo package --workspace --allow-dirty`, `swift test
  --package-path apps/mac`, and `swift build --package-path apps/mac`.
- Do not describe Jarvis as a finished desktop assistant until the Swift shell,
  packaged app, approval UI, real model providers, and Mac release smoke test
  exist.
- Do not describe Jarvis as production assistant ready based only on the Rust
  and Swift local gates. The stronger claim requires packaged-app evidence,
  real provider integration, approval UI, voice or text UX parity as scoped,
  diagnostics/recovery checks, and release smoke proof.
- It is fair to describe the current repo as a Rust foundation with tested
  scaffolding for IPC, storage, policy, routing, runtime, scheduler, plugin
  contracts, deterministic first-party plugin command execution, bounded
  fake-model planned first-party tool orchestration, CLI behavior, and a Swift
  command/management shell with supervisor abstraction when the local gate
  passes.
- Do not claim autonomous external communication, smart-home control, or
  third-party plugin marketplace readiness for v1.
- Keep public-facing claims scoped to tested local behavior.

## Workflow

- Work in isolated worktrees and branches for reviewable slices.
- When multiple agents are active, stay inside assigned ownership. For docs-only
  architecture work, use `apply_patch` and do not touch implementation files.
- Do not revert or overwrite unrelated work from other agents.
- Keep branch work narrow and commit with clear evidence.
- Push the branch after local verification when requested.
- Treat validation as a merge gate; if a command cannot run, record the blocker
  instead of implying coverage.
- `jarvis-cli serve --db-path <path>` starts IPC with SQLite-backed task,
  audit, memory, and emergency-pause state for manual persistence checks.
- `cargo run -p jarvis-cli -- smoke` now covers baseline command/pause smoke,
  plugin manifest listing, and repository-backed task plus memory inspection
  paths, diagnostics redaction, and repository-backed scheduler/job state
  surfaces.

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
