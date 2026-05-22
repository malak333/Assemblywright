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
- Repository-backed `/commands` also persists append-only SQLite model-route
  records. The stored and inspectable route copy keeps provider/outcome/policy
  evidence but omits `context_for_model`, so restart recovery can prove route
  selection without retaining raw command bodies or route context.
- Repository-backed IPC exposes `/activity/summary`, and the CLI exposes
  `jarvis activity summary`, as a pollable progress surface for task status
  counts, active task count, recent tasks, and recent audit entries. It is
  deterministic repository evidence for current activity.
- Repository-backed IPC also exposes `/activity/events`, and the CLI exposes
  `jarvis activity watch`, as bounded server-sent events carrying activity
  summary snapshots. This is local progress-streaming evidence for current
  task/audit state, not per-token model streaming or plugin-internal progress
  events.
- `ConversationRuntime` supports bounded fake-model planned first-party tool
  calls with schema validation, policy checks, approval stops, tool-result audit
  entries, and feedback of tool results into later model steps. The local HTTP
  provider does not yet make real model-planned tool calls; installed-plugin
  orchestration remains target architecture.
- Repository-backed IPC state exposes task, audit, model-route, and memory
  inspection routes, persists scheduler jobs, restores them at startup, and all
  IPC states expose `/plugins/manifests` for deterministic first-party plugin
  manifests.
  Repository-backed IPC also exposes `/plugins/installed` for metadata-only
  local plugin installation. Installed records are persisted with
  `execution_enabled: false` and `execution_grant: metadata_only` by default.
  Installed records also carry a local provenance snapshot with manifest and,
  for `local_subprocess`, command SHA-256 hashes. This proves only local file
  integrity against the install snapshot, not publisher identity or malware
  safety.
  Installed plugin run requests can perform contract-only dry runs that
  validate manifest/action/input schema and audit `side_effect_executed: false`
  without loading or executing plugin code. `local_subprocess` manifests can be
  explicitly enabled through `/plugins/installed/:id/execution` or
  `plugins enable-installed` with `execution_grant: subprocess_stdio` after
  `plugins verify-installed` confirms `matches_install_snapshot`; only then can
  they run through the constrained subprocess-stdio JSON boundary.
- Repository-backed IPC exposes `/permissions/grants`, and the CLI exposes
  `jarvis permissions grants`, as a read-only permission-center summary. It
  combines approval status counts/history, high-risk pending approval count,
  installed-plugin grant state, executable installed-plugin count, provenance
  integrity status, capture method, last verification timestamp, origin claim
  metadata, unverified installed-plugin count, and the
  `side_effects_require_approval` invariant without enabling installed plugin
  execution. The Swift permission center renders those provenance statuses so
  metadata-only, verified, changed, missing, invalid, and legacy-unverified
  plugin grants are visible during review.
- Repository-backed IPC also exposes `/permissions/policy-review`, and the CLI
  exposes `jarvis permissions review`, as a read-only policy review surface. It
  converts pending approvals, high-risk plugin actions, unverified installed
  plugin provenance, and unverified publisher-origin claims into explicit
  severity-ranked review items. The Swift Approval Center renders this summary
  alongside grant history. It is inspection-only and does not execute or enable
  plugin side effects.
- The CLI has matching `tasks`, `memory`, `scheduler`, `diagnostics`, and
  `plugins` subcommands, including `plugins install`, `plugins installed`,
  `plugins installed-get`, `plugins enable-installed`, `plugins
  verify-installed`, `plugins disable-installed`, and `plugins run-installed`
  for disabled-by-default local manifests and explicit subprocess execution.
- A local packaged app release smoke exists, and installed plugin execution now
  has a constrained local subprocess proof. Developer ID signing,
  notarization, installer validation, App Store distribution, real voice loop,
  broader plugin marketplace/WASM/network sandboxing, signed-publisher trust,
  and broader production operations are not yet implemented in this worktree.
  The SwiftUI shell scaffold and IPC client live under `apps/mac`, including a
  command transcript, activity/audit panel, approval decision controls,
  management tabs, permission grant-history summary, degraded-mode handling,
  text-only voice command handoff, and a core supervisor abstraction for
  configured or bundled local core binaries.
- The architecture docs must preserve two diagrams: the current implemented
  Rust/Swift scaffold and the end-goal production architecture. Keep the
  current-vs-target phase table aligned with code before answering readiness
  questions.
- The active architecture docs should also describe the current production
  sweep structure, but that workflow context must remain separate from
  readiness proof.
- The Swift shell is currently a scaffold with a core supervisor abstraction
  and local packaged-app smoke evidence. It is not a Developer ID signed or
  notarized packaged app.
- The Swift Memory tab now uses the Rust IPC memory contract for list,
  include-deleted refresh, create, load, update of mutable fields, review,
  soft-delete, and restore. Category and key remain creation-time fields in
  the current IPC contract; the Swift edit path updates value, provenance, and
  sensitivity. Restore clears `deleted_at` through `/memory/:id/restore` and
  stays subject to the active `(category, key)` uniqueness guard.
- Repository-backed IPC exposes `/memory/classification`, and the CLI exposes
  `jarvis memory classification`, as a read-only memory corpus summary. It
  groups memory by sensitivity and category, reports active/deleted/reviewed
  and unreviewed-active counts, and never returns memory values beyond the
  existing item list/get endpoints. The Swift Memory tab renders this summary
  above the item list.
- The Swift shell has a Keychain-backed launch credential boundary for
  app-supervised model provider secrets. `JarvisCoreCredentialProvider` reads
  known credentials such as the OpenAI API key from Keychain and injects only
  missing process environment values when launching the bundled core; explicit
  environment values still win, and the provider does not auto-enable ChatGPT.
- The Swift shell now exposes production-facing scaffold tabs for approval
  evidence, runs/audit, scheduler create/inspect/cancel, redacted diagnostics,
  and voice state. Voice supports typed transcript staging and hands the
  transcript to the same text command path. The scaffold now models
  interruption, resume/cancel, unavailable, and degraded typed-fallback states,
  owns a protocol-backed macOS Speech/AVFoundation adapter model from the
  SwiftUI Voice tab, and exposes permission request, start/stop capture, and
  interrupt controls. The Voice tab also owns a protocol-backed AVFoundation
  speech-output adapter with preview, stop, and interrupt controls. Swift tests
  cover both adapter boundaries with fakes and do not require live microphone
  access or live audio output. The app still must not claim real voice parity
  until entitlements, clean-profile permission prompts, live microphone capture,
  live audio output, and manual device validation are complete.
- The scheduler is inspectable, cancellable, explicitly runnable through
  `scheduler run-due`, and opt-in runnable as a bounded background loop with
  `jarvis serve --scheduler-background`. Scheduler jobs are in-memory without
  repository backing and durable when the IPC state is started with
  `SqliteRepository`. The background loop uses the same audited run-due path,
  per-tick limit, deterministic due ordering, and fail-closed emergency-pause
  behavior as manual execution. Repository-backed IPC exposes
  `/scheduler/attention`, and the CLI exposes `jarvis scheduler attention`, as
  a redacted app handoff summary for due, running, and failed scheduler jobs.
  The Swift Scheduler tab renders this summary above the job list and now owns
  a protocol-backed notification model plus macOS `UserNotifications` adapter
  controls for due/failed attention items. Swift tests use a fake adapter to
  cover authorization, delivery, duplicate suppression, and denied-permission
  fail-closed behavior. Richer proactive trigger policy and live OS
  notification validation remain target architecture.

## Proof Boundaries

- Local release proof currently means `./scripts/release-local.sh`, which wraps
  `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo
  test --workspace -- --ignored`, `cargo build --workspace`, `cargo run -p
  jarvis-cli -- smoke`, `cargo package --workspace --allow-dirty`, `swift test
  --package-path apps/mac`, and `swift build --package-path apps/mac`.
  It also runs `./scripts/storage-migration-backup-smoke.sh` so file-backed
  migration backup/recovery stays part of the default local release evidence.
- The current E2E expectation for Rust/CLI foundation changes is
  `cargo test -p jarvis-cli --test local_ipc_e2e`; the ignored variant is
  release-proof coverage and is included by `./scripts/release-local.sh`.
- The focused repository-state test for progress visibility is
  `cargo test -p jarvis-core repository_backed_state_endpoints_expose_tasks_and_audit -- --nocapture`.
  Contract coverage for the activity stream is in
  `cargo test -p jarvis-core contract_endpoint_documents_safe_inspection_paths -- --nocapture`,
  and cross-process CLI coverage is in
  `cargo test -p jarvis-cli --test local_ipc_e2e serve_exposes_local_ipc_contract_and_persists_state -- --nocapture`.
  Swift model coverage for the same contract is included in
  `swift test --package-path apps/mac --filter JarvisMacCoreTests`.
- Every feature or phase should identify the relevant E2E or focused
  integration coverage before a readiness claim is made. If behavior changes
  and no coverage exists, add the coverage or record the blocker. Docs-only
  phases should at least preserve the architecture diagrams, release checklist,
  build/test commands, and KB proof-boundary notes.
- Do not describe Jarvis as a finished desktop assistant based on the local
  packaged app smoke alone. Broader readiness still needs Developer ID
  signing/notarization evidence, richer permission UX, real voice where
  claimed, installed-plugin sandboxing/execution where claimed, and manual
  clean-profile release QA.
- Do not describe Jarvis as production assistant ready based only on the Rust
  and Swift local gates. The stronger claim requires packaged-app evidence,
  richer permission UX, real voice where claimed, diagnostics/recovery checks,
  and release smoke proof.
- `./scripts/packaged-supervision-proof.sh` is local packaged-layout evidence:
  it builds the Rust CLI, copies it into
  `Jarvis.app/Contents/Resources/bin/jarvis-cli`, runs Swift supervisor tests
  against that executable, starts the copied binary with repository-backed
  state, and verifies health, command, audit, diagnostics, emergency pause,
  blocked command, pause status, and resume over IPC. It is not signed,
  notarized, clean-profile packaged app release evidence.
- `./scripts/packaged-app-release-smoke.sh` is stronger local packaged app
  evidence: it builds `jarvis-cli` and the Swift app executable, assembles a
  deterministic `Jarvis.app`, writes release-smoke `Info.plist` metadata,
  bundles `jarvis-cli` at `Contents/Resources/bin/jarvis-cli`, ad-hoc signs
  with `codesign -` when available, launches the app executable with a
  temporary HOME/profile and explicit temp database path, and verifies
  app-supervised health, command, audit, diagnostics, emergency pause, blocked
  command, pause status, resume, and SQLite state. It is not Developer ID
  signing, notarization, installer validation, entitlement validation,
  Finder/LaunchServices validation, App Store distribution, or real
  microphone/Speech/live audio-output coverage.
- Swift scheduler notification controls are repo-owned adapter evidence: the
  core model can request authorization, build due/failed notification requests,
  suppress duplicate deliveries for the same attention item, and fail closed
  when permission is denied. This is not a substitute for manual clean-profile
  macOS notification prompt and delivery validation.
- `./scripts/package-distribution.sh` is the repo-owned distribution packaging
  lane. Its `--check` mode is credential-free and validates local tools plus
  entitlements. Full mode requires the owner's Developer ID and notarytool
  credentials, signs with hardened runtime and microphone entitlements,
  notarizes, and staples the app. It still does not replace clean-profile
  Finder launch, live microphone/Speech validation, installer validation, App
  Store review, or live audio-output validation.
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
  currently `codex/phase3-docs-architecture` in
  `/Users/michaelnobile/Antigravity/jarvis-worktrees-phase3/phase3-docs-architecture`.
- Phase-3 work is split across separate worktrees and `codex/` branches:
  `model-route-persistence`, `plugin-subprocess-sandbox`,
  `voice-adapter-production`, `packaged-app-release-smoke`,
  `permission-grants-ux`, and `phase3-docs-architecture`. Treat those names as
  coordination context until each slice is merged and verified on main.
- Follow-on Swift scheduler notification work uses
  `codex/scheduler-notifications` in
  `/Users/michaelnobile/Antigravity/jarvis-worktrees-continuation/scheduler-notifications`.
- Follow-on activity summary work uses `codex/activity-summary` in
  `/Users/michaelnobile/Antigravity/jarvis-worktrees-continuation/activity-summary`.
- Follow-on activity event streaming work uses `codex/activity-events` in
  `/Users/michaelnobile/Antigravity/jarvis-worktrees-continuation/activity-events`.
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
- Durable facts from the May 21, 2026 production sweep: the repo is public at
  `https://github.com/malak333/Jarvis`, work should be split across isolated
  worktrees and `codex/` topic branches, PRs should be reviewable and
  evidence-backed, docs-only workers must not edit Rust or Swift code, and
  readiness language must stay scoped to verified local foundation surfaces
  until distribution-grade packaged app signing/notarization, real voice,
  executable plugin sandbox, recovery, and manual release QA gates exist.
- Durable fact from phase 3 packaged app work: SwiftPM does not create a full
  release `.app` bundle by itself here, so the local smoke assembles the bundle
  deterministically in a temp directory and uses environment-configurable
  supervisor endpoint/database settings to avoid port conflicts and preserve
  clean temp-profile state.
- The user explicitly expects each feature/phase to follow docs and
  documentation, add useful conversation-derived knowledge-base facts, and add
  or confirm end-to-end testing for the discussed scope.
- `jarvis-cli serve --db-path <path>` starts IPC with SQLite-backed task,
  audit, memory, and emergency-pause state for manual persistence checks.
- File-backed `SqliteRepository::open` creates a preflight migration backup
  for existing DBs below the current schema version and restores the original
  DB/WAL/SHM files if opening/configuring/migrating fails. Backups are
  app-owned local files, may include personal memory/audit/plugin metadata, and
  are not redacted diagnostics exports. Keychain secrets are not stored in
  SQLite backups.
- `cargo run -p jarvis-cli -- smoke` now covers baseline command/pause smoke,
  plugin manifest listing, and repository-backed task, model-route, and memory
  inspection paths, diagnostics redaction, and repository-backed scheduler/job
  state surfaces.

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
  expanded into arbitrary local code execution. The current executable boundary
  is limited to `local_subprocess` manifests that declare JSON stdin/stdout,
  use a command canonicalized under `source_path`, are explicitly enabled with
  `execution_grant: subprocess_stdio`, validate input and output schemas, run
  with the declared timeout, and emit audit evidence including whether the
  subprocess started. Any broader executable path needs a stronger sandbox,
  explicit grant state beyond `metadata_only`, policy checks,
  timeout/cancellation behavior, and E2E audit coverage.
