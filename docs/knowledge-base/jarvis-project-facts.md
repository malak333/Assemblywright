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
- Selected model-provider failures are returned as structured failed command
  responses. `ConversationRuntime` marks the task failed, appends
  `model_step_failed` with redacted diagnostics, preserves selected route
  evidence, and lets IPC return `accepted: false` instead of a transport-level
  command error.
- Repository-backed IPC exposes `/activity/summary`, and the CLI exposes
  `jarvis activity summary`, as a pollable progress surface for task status
  counts, active task count, recent tasks, and recent audit entries. It is
  deterministic repository evidence for current activity.
- Repository-backed IPC also exposes `/activity/events`, and the CLI exposes
  `jarvis activity watch`, as bounded server-sent events carrying activity
  summary snapshots. This is local progress-streaming evidence for current
  task/audit state, not per-token model streaming.
- Installed `local_subprocess` plugins can emit bounded newline-delimited
  stderr JSON frames with `jarvis_progress: true`, `stage`, and `message`.
  Jarvis records parsed sequence/stage/message events in the run response and
  append-only audit entries while redacting raw stderr. This is post-run
  audit-backed plugin progress evidence, not real-time plugin UI streaming.
- `/contract` includes a `compatibility` block with supported version range,
  additive-change, deprecation, removed/deprecated endpoint, and client
  requirement policy, plus a `features` list with stable keys, status, proof,
  and boundary fields so Swift and release docs can distinguish implemented
  repo-owned surfaces from manual or target production claims without scraping
  prose.
- `ConversationRuntime` supports bounded fake-model and provider-envelope
  planned first-party tool calls with schema validation, policy checks, approval
  stops, tool-result audit entries, and feedback of tool results into later
  model steps. Ollama-compatible and ChatGPT/OpenAI-compatible text responses
  can return a strict JSON envelope with `message`, `complete`, and
  `tool_requests`; ChatGPT/OpenAI-compatible responses can also return native
  OpenAI `tool_calls` for advertised first-party tool definitions. Plain text
  remains backward-compatible. This is not installed-plugin orchestration or
  broad third-party tool execution.
- Provider-envelope coverage includes
  `ollama_http_provider_parses_tool_request_envelope`,
  `chatgpt_http_provider_parses_tool_request_envelope`,
  `provider_tool_request_envelope_rejects_malformed_tool_requests_without_leaking_prompt`,
  `provider_originated_tool_request_executes_first_party_tool_and_feeds_result`,
  and the cross-process `serve_executes_ollama_provider_tool_request_envelope`
  E2E with an Ollama-compatible stub.
- Native ChatGPT/OpenAI-compatible tool-call coverage includes
  `chatgpt_http_provider_parses_native_tool_calls` and the cross-process
  `serve_executes_chatgpt_native_tool_call` E2E with an OpenAI-compatible stub.
- Repository-backed IPC state exposes task, audit, model-route, and memory
  inspection routes, persists scheduler jobs, restores them at startup, and all
  IPC states expose `/plugins/manifests` for deterministic first-party plugin
  manifests.
  Repository-backed IPC also exposes `/plugins/installed` for metadata-only
  local plugin installation. Installed records are persisted with
  `execution_enabled: false` and `execution_grant: metadata_only` by default.
  Installed records also carry a local provenance snapshot with manifest and,
  for `local_subprocess`, command SHA-256 hashes. This proves only local file
  integrity against the install snapshot, not malware safety or cryptographic
  publisher identity.
  Installed plugin run requests can perform contract-only dry runs that
  validate manifest/action/input schema and audit `side_effect_executed: false`
  without loading or executing plugin code. `local_subprocess` manifests can be
  explicitly enabled through `/plugins/installed/:id/execution` or
  `plugins enable-installed` with `execution_grant: subprocess_stdio`, or
  `subprocess_stdio_network` for network-declaring actions, after
  `plugins verify-installed` confirms `matches_install_snapshot`; only then can
  they run through the constrained subprocess-stdio JSON boundary.
- Installed plugin publisher-origin claims can be operator-pinned through
  `/plugins/installed/:id/publisher/verify` or `plugins verify-publisher`.
  Verification requires the stored provenance to already match the install
  snapshot and `trusted_origin` to exactly match the manifest author claim, then
  sets `origin_claim_verified: true` and appends an
  `installed_plugin_publisher_verified` audit entry. This is a local review
  control, not cryptographic signature validation, marketplace trust, or malware
  analysis.
- Installed plugin manifests can also include `publisher_signature` with
  `scheme: ed25519-v1`, a base64 Ed25519 public key, and a base64 signature
  over the unsigned manifest payload. `/plugins/installed/:id/publisher/signature/verify`
  and `plugins verify-publisher-signature` require local provenance to match
  first, require an explicit `trusted_public_key` that matches the manifest
  public key, verify the signature, set `origin_claim_verified: true`, and
  append `installed_plugin_publisher_signature_verified` with a hashed trusted
  key reference. This proves the manifest was signed by the trusted key; it
  still does not prove marketplace approval, malware safety, or runtime sandbox
  completeness.
- Plugin actions that request the existing `network` permission must now
  declare `network_access.mode: declared_hosts` and exact plain-hostname
  `allowed_hosts`. Invalid host declarations fail manifest validation, and
  `/permissions/policy-review` emits `network_plugin_action` items for installed
  plugins with declared network access. Executable installed plugins with
  network-declaring actions fail closed unless enabled with
  `subprocess_stdio_network`. This is runtime grant gating plus manifest
  governance and review evidence, not OS-level network sandboxing or host-level
  egress filtering.
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
  plugin provenance, unverified publisher-origin claims, network-capable plugin
  actions, active scheduler triggers, and unreviewed memory items into explicit
  severity-ranked review items. Memory review items include category/key and
  sensitivity only; memory values are redacted from policy review. The Swift
  Approval Center renders this summary alongside grant history. It is
  inspection-only and does not execute, enable plugin side effects, or
  autonomously rewrite/delete memory.
- The CLI has matching `tasks`, `memory`, `scheduler`, `diagnostics`, and
  `plugins` subcommands, including `plugins install`, `plugins installed`,
  `plugins installed-get`, `plugins enable-installed`, `plugins
  verify-installed`, `plugins verify-publisher`,
  `plugins verify-publisher-signature`, `plugins disable-installed`, and
  `plugins run-installed` for disabled-by-default local manifests, auditable
  publisher-origin review, trusted-key signature verification, and explicit
  subprocess execution.
- A local packaged app release smoke exists, and installed plugin execution now
  has a constrained local subprocess proof. Developer ID signing,
  notarization, installer validation, App Store distribution, real voice loop,
  broader plugin marketplace/WASM/OS-network sandboxing, plugin malware
  analysis, and broader production operations are not yet implemented in this
  worktree.
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
- Diagnostics export now includes aggregate active, unreviewed, and sensitive
  memory counts when repository backing is enabled. It still omits memory
  values, and memory policy review similarly redacts values while surfacing
  unreviewed memory for user review.
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
  Repository-backed `/permissions/policy-review` also surfaces manual,
  one-time, and recurring scheduler triggers as redacted review items, with
  due and recurring jobs raised above future one-time/manual jobs and scheduler
  command text omitted from the payload.
  Due-job execution appends a redacted `scheduler_proactive_policy_checked`
  audit entry before command submission. The audit uses the same trigger
  classification as `/permissions/policy-review`, marks `command_redacted:
  true`, and keeps scheduler command text out of the policy audit payload.
  `jarvis scheduler recover-stale` and `/scheduler/recover-stale` provide
  explicit operator recovery for persisted stale `Running` jobs after a crash
  or killed process. Recovery marks matching jobs failed, returns diagnostic
  scheduler job fields without commands, and records
  `scheduler_stale_running_recovered` with command redaction evidence. This is
  explicit recovery unless `jarvis serve --scheduler-recover-stale-on-startup`
  is provided. Startup recovery runs the same stale recovery path before the
  server accepts IPC traffic, marks the audit payload with `automatic_recovery:
  true`, and remains bounded by age/limit flags.
  The Swift Scheduler tab renders this summary above the job list and now owns
  a protocol-backed notification model plus macOS `UserNotifications` adapter
  controls for due/failed attention items. Swift tests use a fake adapter to
  cover authorization, delivery, duplicate suppression, and denied-permission
  fail-closed behavior. Broader production trigger policy and live OS
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
  The E2E covers scheduler proactive policy audit evidence during `scheduler
  run-due` by asserting both one-time and recurring due jobs emit redacted
  `scheduler_proactive_policy_checked` audit entries.
  It also covers stale-running scheduler recovery by persisting a running job
  across restart, running `scheduler recover-stale`, and asserting redacted
  recovery output plus `scheduler_stale_running_recovered` audit evidence.
  Startup stale recovery is covered by
  `serve_can_recover_stale_scheduler_jobs_on_startup`, which starts `jarvis
  serve` with the opt-in recovery flags and asserts the recovered job, redacted
  audit entry, and `automatic_recovery: true` marker.
- Focused provider-failure recovery coverage is
  `cargo test -p jarvis-core model_provider_failure_returns_failed_response_with_route_evidence -- --nocapture`
  plus
  `cargo test -p jarvis-core command_schema_returns_failed_runtime_response_for_model_provider_error -- --nocapture`.
- Focused memory policy review coverage is
  `cargo test -p jarvis-core permission_policy_review_summarizes_unreviewed_memory_without_values -- --nocapture`
  plus
  `cargo test -p jarvis-core diagnostics_export_is_redacted_and_counts_repository_state -- --nocapture`.
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
  with `codesign -` and `packaging/Jarvis.entitlements` when available,
  verifies microphone/Speech usage strings and the packaged app audio-input
  entitlement, launches the app executable with a temporary HOME/profile and
  explicit temp database path, and verifies app-supervised health, command,
  audit, diagnostics, emergency pause, blocked command, pause status, resume,
  and SQLite state. It is not Developer ID signing, notarization, installer
  validation, Finder/LaunchServices validation, App Store distribution, or real
  microphone/Speech/live audio-output coverage.
- Swift scheduler notification controls are repo-owned adapter evidence: the
  core model can request authorization, build due/failed notification requests,
  suppress duplicate deliveries for the same attention item, and fail closed
  when permission is denied. This is not a substitute for manual clean-profile
  macOS notification prompt and delivery validation.
- `./scripts/package-distribution.sh` is the repo-owned distribution packaging
  lane. Its `--check` mode is credential-free and validates local tools plus
  entitlements. Its `--unsigned-structure-check` mode builds release Rust/Swift
  artifacts, assembles `target/distribution/Jarvis.app`, optionally ad-hoc signs
  when `codesign` is available, creates an unsigned `/Applications` installer
  package, and inspects the package payload for the app executable, bundled
  core, and `Info.plist`. Full mode requires the owner's Developer ID
  Application, Developer ID Installer, and notarytool credentials; signs with
  hardened runtime and microphone entitlements; notarizes and staples the app
  zip; then creates, signs, notarizes, and staples a `/Applications` installer
  package. The unsigned structure check still does not prove Developer ID
  signing, notarization, stapling, installation, Finder launch, live
  microphone/Speech validation, App Store review, live audio-output validation,
  or manual QA.
- It is fair to describe the current repo as a Rust foundation with tested
  scaffolding for IPC, storage, policy, routing, runtime, scheduler, plugin
  contracts, deterministic first-party plugin command execution, bounded
  fake-model and strict provider-envelope planned first-party tool orchestration,
  opt-in Ollama-compatible local HTTP provider behavior, opt-in
  ChatGPT/OpenAI-compatible provider behavior, CLI behavior, and a Swift
  command/management shell with supervisor
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
  `execution_grant: subprocess_stdio` for non-network actions or
  `subprocess_stdio_network` for network-declaring actions, validate input and
  output schemas, run with the declared timeout, and emit audit evidence
  including whether the subprocess started. Subprocess stderr may contain
  bounded progress frames, but raw stderr remains redacted from response and
  audit payloads. Any broader executable path or real-time plugin progress
  stream needs a stronger sandbox,
  explicit grant state beyond `metadata_only`, policy checks,
  timeout/cancellation behavior, and E2E audit coverage.
