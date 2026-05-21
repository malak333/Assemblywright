# Jarvis Project Facts

These notes capture durable facts for future agents working on this repository.

## Repository And Scope

- The repository is public at `https://github.com/malak333/Jarvis`.
- Production implementation work should assume public-repo hygiene: no secrets,
  no private-source material, no hidden readiness claims, and release evidence
  that can be reviewed from the branch/PR.
- The product direction is a local-first macOS assistant foundation, legally
  distinct from Marvel/JARVIS branding and assets.
- The current repo contains a Rust workspace with `jarvis-core` and
  `jarvis-cli`, plus a Swift package scaffold under `apps/mac`.
- Implemented `jarvis-core` surfaces include shared task/audit/safety types,
  an Axum loopback IPC server, runtime-backed command execution with
  `FakeLocalModel` by default, an opt-in Ollama-compatible local HTTP provider,
  or an opt-in ChatGPT/OpenAI-compatible HTTP provider behind explicit
  env/config, sensitivity, redaction, and audit guardrails, emergency-pause
  state, inspectable scheduler state, a conversation runtime with SQLite
  task/audit persistence hooks, local-first model routing policy, SQLite
  repository migrations, memory item persistence, append-only audit table
  triggers, plugin manifest validation, and deterministic first-party test
  plugins.
- IPC `/commands` now uses repository-backed runtime storage when `IpcState` is
  constructed with `SqliteRepository`, records a local-first model-router audit
  entry, and can execute deterministic first-party plugin commands such as
  `plugin echo ...` and `status` through policy. `dry_run` skips plugin
  execution and records audit evidence.
- `ConversationRuntime` supports bounded fake-model planned first-party tool
  calls with schema validation, policy checks, approval stops, tool-result audit
  entries, and feedback of tool results into later model steps. The local HTTP
  provider does not yet make real model-planned tool calls; installed-plugin
  orchestration remains target architecture.
- Repository-backed IPC state exposes task, audit, and memory inspection routes,
  persists scheduler jobs, restores them at startup, and all IPC states expose
  `/plugins/manifests` for deterministic first-party plugin manifests.
  Repository-backed IPC also exposes `/plugins/installed` for metadata-only
  local plugin installation. Installed records are persisted with
  `execution_enabled: false` and `execution_grant: metadata_only`; they are not
  executable. Installed plugin run requests can perform contract-only dry runs
  that validate manifest/action/input schema and audit
  `side_effect_executed: false` without loading or executing plugin code.
- The CLI has matching `tasks`, `memory`, `scheduler`, `diagnostics`, and
  `plugins` subcommands, including `plugins install`, `plugins installed`, and
  `plugins installed-get` for disabled local manifest metadata.
- The planned signed packaged app, installed plugin execution, real voice loop,
  and broader production operations are not yet implemented in this worktree.
  The SwiftUI shell scaffold and IPC client live under `apps/mac`, including a
  command transcript, activity/audit panel, approval decision controls,
  management tabs, degraded-mode handling, text-only voice command handoff, and
  a core supervisor abstraction for configured or bundled local core binaries.
- The architecture docs must preserve two diagrams: the current implemented
  Rust/Swift scaffold and the end-goal production architecture. Keep the
  current-vs-target phase table aligned with code before answering readiness
  questions.
- The Swift shell is currently a scaffold with a core supervisor abstraction,
  not a signed packaged app with bundled-core smoke evidence.
- The Swift shell now exposes production-facing scaffold tabs for approval
  evidence, runs/audit, scheduler create/inspect/cancel, redacted diagnostics,
  and voice state. Voice supports typed transcript staging and hands the
  transcript to the same text command path, but remains a text-only scaffold
  rather than real microphone capture or speech recognition.
- The scheduler is inspectable, cancellable, explicitly runnable through
  `scheduler run-due`, and opt-in runnable as a bounded background loop with
  `jarvis serve --scheduler-background`. Scheduler jobs are in-memory without
  repository backing and durable when the IPC state is started with
  `SqliteRepository`. The background loop uses the same audited run-due path,
  per-tick limit, deterministic due ordering, and fail-closed emergency-pause
  behavior as manual execution. Richer proactive trigger policy and app
  notification handoff remain target architecture.

## Proof Boundaries

- Local release proof currently means `./scripts/release-local.sh`, which wraps
  `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo
  test --workspace -- --ignored`, `cargo build --workspace`, `cargo run -p
  jarvis-cli -- smoke`, `cargo package --workspace --allow-dirty`, `swift test
  --package-path apps/mac`, and `swift build --package-path apps/mac`.
- The current E2E expectation for Rust/CLI foundation changes is
  `cargo test -p jarvis-cli --test local_ipc_e2e`; the ignored variant is
  release-proof coverage and is included by `./scripts/release-local.sh`.
- Do not describe Jarvis as a finished desktop assistant until the Swift shell,
  packaged app, richer permission UX, and Mac release smoke test exist.
- Do not describe Jarvis as production assistant ready based only on the Rust
  and Swift local gates. The stronger claim requires packaged-app evidence,
  richer permission UX, real voice where claimed, diagnostics/recovery checks,
  and release smoke proof.
- It is fair to describe the current repo as a Rust foundation with tested
  scaffolding for IPC, storage, policy, routing, runtime, scheduler, plugin
  contracts, deterministic first-party plugin command execution, bounded
  fake-model planned first-party tool orchestration, opt-in Ollama-compatible
  local HTTP provider behavior, opt-in ChatGPT/OpenAI-compatible provider
  behavior, CLI behavior, and a Swift command/management shell with supervisor
  abstraction, approval decisions, and text-only voice handoff when the local
  gate passes.
- Do not claim autonomous external communication, smart-home control, or
  third-party plugin marketplace readiness for v1.
- Keep public-facing claims scoped to tested local behavior.

## Workflow

- Work in isolated worktrees and branches for reviewable slices.
- Use topic branches and PRs for production work. The docs production slice is
  `codex/production-docs` in
  `/Users/michaelnobile/Antigravity/jarvis-worktrees/production-docs`.
- When multiple agents are active, stay inside assigned ownership. For docs-only
  architecture work, use `apply_patch` and do not touch implementation files.
- Do not revert or overwrite unrelated work from other agents.
- Keep branch work narrow and commit with clear evidence.
- Push the branch after local verification when requested.
- Treat validation as a merge gate; if a command cannot run, record the blocker
  instead of implying coverage.
- A six-agent autonomous sweep, sometimes referred to as the 6-agent sweep, is
  a coordination model for parallel ownership slices. It is not itself
  readiness evidence; only checked-in code/docs, reviewed PRs, and verification
  output count as proof.
- `jarvis-cli serve --db-path <path>` starts IPC with SQLite-backed task,
  audit, memory, and emergency-pause state for manual persistence checks.
- `cargo run -p jarvis-cli -- smoke` now covers baseline command/pause smoke,
  plugin manifest listing, and repository-backed task plus memory inspection
  paths, diagnostics redaction, and repository-backed scheduler/job state
  surfaces.

## Safety Guardrails

- Local model routing is the default.
- ChatGPT is the only approved cloud model and requires explicit env opt-in,
  explicit routing, sensitivity checks, minimized redacted context, and audit
  evidence.
- Side effects pass through capability scopes plus risk tiers.
- High-risk or uncertain actions fail closed.
- Emergency pause, cancellation, and auditability are architectural
  requirements.
- Plugins must declare capabilities, scopes, risk tiers, schemas, proactive
  behavior, memory access, model access, audit fields, timeout behavior, and
  cancellation behavior before execution.
- Installed plugin execution remains disabled by default and must not be
  expanded into arbitrary local code execution. The current safe boundary is
  metadata persistence plus contract-only dry runs; any future executable path
  needs a sandboxed runner, explicit grant state beyond `metadata_only`, policy
  checks, timeout/cancellation behavior, and E2E audit coverage.
