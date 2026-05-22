# Release Checklist

Use this checklist before tagging or publishing any Jarvis release. Keep the
evidence local-first unless the user explicitly approves hosted infrastructure.

## Scope Check

- Confirm the release target is this public repository
  (`https://github.com/malak333/Jarvis`) and that the work is landing through a
  reviewable worktree/branch/PR slice.
- Confirm `DESIGN.md` still matches the implementation scope.
- Confirm release notes distinguish implemented Rust foundation and Swift shell
  scaffold behavior from the implemented opt-in Ollama-compatible local
  provider boundary, implemented opt-in ChatGPT/OpenAI-compatible provider
  boundary, metadata-only local plugin installation, explicit installed-plugin
  subprocess execution grant, implemented Swift approval decision surface,
  adapter-backed Swift voice input/output controls, local packaged smoke, and
  distribution packaging lane. Keep real microphone, live audio output, and
  distribution readiness scoped to the manual gates below.
- Confirm the current architecture map still matches the real module wiring,
  especially the fact that `/commands` invokes the configured routed
  `ModelExecutor` (`FakeLocalModel` by default, Ollama-compatible HTTP or
  ChatGPT/OpenAI-compatible HTTP only when explicitly enabled), records
  route/policy/plugin audit evidence for deterministic first-party plugin
  commands, and supports bounded fake-model planned first-party tool execution
  before any broader assistant claim.
- Confirm the current-vs-target implementation phase table is up to date before
  using any production-readiness language. Release notes may claim foundation
  readiness only for verified Rust/Swift surfaces, not full assistant readiness.
- Confirm no Marvel branding, copyrighted visuals, or confusing product claims
  were introduced.
- Confirm any autonomous sweep summary names the active ownership slices and
  states which evidence came from commands, tests, or manual checks. A
  six-agent sweep is coordination context, not proof of readiness.
- For phase 3, confirm the active worktree/branch lanes are named separately
  from merged implementation evidence: `model-route-persistence`,
  `plugin-subprocess-sandbox`, `voice-adapter-production`,
  `packaged-app-release-smoke`, `permission-grants-ux`, and
  `phase3-docs-architecture`.
- For each feature/phase, confirm the relevant docs were updated, durable
  knowledge-base facts were added, and matching E2E or focused integration
  coverage exists. If coverage does not exist, add it for behavior changes or
  record the blocker before using broader readiness language.

## Code Gate

- `./scripts/release-local.sh`

The script runs the full local gate below, including the opt-in ignored
release-proof E2E test. Run individual commands only when diagnosing a failing
stage or when a PR needs focused evidence for one ownership slice.

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `./scripts/storage-migration-backup-smoke.sh`
- `cargo test --workspace -- --ignored`
- `cargo build --workspace`
- `cargo run -p jarvis-cli -- smoke`
- `cargo package --workspace --allow-dirty`
- Focused supervision proof for branches that touch Swift core launch or bundle
  discovery: `./scripts/packaged-supervision-proof.sh`
- Focused packaged app release smoke for branches that touch packaging,
  app-supervised core launch, or Mac release evidence:
  `./scripts/packaged-app-release-smoke.sh`
- Distribution packaging preflight for branches that touch release packaging,
  signing, entitlements, or notarization:
  `./scripts/package-distribution.sh --check`
- `swift test --package-path apps/mac`
- `swift build --package-path apps/mac`
- Optional manual CLI/IPC smoke against a running local server:
  - Terminal 1: `cargo run -p jarvis-cli -- serve`
  - Terminal 2: `cargo run -p jarvis-cli -- health`
  - Terminal 2: `cargo run -p jarvis-cli -- command --dry-run "status check"`
  - Terminal 2: `cargo run -p jarvis-cli -- scheduler list`
  - Terminal 2: `cargo run -p jarvis-cli -- pause --reason "release smoke"`
  - Terminal 2: `cargo run -p jarvis-cli -- pause-status`
  - Terminal 2: `cargo run -p jarvis-cli -- resume`

## Safety Gate

- Confirm high-risk actions require approval or are blocked.
- Confirm cloud routing is local-first and ChatGPT-only when cloud use is
  approved.
- Confirm restricted data cannot route to cloud, and credential-adjacent or
  private cloud routes require explicit approval.
- Confirm local and ChatGPT provider errors and route evidence do not include
  raw command bodies, API keys, or unredacted endpoint credentials.
- Confirm emergency pause blocks IPC runtime command execution and cancels
  active scheduler jobs.
- Confirm scheduler due-job execution fails closed by activating emergency
  pause, cancelling remaining open scheduler jobs, and recording scheduler
  audit evidence when a due command is not accepted.
- Confirm runtime emergency pause and cancellation tests still cover active
  command cancellation.
- Confirm plugin manifests validate declared permissions, schemas, proactive
  behavior, memory/model access, timeout behavior, and cancellation behavior.
- Confirm local plugin installation accepts only validated manifest metadata
  with safe absolute source paths and stores installed records with
  `execution_enabled: false`, `execution_grant: metadata_only`, and local
  provenance snapshot metadata.
- Confirm installed plugin metadata remains disabled by default and becomes
  executable only after local provenance verification reports
  `matches_install_snapshot` and an explicit `subprocess_stdio` execution
  grant is set.
- Confirm publisher-origin verification fails closed until local provenance
  matches the install snapshot, requires `trusted_origin` to exactly match the
  installed manifest author claim, persists `origin_claim_verified: true`, and
  appends `installed_plugin_publisher_verified` audit evidence. Do not describe
  this as cryptographic signed-publisher trust.
- Confirm installed plugin run attempts fail closed with manifest/version and
  action validation, default `execution_enabled: false` semantics, local
  provenance verification, safe command path checks, JSON stdin/stdout, timeout
  enforcement, output schema validation, durable audit evidence, and
  `side_effect_executed: false` when no side effect is allowed.
- Confirm persistent audit entries remain append-only in SQLite tests.
- Confirm route, policy, approval, action, and failure evidence stay covered
  before claiming an end-to-end assistant release. The current command path
  persists runtime, route, and deterministic first-party plugin audit evidence
  when repository backing is used. It also persists append-only model-route
  records in SQLite and exposes redacted `/model-routes` CLI/IPC inspection
  that survives restart without retaining route context. Approval-required first-party command
  scaffolds persist inspectable pending approvals and record CLI/IPC grant or
  denial decisions without executing side effects. Bounded fake-model
  first-party tool calls and Ollama-compatible plus ChatGPT/OpenAI-compatible
  provider request/error behavior are covered in focused tests; Swift approval
  decision controls are covered by contract/model tests.
- Confirm task, audit, model-route, memory, and plugin manifest inspection
  endpoints still require or use the correct repository/plugin backing and are
  covered by local smoke or focused IPC tests.
- Confirm approval inspection and grant/deny endpoints require repository
  backing, preserve fail-closed execution behavior, and stay covered by local
  IPC tests.
- Confirm `/permissions/grants` and `jarvis permissions grants` expose
  read-only approval history/counts plus installed-plugin grant state,
  provenance integrity status, unverified plugin counts, and the
  `side_effects_require_approval` invariant. This inspection surface must not
  enable installed plugin code execution.
- Confirm `/permissions/policy-review` and `jarvis permissions review` expose
  read-only severity-ranked review items for pending approvals, high-risk
  plugin actions, unverified provenance, and unverified origin claims without
  enabling side effects, and that operator-pinned publisher verification clears
  the unverified-origin review item for that plugin.
- Confirm the Swift Approval Center renders permission policy review status
  alongside grant history when the IPC contract exposes the endpoint.
- Confirm scheduler job create/list/cancel and due-run execution state is
  restored and updated when repository backing is enabled. Due-run coverage
  proves explicit CLI/IPC runner behavior, including interval reschedule and
  fail-closed pause behavior, not background production trigger scheduling.
- Confirm diagnostics export remains redacted and does not include command
  bodies, scheduler commands, model route contexts, audit payloads, memory
  values, raw cancellation reasons, or credentials.
- Confirm the Swift Memory tab still uses the Rust IPC memory contract for
  create, load, update of mutable fields, review, soft-delete, include-deleted
  refresh, restore, classification summary, and filtering, with deterministic
  Swift package coverage.
- Confirm the Swift Scheduler tab still consumes `/scheduler/attention` and
  renders redacted due/running/failed attention state without exposing
  scheduler command bodies.
- Confirm the cross-process CLI E2E still covers command, plugin, audit,
  redacted model-route inspection and restart recovery, memory
  classification summary, create/update/review/delete/restore, scheduler
  schedule/get/list/cancel, redacted scheduler attention handoff, scheduler
  run-due success/reschedule, scheduler fail-closed pause on non-accepted due
  jobs, diagnostics redaction, persistence restart, and emergency-pause
  blocking/resume behavior. Treat this as the minimum E2E expectation for the
  current Rust/CLI foundation; packaged Mac release smoke is now covered by
  `./scripts/packaged-app-release-smoke.sh` for the local assembled app
  boundary.
- Confirm `./scripts/storage-migration-backup-smoke.sh` passes for storage
  changes, proving legacy DB backup creation, restore after migration-open
  failure, and newer-schema diagnostics. Treat broad installer upgrade and
  full historical fixture matrices as separate release-candidate gates.
- Confirm local plugin metadata install/list/get coverage remains in that E2E
  path, and installed plugin execution coverage applies only after an explicit
  `subprocess_stdio` grant.
- For each new executable feature phase, confirm E2E coverage is either part of
  `local_ipc_e2e`, Swift package tests, a focused integration proof, or the
  future packaged Mac smoke lane. Docs-only changes should still name the
  existing proof boundary they preserve.
- Confirm the Swift shell remains described as a scaffold until a Developer ID
  signed and notarized app exists. `./scripts/packaged-supervision-proof.sh`
  builds the Rust CLI, copies it into a temporary
  `Jarvis.app/Contents/Resources/bin/jarvis-cli` layout, points Swift
  supervisor tests at that executable, and starts the copied binary with a
  repository-backed database to verify health, command, audit, diagnostics,
  emergency pause, blocked command, pause status, and resume surfaces.
  `./scripts/packaged-app-release-smoke.sh` goes further by assembling a
  deterministic SwiftPM-built `Jarvis.app`, writing release-smoke `Info.plist`
  metadata, bundling `jarvis-cli`, ad-hoc signing with `codesign -` when
  available, launching the app executable under a temporary HOME/profile, and
  verifying app-supervised core health, command, audit, diagnostics, emergency
  pause, blocked command, pause status, resume, and clean-profile SQLite state.
  This is local packaged app evidence only; it is not Developer ID signing,
  notarization, installer validation, entitlement validation, or App Store
  release evidence.

## Documentation Gate

- Architecture map is current.
- Both architecture diagrams render: the current implementation diagram and the
  end-goal production diagram.
- Current-vs-target implementation phase table is current.
- Plugin contract is current.
- Safety rules are current.
- Build/test commands are current.
- Knowledge-base notes capture durable workflow and proof-boundary facts.
- Knowledge-base notes include public-repo status, worktree/branch/PR workflow,
  six-agent autonomous sweep expectations, phase-3 worktree names, E2E
  expectations, and proof boundaries without overclaiming production readiness.
- Every phase summary records whether docs, KB facts, and E2E coverage were
  followed; unresolved gaps are blockers for stronger production claims.
- README points to the active design and command gate.
- Mermaid diagrams render in GitHub or the intended documentation viewer.

## Mac App Smoke Test

Current local gate:

- Run `./scripts/packaged-app-release-smoke.sh`.
- The script builds `jarvis-cli` and `JarvisMacApp`, assembles a deterministic
  `Jarvis.app` bundle, writes `Info.plist`, bundles the core at
  `Contents/Resources/bin/jarvis-cli`, ad-hoc signs the bundle when
  `codesign` is available, launches the app executable with isolated endpoint
  and database environment, and verifies health, command, audit, diagnostics,
  emergency pause, blocked command, pause status, resume, and temp-profile
  SQLite state.

Still future gates for production distribution:

- Packaged app launches on a clean Mac user profile.
- App starts and supervises `jarvis-core`.
- Text command reaches the Rust core.
- Text-transcript voice command parity is verified through the scaffold.
- Scheduler attention produces OS-level user notifications with user-visible
  permission handling. The Swift adapter boundary is implemented and tested
  with fakes; live clean-profile notification prompt and delivery still require
  manual verification.
- The macOS Speech/AVFoundation adapter boundary compiles and has deterministic
  fake-adapter state/error tests.
- The AVFoundation speech-output adapter boundary compiles and has
  deterministic fake-adapter state/error tests.
- Real microphone voice command parity is verified only after the packaged app
  has the required entitlements and manual device validation.
- Live text-to-speech playback is verified only after packaged app audio-output
  validation on a real device.
- Activity view shows current task state, active/status counts, and recent
  audit progress through `/activity/summary`.
- CLI activity watch receives bounded `/activity/events` progress events.
- Memory tab can create, edit mutable fields, mark reviewed, soft-delete,
  restore, and include deleted items through the supervised core IPC contract.
- Audit entry is written for the command.
- Emergency pause stops new actions.
- App exits cleanly and restarts with recoverable state.

The current script covers local app-executable launch and ad-hoc signing only.
It does not prove Finder launch, LaunchServices registration, Developer ID
signing, notarization, entitlement validation, installer behavior, App Store
distribution, microphone permissions, real speech capture, or a separate
clean-user manual QA pass.

Distribution packaging gate:

- Run `./scripts/package-distribution.sh --check` on packaging-related PRs to
  validate local tool availability and entitlements templates.
- For a release candidate, set `JARVIS_DEVELOPER_ID_APPLICATION` and either
  `JARVIS_NOTARYTOOL_PROFILE` or the Apple ID/team/password notarytool
  variables, then run `./scripts/package-distribution.sh`.
- Confirm the resulting app is Developer ID signed, notarized, stapled, and
  still passes clean-profile Finder launch/manual QA before any broader
  production distribution claim.

## Release Notes

Release notes must include:

- Version number.
- Summary of user-visible changes.
- Migration notes.
- Migration backup/recovery evidence and any backup privacy implications.
- Known limitations.
- Local verification commands and dates.
- Any manual checks that remain the user's responsibility.
- Any blockers that prevent treating the run as full production assistant
  readiness rather than local foundation evidence.
