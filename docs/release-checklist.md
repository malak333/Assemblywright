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
  commands, and supports bounded fake-model, strict provider-envelope, and
  native ChatGPT/OpenAI-compatible first-party tool execution
  before any broader assistant claim.
- Confirm local-model tool discipline/recovery remains represented in the
  architecture map: `/tools/model` as the redacted registered first-party
  model-tool catalog source, Ollama JSON allowlist projection,
  ChatGPT/OpenAI-compatible native tool projection, strict provider envelopes,
  bounded tool requests, invalid-tool rejection, redacted provider failure
  responses, and no installed-plugin or external tool execution through
  model-planned calls.
- Confirm the current-vs-target implementation phase table is up to date before
  using any production-readiness language. Release notes may claim foundation
  readiness only for verified Rust/Swift surfaces, not full assistant readiness.
- Confirm `jarvis release readiness` or `/release/readiness` reports the same
  implemented feature proofs, pending feature boundaries, recommended
  verification commands, and manual production blockers as this checklist.
  The CLI command should default to operator-readable output and also return
  the conservative local readiness summary when no IPC server is running or
  loopback IPC is unavailable, while preserving the same production blockers.
  Use `--all-commands` for the complete readable verification runbook, or
  `--json` or `JARVIS_CLI_JSON=1` for the exact structured payload.
  Treat default readiness as conservative inventory only. After owner-recorded
  evidence exists, rerun readiness with
  `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external` and confirm
  `production_ready: true` only appears when every required
  `/release/evidence-status` item is present, no missing or invalid evidence
  remains, and evidence-cleared features leave no pending readiness features.
  Treat this as validated owner-recorded release evidence, not proof that
  Jarvis performed signing, notarization, stapling, installation,
  live-device QA, marketplace review, malware scanning, or OS sandboxing.
- Confirm `jarvis release evidence-status` or `/release/evidence-status` reports
  the standard signed artifact, live-device QA report, plugin-trust QA report,
  and final evidence bundle inventory. The CLI command should default to
  operator-readable output and use `--json` or `JARVIS_CLI_JSON=1` for the
  exact structured payload. Treat it as file/report inventory plus report
  semantic validation only, not proof that
  signing, notarization, installation, Finder launch, live-device QA,
  marketplace review, malware scanning, or OS sandboxing was performed.
- Confirm the live-device QA report is `present`, not `invalid`, before using
  external evidence mode. Evidence-status semantically checks the expected
  installed app path, bundle ID, short/build version, non-self-test identity,
  ordered non-future UTC voice-check timestamps, observed transcript, and command
  observation; weak or stale hand-written reports must keep
  `live_voice_loop` pending.
- Confirm signed-distribution provenance, plugin-trust, and final bundle reports
  are `present`, not `invalid`. Evidence-status checks signed provenance
  version/bundle metadata, signing/notary/staple/Gatekeeper evidence fields,
  required flags, non-future plugin-trust review timestamps, final bundle version,
  artifact/report path matching, SHA-256 digest shape, signed-provenance
  zip/pkg digests against the current artifact files, final-bundle digests
  against current artifacts/reports, and local signature-validation status
  before treating those reports as usable evidence.
- Confirm `release-plugin-trust-qa.sh --assert-complete`,
  `release-evidence-bundle.sh --bundle`, and
  `release-evidence-doctor.sh --assert-complete` reject non-UTC plugin-trust
  future-dated timestamps, reversed review windows, and plugin reports generated before the
  recorded review completed.
- Confirm `release-evidence-doctor.sh --assert-complete` enforces the same
  final-bundle semantic floor as `/release/evidence-status`: non-future UTC generation
  timestamp, expected release version, artifact/report paths matching the
  configured evidence paths, SHA-256-shaped artifact/report digests matching
  the current files, and
  `validation_flags.local_signature_validation=true`.
- Confirm `release-evidence-doctor.sh --check` prints the follow-up signing,
  live-device template/assertion, plugin-trust template/assertion, and final
  evidence-bundle template/bundle commands whenever evidence is missing.
- Confirm `jarvis release --help`, `jarvis release readiness --help`, and
  `jarvis release evidence-status --help` preserve the same read-only,
  IPC-first/local-fallback, evidence-mode, and file/report-inspection
  boundaries as the JSON and operator-readable surfaces.
- Confirm no Marvel branding, copyrighted visuals, or confusing product claims
  were introduced.
- Confirm any autonomous sweep summary names the active ownership slices and
  states which evidence came from commands, tests, or manual checks. A
  six-agent sweep is coordination context, not proof of readiness.
- Treat older phase/worktree lane names as historical coordination context
  unless the branch is verified active in the current checkout. Current
  readiness should come from `/release/readiness`, checked-in docs, and local
  verification output.
- For each feature/phase, confirm the relevant docs were updated, durable
  knowledge-base facts were added, and matching E2E or focused integration
  coverage exists. If coverage does not exist, add it for behavior changes or
  record the blocker before using broader readiness language.

## Code Gate

- `./scripts/release-local.sh`

The script runs the full local gate below, including the opt-in ignored
release-proof E2E test. Run individual commands only when diagnosing a failing
stage or when a PR needs focused evidence for one ownership slice.

- `./scripts/release-version-consistency.sh --check`
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo test --workspace -- --ignored`
- `./scripts/storage-migration-backup-smoke.sh`
- `cargo build --workspace`
- `cargo run -p jarvis-cli -- smoke`
- `./scripts/release-operator-qa-smoke.sh`
- `cargo package --workspace --allow-dirty`
- `./scripts/package-distribution.sh --version-consistency-self-test`
- `./scripts/package-distribution.sh --unsigned-launch-check`
- `./scripts/release-live-device-qa.sh --check`
- `./scripts/release-live-device-qa.sh --self-test`
- `./scripts/release-plugin-trust-qa.sh --check`
- `./scripts/release-plugin-trust-qa.sh --self-test`
- `./scripts/release-evidence-bundle.sh --check`
- `./scripts/release-evidence-bundle.sh --self-test`
- `./scripts/release-evidence-doctor.sh --check`
- `./scripts/release-evidence-doctor.sh --self-test`
- Focused supervision proof for branches that touch Swift core launch or bundle
  discovery: `./scripts/packaged-supervision-proof.sh`
- Focused packaged app release smoke for branches that touch packaging,
  app-supervised core launch, or Mac release evidence:
  `./scripts/packaged-app-release-smoke.sh`
- Distribution packaging preflight for branches that touch release packaging,
  signing, entitlements, or notarization:
  `./scripts/package-distribution.sh --check`
- Unsigned distribution launch proof is part of the default local gate:
  `./scripts/package-distribution.sh --unsigned-launch-check`
- Live-device QA preflight is part of the default local gate:
  `./scripts/release-live-device-qa.sh --check`
- Live-device QA assertion/report mechanics are covered by a fake fixture in
  the default local gate: `./scripts/release-live-device-qa.sh --self-test`
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
  provenance snapshot metadata, including deterministic source-tree hashes that
  detect helper/resource drift under `source_path`.
- Confirm installed plugin metadata remains disabled by default and becomes
  executable only after local provenance verification reports
  `matches_install_snapshot` and an explicit `subprocess_stdio` execution
  grant is set for non-network actions, or `subprocess_stdio_network` is set
  for actions that declare network access.
- Confirm publisher-origin verification fails closed until local provenance
  matches the install snapshot, requires `trusted_origin` to exactly match the
  installed manifest author claim, persists `origin_claim_verified: true`, and
  appends `installed_plugin_publisher_verified` audit evidence. Do not describe
  this as cryptographic signed-publisher trust.
- Confirm publisher-signature verification fails closed until local provenance
  matches the install snapshot, requires a trusted public key to exactly match
  the signed manifest public key, verifies the Ed25519 manifest signature,
  persists `origin_claim_verified: true`, and appends
  `installed_plugin_publisher_signature_verified` audit evidence with a hashed
  trusted-key reference.
- Confirm network-capable plugin actions must request the `network` permission,
  declare `network_access.mode: declared_hosts`, and list exact plain-hostname
  `allowed_hosts`; invalid hosts, wildcard/scheme/path/port declarations, and
  missing host declarations must fail manifest validation. Confirm executable
  installed plugins with network-declaring actions fail closed under the default
  `subprocess_stdio` grant and run only after `subprocess_stdio_network`.
- Confirm owner-recorded host-level egress evidence names the reviewed
  policy/profile, records an ordered UTC egress validation timestamp, and
  includes both an undeclared-host deny fixture note and a declared-host allow
  fixture note. Treat this as external host-control evidence, not repo-local
  enforcement proof.
- Confirm installed plugin run attempts fail closed with manifest/version and
  action validation, default `execution_enabled: false` semantics, local
  provenance verification, safe command path checks, JSON stdin/stdout, timeout
  enforcement, output schema validation, minimal subprocess environment
  isolation that prevents inherited app/core secrets from reaching plugins,
  durable audit evidence, and `side_effect_executed: false` when no side effect
  is allowed.
- Confirm installed subprocess progress frames are bounded to parsed
  sequence/stage/message events, append `installed_plugin_progress` audit
  evidence, emit redacted `activity_progress` SSE frames through
  `/activity/events`, and do not expose raw stderr in responses, event streams,
  or audit payloads.
- Confirm persistent audit entries remain append-only in SQLite tests.
- Confirm route, policy, approval, action, and failure evidence stay covered
  before claiming an end-to-end assistant release. The current command path
  persists runtime, route, and deterministic first-party plugin audit evidence
  when repository backing is used. It also persists append-only model-route
  records in SQLite and exposes redacted `/model-routes` CLI/IPC inspection
  that survives restart without retaining route context. Approval-required
  first-party command scaffolds persist inspectable pending approvals and record
  CLI/IPC grant or denial decisions without executing side effects. Bounded
  fake-model first-party tool calls, strict Ollama-compatible and
  ChatGPT/OpenAI-compatible provider-envelope first-party tool requests, native
  ChatGPT/OpenAI-compatible first-party `tool_calls`, and provider
  request/error behavior are covered in focused tests; selected
  provider failures must return structured failed command responses with
  redacted `model_step_failed` audit and route evidence. Malformed provider
  tool envelopes must fail with redacted diagnostics, mixed prose plus JSON
  `tool_requests` must not be accepted as normal output, and
  provider-originated tool calls must still pass runtime schema, policy,
  approval, and audit paths; hallucinated provider plugin IDs/actions must fail
  closed before policy checks or tool execution and feed
  `tool_request_rejected` guidance back as rejected tool results for bounded
  recovery; Swift approval decision controls are covered by contract/model
  tests.
- Confirm task, audit, model-route, memory, and plugin manifest inspection
  endpoints still require or use the correct repository/plugin backing and are
  covered by local smoke or focused IPC tests.
- Confirm approval inspection and grant/deny endpoints require repository
  backing, remain side-effect-free, and stay covered by local IPC tests.
- Confirm approved first-party approval execution requires a one-shot explicit
  `/approvals/:id/execute` or `jarvis approvals execute <approval-id>` call,
  verifies the original task action and scope contract against the approval
  record, applies an approval grant only for that replay, updates the task
  result, prevents duplicate replay through existing audit evidence, and
  records `approval_executed` plus plugin completion audit evidence with
  `side_effect_executed: true`.
- Confirm `/permissions/grants` and `jarvis permissions grants` expose
  read-only approval history/counts plus installed-plugin grant state,
  provenance integrity status, unverified plugin counts, and the
  `side_effects_require_approval` invariant. This inspection surface must not
  enable installed plugin code execution.
- Confirm the Swift Plugin tab renders installed-plugin registry records
  read-only, including source path, execution grant, provenance integrity,
  origin-review state, and executable status, and that first-party manifests
  remain visible with a warning when the repository-backed installed registry
  endpoint is unavailable.
- Confirm `/permissions/policy-review` and `jarvis permissions review` expose
  read-only severity-ranked review items for pending approvals, high-risk
  plugin actions, unverified provenance, and unverified origin claims without
  enabling side effects, include network-capable plugin actions, and that
  operator-pinned publisher verification clears the unverified-origin review
  item for that plugin.
- Confirm permission policy review includes active scheduler triggers without
  exposing scheduler command bodies, and that recurring/due triggers remain
  visible before due-job execution.
- Confirm scheduler due-job execution records
  `scheduler_proactive_policy_checked` before command submission, reuses the
  policy-review trigger classification, marks command redaction explicitly, and
  does not expose scheduler command bodies in that policy audit.
- Confirm scheduler due-job execution marks scheduler-originated plugin calls
  as proactive, allows only manifest-opted-in `proactive_run` actions, rejects
  non-proactive plugin actions before side effects, and records redacted
  `plugin_execution_blocked` evidence.
- Confirm scheduler stale-running recovery is bounded and redacted:
  `/scheduler/recover-stale` or `jarvis scheduler recover-stale` marks stale
  `Running` jobs failed with `automatic_recovery: false`; opt-in
  `jarvis serve --scheduler-recover-stale-on-startup` uses the same recovery
  path with `automatic_recovery: true`. Both paths must respect age/limit
  controls, return redacted diagnostic job fields, and append
  `scheduler_stale_running_recovered` without exposing scheduler command bodies
  or running stale job side effects.
- Confirm permission policy review includes unreviewed memory items and deleted
  sensitive memory retained in local storage without exposing memory values, and
  diagnostics export exposes only aggregate active, unreviewed, and sensitive
  memory counts.
- Confirm the Swift Approval Center renders permission policy review status
  alongside grant history when the IPC contract exposes the endpoint, stages
  approved-unexecuted first-party approvals for Run Approved, and hides
  approvals that already have `approval_executed` task-audit evidence.
- Confirm scheduler job create/list/cancel and due-run execution state is
  restored and updated when repository backing is enabled. Due-run coverage
  proves explicit CLI/IPC runner behavior, including interval reschedule and
  fail-closed pause behavior, not background production trigger scheduling.
- Confirm diagnostics export remains redacted and does not include command
  bodies, scheduler commands, model route contexts, audit payloads, memory
  values, raw cancellation reasons, or credentials. Aggregate memory review
  counts are allowed.
- Confirm the Swift Memory tab still uses the Rust IPC memory contract for
  create, load, update of mutable fields, review, soft-delete, include-deleted
  refresh, restore, classification summary, and filtering, with deterministic
  Swift package coverage.
- Confirm the Swift Scheduler tab still consumes `/scheduler/attention` and
  renders redacted due/running/failed attention state without exposing
  scheduler command bodies.
- Confirm `/contract` exposes compatibility policy plus feature proof/boundary
  metadata and Swift decodes it, so release notes can cite implemented surfaces
  without overclaiming pending manual gates.
- Confirm `/release/readiness` and `jarvis release readiness` expose a
  read-only conservative readiness summary derived from contract feature
  metadata and release-checklist blockers, and that it does not perform or
  claim signing, notarization, installation, Finder/LaunchServices validation,
  live microphone/Speech validation, spoken transcript handoff, live
  audio-output validation, App Store review, marketplace plugin review, malware
  analysis, or OS sandbox enforcement. The CLI fallback for an unavailable local
  IPC server must keep the same conservative blocker set instead of claiming
  server-backed proof. Confirm `jarvis release readiness --all-commands` is
  ordered as a release execution runbook: local gates, unsigned distribution
  launch check, signed/notarized packaging, live-device QA, plugin-trust QA,
  final evidence bundle generation, evidence-doctor assertion, then external
  evidence-mode readiness.
- Confirm `./scripts/release-plugin-trust-qa.sh --check` is included in release
  readiness recommendations and the local release gate, and that
  `./scripts/release-plugin-trust-qa.sh --write-template
  target/release-plugin-trust-qa.env` generates a sourceable plugin-trust QA
  template with every `JARVIS_PLUGIN_QA_*` validation flag defaulted to `false`.
  Confirm the readiness runbook also includes the source-and-run
  `target/release-plugin-trust-qa.env` command for `--assert-complete`. Those
  flags may be changed only after the corresponding external plugin trust check
  has actually completed.
- Confirm
  `./scripts/release-plugin-trust-qa.sh --self-test` proves only JSON report
  mechanics with fake validation flags and fake evidence notes. Treat
  `--assert-complete` output as owner-recorded external evidence for marketplace
  review, malware scanning, signed publisher policy, OS-level process/network
  sandbox validation, and host-level egress validation only after
  owner/timestamp/evidence-note fields are present, including the structured
  egress policy label plus deny/allow fixture notes.
- Confirm `./scripts/release-evidence-bundle.sh --check` is included in
  release readiness recommendations and the local release gate, that its
  preflight output points operators to the fillable final-bundle template, and that
  `./scripts/release-evidence-bundle.sh --self-test` proves only final bundle
  manifest mechanics with fake artifacts/reports. Confirm
  `./scripts/release-evidence-bundle.sh --write-template
  target/release-evidence-bundle.env` is also included in release readiness
  recommendations and generates a sourceable final-bundle template with every
  `JARVIS_EVIDENCE_*` validation flag defaulted to `false`. Confirm the
  readiness runbook also includes the source-and-run
  `target/release-evidence-bundle.env` command for `--bundle`;
  those flags may be changed only after the corresponding external release
  check has actually completed. Confirm the readiness runbook also includes
  `./scripts/release-evidence-doctor.sh --assert-complete` after the bundle
  command as the final inventory assertion. Treat `--check`,
  `release-evidence-doctor.sh`, `/release/evidence-status`, and
  `jarvis release evidence-status` as present/missing/invalid inventory for
  expected paths, app bundle `Info.plist` metadata, JSON flags, non-future JSON
  report timestamps, signed-distribution provenance, and release metadata only;
  those paths do not validate Developer ID signing, notarization, stapling,
  installation, live-device QA, plugin-trust QA, owner assertions, or final
  bundle creation.
  Treat `--bundle` output as a manifest of referenced signed/notarized artifacts
  and owner-recorded QA evidence. The production `--bundle` path, unlike
  doctor/status inventory, must keep local signature validation enabled, check
  the app signature, app stapling ticket, installer signature, installer
  stapling ticket, and app zip payload before writing the manifest, parse every
  required live-device/plugin-trust report flag, require owner-recorded evidence
  fields in both QA reports, confirm the live-device QA report matches the
  expected app bundle `Info.plist` bundle id/version/build, reject future-dated
  report timestamps, verify signed-provenance zip/pkg digests
  against the current artifact files, and write SHA-256 digests for the signed
  distribution artifacts, signed provenance, plus QA reports before writing evidence. The
  disabled-signature path is reserved for the fake self-test fixture.
- Confirm `/release/evidence-status` and `jarvis release evidence-status` expose
  the same standard release evidence inventory as structured, redacted status
  items with `present`, `missing`, or `invalid` state, including signed
  provenance JSON-report validation plus JSON-report
  required-field and semantic validation for owner-recorded live-device,
  plugin-trust, and final bundle evidence.
- Confirm the Swift Release tab decodes the same `/release/readiness` contract
  and renders blocking gates, recommended commands, implemented proofs, pending
  features, proof boundary, stale cached-readiness state, and structured
  `/release/evidence-status` inventory without enabling release side effects.
- Confirm the cross-process CLI E2E still covers command, plugin, audit,
  redacted model-route inspection and restart recovery, memory
  classification summary, create/update/review/delete/restore, scheduler
  schedule/get/list/cancel, redacted scheduler trigger policy review,
  redacted scheduler attention handoff, scheduler run-due success/reschedule,
  redacted proactive scheduler policy audit before due command submission,
  explicit and opt-in startup stale-running scheduler recovery after persisted running state,
  scheduler fail-closed pause on non-accepted due
  jobs, diagnostics redaction, persistence restart, and emergency-pause
  blocking/resume behavior. Treat this as the minimum E2E expectation for the
  current Rust/CLI foundation; packaged Mac release smoke is now covered by
  `./scripts/packaged-app-release-smoke.sh` for the local assembled app
  boundary.
- Confirm `./scripts/release-operator-qa-smoke.sh` passes when CLI/operator
  release surfaces change, proving command, audit, routes, memory mutation,
  scheduler attention/run-due, activity, permission review, diagnostics,
  emergency pause, release readiness, and restart recovery in one
  repository-backed local smoke.
- Confirm `./scripts/storage-migration-backup-smoke.sh` passes for storage
  changes, proving legacy DB backup creation, restore after migration-open
  failure, newer-schema diagnostics, and representative schema v1-v8 fixture
  preservation. Treat broad installer upgrade behavior as a separate
  release-candidate gate.
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
  available using `packaging/Jarvis.entitlements`, verifying microphone/Speech
  usage strings plus the packaged app audio-input entitlement, launching the app
  executable under a temporary HOME/profile, and verifying app-supervised core
  health, command, audit, diagnostics, emergency pause, blocked command, pause
  status, resume, and clean-profile SQLite state. This is local packaged app
  evidence only; it is not Developer ID signing, notarization, installer
  validation, live microphone/Speech/audio-output validation, or App Store
  release evidence.
  `./scripts/package-distribution.sh --unsigned-launch-check` is the release
  distribution counterpart: it builds release Rust/Swift artifacts, assembles
  `target/distribution/Jarvis.app`, creates an unsigned installer payload,
  launches the app executable from that release layout with an isolated HOME,
  and verifies bundled-core health, command, audit, diagnostics, emergency
  pause, blocked command, resume, and SQLite state. It is still not Developer
  ID signing, notarization, stapling, /Applications installation,
  Finder/LaunchServices validation, live device validation, or manual QA.

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
- Typed transcript staging and fake-adapter final-transcript handoff into the
  text command path are verified locally.
- Scheduler attention produces OS-level user notifications with user-visible
  permission handling for due, failed, and emergency-pause-blocked attention.
  The Swift adapter boundary is implemented and tested with fakes; live
  clean-profile notification prompt and delivery still require manual
  verification.
- The macOS Speech/AVFoundation adapter boundary compiles and has deterministic
  fake-adapter state/error tests.
- The AVFoundation speech-output adapter boundary compiles and has
  deterministic fake-adapter state/error tests.
- Live microphone/Speech capture, spoken transcript handoff into the same
  command path, and live audio-output playback are verified only after the
  packaged app has the required entitlements and owner-recorded manual device
  validation.
- Live text-to-speech playback is verified only after packaged app audio-output
  validation on a real device.
- Run `./scripts/release-live-device-qa.sh --check` before a release candidate
  to print the live-device runbook. After clean-profile install, Finder launch,
  microphone/Speech permission prompts, spoken transcript handoff into the
  command path, live audio-output, notification, restart, and manual QA are
  actually validated on the release machine, run
  `./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env`,
  fill the generated template, source it, and rerun with `--assert-complete`.
  All required `JARVIS_QA_*` flags must be set to `true`, including
  `JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true`, plus the required
  owner/device/profile/UTC timestamp, voice evidence-note, and structured
  spoken-command observation fields. Owner-recorded evidence fields must contain
  non-whitespace text, and `JARVIS_QA_SELF_TEST_FIXTURE=true` is reserved for
  the script's internal fake-fixture self-test rather than release evidence. The installed app path must match the
  expected `/Applications/Jarvis.app` path unless explicitly overridden with
  `JARVIS_QA_INSTALLED_APP_PATH`, the observed transcript must match the spoken
  test phrase after trimming, the expected command text must match the observed
  command text after trimming, `JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID` must be
  `task:<uuid>` or `audit:<uuid>` from live command/audit evidence, and the
  report generation timestamp must be UTC, no earlier than the completed voice check, and not future-dated. Confirm the generated report includes
  installed-app metadata, `voice_loop`, `owner_recorded_live_voice_evidence`,
  `voice_command_observation`, validation flags, schema identity, and proof
  boundary, then preserve the `target/release-live-device-qa-report.json`
  artifact, or the `JARVIS_QA_REPORT_PATH` override, with the release notes.
  Then rerun `jarvis release evidence-status` and
  `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external jarvis release readiness`
  against that report and confirm the live voice/audio readiness item is
  cleared only from valid owner-recorded evidence.
- Activity view shows current task state, active/status counts, redacted recent
  task metadata, and recent audit progress through `/activity/summary` without
  exposing recent task command bodies.
- CLI activity watch receives bounded `/activity/events` progress events.
- Swift Runs tab can request a bounded `/activity/events` stream, render recent
  activity-summary/error frames, and update the visible activity summary without
  starting an unbounded background listener.
- Memory tab can create, edit mutable fields, mark reviewed, soft-delete,
  restore, and include deleted items through the supervised core IPC contract.
- Audit entry is written for the command.
- Emergency pause stops new actions.
- App exits cleanly and restarts with recoverable state.
- CLI/operator release QA covers repository-backed command, audit, route,
  memory, scheduler, activity, permission, diagnostics, pause, release
  readiness, and restart recovery paths in one local smoke.

The current script covers local app-executable launch and ad-hoc signing only.
It does not prove Finder launch, LaunchServices registration, Developer ID
signing, notarization, entitlement validation, installer behavior, App Store
distribution, microphone permissions, real speech capture, or a separate
clean-user manual QA pass.

Operator QA gate:

- Run `./scripts/release-operator-qa-smoke.sh` for local CLI/operator release
  QA. Treat it as repository-backed command, audit, route, memory, scheduler,
  activity, permission review, diagnostics, pause, release readiness, and
  restart evidence only. It does not prove clean-profile install,
  Finder/LaunchServices launch, live microphone/Speech, live audio output,
  live OS notification delivery, or manual device QA.

Distribution packaging gate:

- Run `./scripts/release-version-consistency.sh --check` before distribution or
  evidence changes to verify release scripts derive one canonical version from
  Rust package metadata.
- Run `./scripts/package-distribution.sh --check` on packaging-related PRs to
  validate local app signing, installer packaging, notarization tool
  availability, and entitlements templates.
- Run `./scripts/package-distribution.sh --unsigned-structure-check` on
  distribution-layout PRs to build the release app, create an unsigned installer
  package, and inspect the payload without requiring Apple credentials. Treat it
  as structure evidence only, not signing, notarization, installation,
  Finder/LaunchServices, live device, or manual QA proof.
- Run `./scripts/package-distribution.sh --unsigned-launch-check` when a
  packaging change should prove the release-built `Jarvis.app` executable can
  supervise its bundled core from an isolated HOME. Treat it as local launch and
  IPC evidence only; it still does not prove signing, notarization, stapling,
  installation, Finder/LaunchServices, live device, or manual QA.
- Confirm `jarvis --version` reports the canonical release version and that
  `release-evidence-doctor.sh` / `release-evidence-bundle.sh` accept the
  bundled `Contents/Resources/bin/jarvis-cli --version` output for the same
  version before treating local distribution artifacts as valid evidence.
- For a release candidate, set `JARVIS_DEVELOPER_ID_APPLICATION`,
  `JARVIS_DEVELOPER_ID_INSTALLER`, and either `JARVIS_NOTARYTOOL_PROFILE` or
  the Apple ID/team/password notarytool variables, then run
  `./scripts/package-distribution.sh`.
- Confirm the resulting app zip and installer package are Developer ID signed,
  notarized, and stapled. The script also verifies the app signature, installer
  signature, app staple, and package staple.
- Still perform clean-profile installer run, Finder launch, microphone/Speech
  permission prompts, spoken transcript handoff into the command path, live
  audio-output, and manual QA before any broader production distribution claim.
  `./scripts/release-live-device-qa.sh --assert-complete` is the repo-owned way
  to record that those checks were completed; it remains an owner assertion,
  not automated live-device proof. The resulting JSON report records
  owner-asserted validation flags, voice-loop evidence fields, owner-recorded
  live voice evidence notes, structured spoken-command observation fields,
  installed-app metadata, schema identity, and proof boundary.
  Confirm the same report is visible through `jarvis release evidence-status`
  before using evidence-aware readiness language.

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
