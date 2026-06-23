# Release Checklist

Use this checklist before tagging or publishing any Jarvis release. Keep the
evidence local-first unless the user explicitly approves hosted infrastructure.

## Scope Check

- Confirm the release target is this public repository
  (`https://github.com/malak333/Jarvis`) and that the work is landing through a
  reviewable worktree/branch/PR slice.
- Confirm `DESIGN.md` still matches the implementation scope.
- Confirm release notes distinguish implemented Rust foundation and Swift shell
  inspection/control behavior from the implemented opt-in Ollama-compatible local
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
  evidence exists, start or restart the core with
  `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external`, rerun readiness against
  that core, confirm the JSON field `evidence_mode_enabled` is true, and confirm
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
  exact structured payload. Confirm the app bundle metadata and bundled
  `jarvis-cli.version` marker are semantically checked before those items can
  count as present, and that missing or stale marker details point operators to
  rerun `./scripts/package-distribution.sh --unsigned-launch-check` or the
  signed packaging lane. Treat it as file/report inventory plus report semantic
  validation only, not proof that signing, notarization, stapling,
  installation, Finder launch, executable runtime behavior, live-device QA,
  marketplace review, malware scanning, OS sandboxing, or host-level egress
  enforcement was performed.
- Confirm the live-device QA report is `present`, not `invalid`, before using
  external evidence mode. Evidence-status semantically checks the expected
  installed app path, bundle ID, short/build version, bundled-core path/version
  and SHA-256 binding, non-self-test identity, ordered non-future UTC voice-check
  timestamps, observed transcript, command observation, repository-backed
  command-result evidence, and structured scheduler notification observation;
  weak or stale hand-written reports must keep `live_voice_loop` pending.
- Confirm signed-distribution provenance, plugin-trust, and final bundle reports
  are `present`, not `invalid`. Evidence-status checks signed provenance
  version/bundle metadata, bundled core path/version/SHA-256 binding,
  Apple-tool-derived signing/notary/staple/Gatekeeper evidence fields from
  `codesign`, `pkgutil --check-signature`, `xcrun notarytool`,
  `xcrun stapler`, and `spctl`, exact notary `Accepted` statuses,
  notary log paths plus SHA-256 digests, required flags, non-future
  plugin-trust review timestamps, owner-asserted plugin-trust review source,
  final bundle version, artifact/report path matching, SHA-256 digest shape,
  signed-provenance zip/pkg/core/notary-log digests against
  the current artifact files, final-bundle digests against current
  artifacts/reports, semantic validity of the signed-provenance, live-device
  QA, and plugin-trust QA child reports referenced by the final bundle, and
  final-bundle archive URI plus local signature-validation status before
  treating those reports as usable evidence.
- Confirm owner evidence-note validation rejects exact placeholders and
  embedded placeholder wording in live-device, plugin-trust, and final-bundle
  reports; sentences containing `TODO`, `pending`, `fixture`, `example`, or
  `self-test` must not clear external evidence gates.
- Confirm `release-plugin-trust-qa.sh --assert-complete`,
  `release-evidence-bundle.sh --bundle`, and
  `release-evidence-doctor.sh --assert-complete` reject non-UTC plugin-trust
  future-dated timestamps, reversed review windows, and plugin reports generated before the
  recorded review completed, and plugin reports whose `review_source` is not
  `owner-asserted-manual-review`.
- Confirm `release-evidence-doctor.sh --assert-complete` enforces the same
  final-bundle semantic floor as `/release/evidence-status`: non-future UTC generation
  timestamp, `schema_version: 1`, `evidence_type: release_evidence_bundle`,
  expected release version, artifact/report paths matching the configured
  evidence paths, SHA-256-shaped artifact/report digests matching the current files,
  semantic validity of referenced child reports even when their digests match, and
  `validation_flags.local_signature_validation=true`, requires the owner-recorded
  reports archive reference to be a durable URI-shaped location rather than a
  placeholder or self-test path, and rejects a stale packaged
  `jarvis-cli.version` marker beside the bundled core with packaging remediation
  guidance.
- Confirm `release-evidence-doctor.sh --check` prints the follow-up package
  preflight, both supported signing credential forms, external handoff
  directory generator, live-device template/assertion, plugin-trust
  template/assertion, and final evidence-bundle template/bundle commands
  whenever evidence is missing.
- Confirm `release-external-handoff.sh --write target/release-external-handoff`
  creates the sourceable live-device, plugin-trust, and final-bundle env
  templates plus read-only readiness/evidence/runbook JSON snapshots and
  `release-evidence-checklist.md` with the remaining signed-distribution,
  live-device notification, plugin artifact, and archive URI fields, plus
  `release-handoff-manifest.json` binding the generated handoff files to the
  release version, git commit, snapshot endpoint, proof boundary, byte counts,
  and SHA-256 digests. All external validation flags must still default false.
  Treat this as operator handoff scaffolding only, not evidence that the
  external checks were completed.
- Confirm `jarvis release plugin-trust-runbook` hands off from completed
  plugin-trust QA into final evidence bundling and
  `release-evidence-doctor.sh --assert-complete`, not back to the signed
  distribution runbook.
- Confirm `jarvis release evidence-bundle-runbook` and
  `/release/evidence-bundle-runbook` expose the final read-only handoff for
  signed-distribution provenance, live-device QA, plugin-trust QA, and
  `release_evidence_bundle`, and that `release-external-handoff.sh --write`
  includes `evidence-bundle-runbook.json` in the manifest with byte count and
  SHA-256 digest coverage.
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

The public GitHub workflow `.github/workflows/release-local.yml` runs this
same gate on `macos-15` with SHA-pinned checkout/toolchain actions and Rust
`1.95.0` for pull requests, pushes to `main`, and manual dispatch. Treat a
passing workflow as public PR evidence for the repo-owned local gate only; it
is not external signing, notarization, clean-profile installation, Finder
launch, live-device QA, or plugin marketplace trust evidence.
Confirm `/contract`, `jarvis release readiness --json`,
`jarvis release readiness --format json`, and the Swift Release tab expose this
as `release_ci_gate` with the same proof boundary before using CI-passing
language in release notes. Release runbook commands keep the same JSON
compatibility convention: `--json` is canonical, and `--format json` is accepted
for older automation that expects format-style structured output. The CLI
runbook JSON is the operator/snapshot JSON used by release scripts and handoff
E2E tests; the IPC runbook endpoints expose the app-facing
`ReleaseRunbookResponse` with the same release commands, manual checks, proof
boundary, and evidence summaries, but it is a distinct contract shape for Swift
clients.

The script runs the full local gate below, including the opt-in ignored
release-proof E2E test. Run individual commands only when diagnosing a failing
stage or when a PR needs focused evidence for one ownership slice.

- `./scripts/release-version-consistency.sh --check`
- `./scripts/release-ci-workflow-smoke.sh`
- `./scripts/release-docs-drift-smoke.sh`
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo test --workspace -- --ignored`
- `./scripts/storage-migration-backup-smoke.sh`
- `cargo build --workspace`
- `cargo run -p jarvis-cli -- smoke`
- `./scripts/release-operator-qa-smoke.sh`
- `./scripts/release-cargo-package.sh`
- `./scripts/package-distribution.sh --check`
- `./scripts/package-distribution.sh --check-guidance-self-test`
- `./scripts/package-distribution.sh --entitlements-policy-self-test`
- `./scripts/package-distribution.sh --version-consistency-self-test`
- `./scripts/package-distribution.sh --provenance-self-test`
- `./scripts/package-distribution.sh --unsigned-launch-check`
- `cargo run -p jarvis-cli -- release signed-distribution-runbook`
- `cargo run -p jarvis-cli -- release live-device-runbook`
- `cargo run -p jarvis-cli -- release plugin-trust-runbook`
- `./scripts/release-live-device-qa.sh --check`
- `./scripts/release-live-device-qa.sh --self-test`
- `./scripts/release-plugin-trust-qa.sh --check`
- `./scripts/release-plugin-trust-qa.sh --self-test`
- `./scripts/release-evidence-bundle.sh --check`
- `./scripts/release-evidence-bundle.sh --self-test`
- `./scripts/release-evidence-doctor.sh --check`
- `./scripts/release-evidence-doctor.sh --self-test`
- `./scripts/release-external-handoff.sh --check`
- `./scripts/release-external-handoff.sh --self-test`
- `swift test --disable-sandbox --package-path apps/mac`
- `swift build --disable-sandbox --package-path apps/mac`
- Focused supervision proof for branches that touch Swift core launch or bundle
  discovery: `./scripts/packaged-supervision-proof.sh`
- Distribution packaging preflight for branches that touch release packaging,
  signing, entitlements, or notarization:
  `./scripts/package-distribution.sh --check`
- Unsigned distribution launch proof is part of the default local gate:
  `./scripts/package-distribution.sh --unsigned-launch-check`
- Live-device QA preflight is part of the default local gate:
  `./scripts/release-live-device-qa.sh --check`
- Live-device QA operator runbook and current evidence status are available
  without side effects and are part of the default local gate:
  `cargo run -p jarvis-cli -- release live-device-runbook`
- Live-device QA assertion/report mechanics are covered by a fake fixture in
  the default local gate: `./scripts/release-live-device-qa.sh --self-test`
- `swift test --disable-sandbox --package-path apps/mac`
- `swift build --disable-sandbox --package-path apps/mac`
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
- Confirm model responses append bounded `model_output_chunk` audit metadata,
  expose only sequence and byte/character counts with `content_redacted: true`
  through `/activity/events`, and do not expose raw model chunk text on safe
  inspection streams.
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
  claim signing, notarization, stapling, installation, Finder/LaunchServices
  validation, live microphone/Speech validation, spoken transcript handoff, live
  audio-output validation, App Store review, marketplace plugin review, malware
  analysis, or OS sandbox enforcement. The CLI fallback for an unavailable local
  IPC server must keep the same conservative blocker set instead of claiming
  server-backed proof. Confirm `jarvis release readiness --all-commands` is
  ordered as a release execution runbook: local gates, unsigned distribution
  launch check, signed/notarized packaging, live-device QA, plugin-trust QA,
  final evidence bundle generation, evidence-doctor assertion, then external
  evidence-mode readiness.
- Confirm `/release/live-device-runbook`,
  `/release/signed-distribution-runbook`, and
  `/release/plugin-trust-runbook` are present in `/contract` as redacted safe
  inspection endpoints, and that the Swift Release tab can render those
  runbooks without treating them as evidence completion. These endpoints are
  operator guidance only and must not perform signing, notarization,
  installation, live-device QA, plugin-trust review, or final evidence bundling.
- Confirm `./scripts/release-plugin-trust-qa.sh --check` is included in release
  readiness recommendations and the local release gate, and that
  `./scripts/release-plugin-trust-qa.sh --write-template
  target/release-plugin-trust-qa.env` generates a sourceable plugin-trust QA
  template with every `JARVIS_PLUGIN_QA_*` validation flag defaulted to `false`.
  Confirm the readiness runbook also includes the source-and-run
  `target/release-plugin-trust-qa.env` command for `--assert-complete`. Those
  flags may be changed only after the corresponding external plugin trust check
  has actually completed, and every artifact URI must point to a durable
  release evidence archive rather than a placeholder, self-test, fixture, or
  temporary path.
- Confirm `./scripts/release-evidence-bundle.sh --self-test` validates the
  final-bundle reports archive URI path instead of bypassing it: the positive
  fake bundle uses a durable-looking URI, and temporary or non-URI archive
  locations are rejected before a bundle is written.
- Confirm
  `./scripts/release-plugin-trust-qa.sh --self-test` proves only JSON report
  mechanics with fake validation flags and fake evidence notes. The report must
  include `schema_version: 1`, `evidence_type:
  owner_recorded_plugin_trust_qa`, and the current release `version`, while
  final operator evidence must keep `self_test_fixture: false` and
  `review_source: owner-asserted-manual-review`; self-test/imported review
  sources, wrong-version reports, and misidentified report shapes are rejected
  by the doctor/status gates. Treat `--assert-complete` output as
  owner-recorded external evidence for marketplace review, malware
  scanning, signed publisher policy, OS-level process/network sandbox
  validation, host-level egress enforcement, and manual trust review only after
  owner/timestamp/evidence-note fields are present. Every category must also
  include the matching archived artifact URI and SHA-256 digest; structured
  egress evidence must include the policy label plus deny/allow fixture notes.
  Bundle, doctor, and evidence-status revalidation reject temporary plugin
  artifact URIs and bare non-URI artifact locations, so hand-edited reports
  cannot bypass the archived-evidence requirement after generation.
- Confirm CLI E2E coverage still runs
  `release-plugin-trust-qa.sh --assert-complete` with owner-recorded
  archive URI/SHA-256 evidence fields, rebinds the generated report digest into
  the final bundle fixture, and verifies `jarvis release evidence-status`
  accepts the generated plugin-trust QA report and bundle as present. This
  proves script/status compatibility only, not real marketplace, malware,
  sandbox, or host-egress validation.
- Confirm `./scripts/release-evidence-bundle.sh --check` is included in
  release readiness recommendations and the local release gate, that its
  preflight output points operators to the fillable final-bundle template, the
  exact source command, and the exact `--bundle` command, and that
  `./scripts/release-evidence-bundle.sh --self-test` proves only final bundle
  manifest mechanics with fake artifacts/reports plus that operator handoff.
  Confirm
  `./scripts/release-evidence-bundle.sh --write-template
  target/release-evidence-bundle.env` is also included in release readiness
  recommendations and generates a sourceable final-bundle template with every
  `JARVIS_EVIDENCE_*` validation flag defaulted to `false`. Confirm the
  readiness runbook also includes the source-and-run
  `target/release-evidence-bundle.env` command for `--bundle`;
  those flags may be changed only after the corresponding external release
  check has actually completed. Confirm the template keeps
  `JARVIS_EVIDENCE_OVERWRITE_OUTPUT=false`, and that any `true` override is
  used only after preserving the previous bundle artifact. Confirm the final
  bundle output path is distinct from the signed-distribution provenance,
  live-device QA, plugin-trust QA, app zip, and installer package input paths
  so `--bundle` cannot overwrite evidence it has just validated. Confirm the
  readiness runbook also includes
  `./scripts/release-evidence-doctor.sh --assert-complete` after the bundle
  command as the final inventory assertion. Treat `--check`,
  `release-evidence-doctor.sh`, `/release/evidence-status`, and
  `jarvis release evidence-status` as read-only present/missing/invalid
  inventory plus semantic validation for expected paths, app bundle `Info.plist`
  metadata, bundled-core marker metadata, JSON flags, non-future report
  timestamps, signed-distribution provenance, artifact/report digest bindings,
  final-bundle child-report semantic validity, owner-recorded release evidence
  fields, and release metadata. Those paths do
  not perform Developer ID signing, notarization, stapling, installation,
  live-device QA, plugin-trust QA, owner assertions, final bundle creation, or
  host-level egress enforcement.
  Treat `--bundle` output as a manifest of referenced signed/notarized artifacts
  and owner-recorded QA evidence. The production `--bundle` path, unlike
  doctor/status inventory, must keep local signature validation enabled, check
  the app signature, app stapling ticket, installer signature, installer
  stapling ticket, and app zip payload through Apple-tool-derived validation
  before writing the manifest, parse every
  required live-device/plugin-trust report flag, require owner-recorded evidence
  fields in both QA reports, require structured live-device notification
  observation fields for kind/title/body/thread/timestamp with
  `thread_identifier: jarvis.scheduler`, confirm the live-device QA report
  matches the expected app bundle `Info.plist` bundle id/version/build, reject
  future-dated report timestamps, require plugin-trust
  `review_source: owner-asserted-manual-review`, verify signed-provenance zip/pkg/core/notary-log digests
  against the current artifact files and preserved notarytool logs, and write SHA-256 digests for the signed
  distribution artifacts, signed provenance, plus QA reports before writing evidence. The
  disabled-signature path is reserved for the fake self-test fixture.
- Confirm `/release/evidence-status` and `jarvis release evidence-status` expose
  the same standard release evidence inventory as structured, redacted status
  items with `present`, `missing`, or `invalid` state, including signed
  provenance JSON-report validation plus JSON-report
  required-field and semantic validation for owner-recorded live-device,
  plugin-trust, and final bundle evidence. The default readable CLI output
  should include per-item paths and details for present, missing, and invalid
  evidence items when those fields are available, and should mark present
  presence-only artifacts on the same status line, while `--json` preserves
  the exact structured inventory.
- Confirm the Swift Release tab decodes the same `/release/readiness` contract
  and renders blocking gates, recommended commands, implemented proofs, pending
  features, proof boundary, stale cached-readiness state, and structured
  `/release/evidence-status` inventory without enabling release side effects.
  Its production-ready display must use the model's evidence-aware effective
  readiness state, not only the raw readiness payload, so incomplete, invalid,
  missing, or stale evidence keeps the app UI blocked.
- Confirm read-only Release tab runbook load failures surface as warnings while
  readiness and evidence-status remain visible and production-ready stays
  fail-closed.
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
  current Rust/CLI foundation; local packaged Mac launch proof is now covered
  by `./scripts/package-distribution.sh --unsigned-launch-check` for the
  release distribution layout boundary.
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
  implemented packaged Mac smoke lane. Docs-only changes should still name the
  existing proof boundary they preserve.
- Confirm local packaged-app proof remains separate from signed production app
  evidence until a Developer ID signed and notarized app exists.
  `./scripts/packaged-supervision-proof.sh`
  builds the Rust CLI, copies it into a temporary
  `Jarvis.app/Contents/Resources/bin/jarvis-cli` layout, points Swift
  supervisor tests at that executable, and starts the copied binary with a
  repository-backed database to verify health, command, audit, diagnostics,
  emergency pause, blocked command, pause status, and resume surfaces.
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
- Post-merge cleanup audit is recorded before stronger readiness language:
  `gh pr list --state open --json number,title,headRefName,baseRefName,url`,
  `gh run list --workflow release-local.yml --branch main --limit 5`,
  `git worktree list --porcelain`,
  `git branch --merged main --list 'codex/*'`,
  `git branch --no-merged main --list 'codex/*'`, and
  `git status --short --branch`.
- README points to the active design and command gate.
- Mermaid diagrams render in GitHub or the intended documentation viewer.

## Mac App Smoke Test

Current local gate:

- Run `./scripts/package-distribution.sh --unsigned-launch-check`.
- The command builds release Rust/Swift artifacts, assembles the
  distribution-shaped `Jarvis.app`, creates an unsigned installer payload,
  launches the app executable with isolated endpoint and database environment,
  and verifies health, command, audit, diagnostics, emergency pause, blocked
  command, pause status, resume, bundled-core version alignment, and
  temp-profile SQLite state.
- `./scripts/packaged-app-release-smoke.sh` is a deprecated compatibility
  wrapper that delegates to the unsigned distribution launch check.

Clean-profile and manual production gates not proven by this local smoke:

- Packaged app launches on a clean Mac user profile.
- Installed app starts and supervises the bundled `jarvis-core` from
  `/Applications`.
- A text command reaches the Rust core from the clean-profile installed app.
- Typed transcript staging and fake-adapter final-transcript handoff are
  verified locally, but spoken transcript handoff still needs manual
  live-device validation.
- Swift voice capture controls must keep start capture disabled until
  microphone/Speech permissions have been granted; model tests cover the
  permission-before-capture invariant, but live permission prompts and capture
  still require clean-profile device validation.
- Scheduler attention produces OS-level user notifications with user-visible
  permission handling for due, failed, and emergency-pause-blocked attention.
  The Swift adapter boundary is implemented and tested with fakes; live
  clean-profile notification prompt and delivery still require manual
  verification.
- The macOS Speech/AVFoundation adapter boundary compiles and has deterministic
  fake-adapter state/error tests.
- The AVFoundation speech-output adapter boundary compiles and has
  deterministic fake-adapter state/error tests, including natural adapter
  completion returning the model to idle so the preview controls do not stay
  locked in a speaking state after playback finishes, plus utterance identity
  coverage so stale completion/cancel callbacks cannot mark newer playback idle.
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
  The generated template materializes `JARVIS_QA_EXPECTED_VERSION` from the
  canonical Rust package release version instead of leaving a shell placeholder,
  includes one sourceable `JARVIS_RELEASE_CORE_ENDPOINT`, and embeds the
  release-core command evidence capture plus the post-report external
  evidence-mode `release evidence-status` and `release readiness` checks
  against that same endpoint.
  Signed-distribution and plugin-trust runbook evidence-status commands must
  also use the guarded `JARVIS_RELEASE_CORE_ENDPOINT` form before doctor checks
  or final bundling, so endpoint drift is caught by the shell rather than
  hidden in readiness output.
  The generated `release-external-handoff.sh --write` README must use the same
  guarded `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external` and
  `JARVIS_RELEASE_CORE_ENDPOINT` commands for final evidence-status/readiness
  checks rather than placeholder endpoint text.
  All required `JARVIS_QA_*` flags must be set to `true`, including
  `JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true`, plus the required
  owner/device/profile/UTC timestamp, non-voice owner evidence-note, voice
  evidence-note, and structured spoken-command observation fields.
  `--assert-complete` rejects empty or placeholder evidence-note fields,
  including values such as `TODO`, `pending`, `n/a`, `fixture`, or
  `self-test fixture`,
  and `JARVIS_QA_SELF_TEST_FIXTURE=true` is reserved for the script's internal
  fake-fixture self-test rather than release evidence. `/release/evidence-status`
  enforces the same evidence-note checks before the report can clear
  `live_voice_loop`. The installed app path must match the
  expected `/Applications/Jarvis.app` path unless explicitly overridden with
  `JARVIS_QA_INSTALLED_APP_PATH`, the observed transcript must match the spoken
  test phrase after trimming, the expected command text must match the observed
  command text after trimming, `JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID` must be
  `task:<uuid>` or `audit:<uuid>` from live command/audit evidence, and
  `/release/evidence-status` must run against repository-backed IPC state and
  resolve it to an existing task or task-associated audit row before it can
  clear readiness. Fallback/no-server CLI evidence-status treats shape-only
  command evidence as invalid; the live-device and bundle scripts preflight the
  ID shape before repository-backed evidence-status performs the durable lookup.
  The final `release-evidence-doctor.sh --assert-complete` check delegates to
  `jarvis release evidence-status --json`, and `JARVIS_EVIDENCE_STATUS_ENDPOINT`
  can point that assertion at the release core so syntactically valid but
  unresolved task/audit evidence cannot pass. The
  report must bind the installed bundled core path, `jarvis <version>` output,
  and SHA-256 digest. The report generation timestamp must be UTC, no earlier
  than the completed voice check, and not future-dated. Confirm the generated
  report includes installed-app metadata, app microphone/Speech usage
  descriptions, `bundled_core`, all live-device validation flags, `voice_loop`,
  `owner_recorded_live_voice_evidence`, `owner_recorded_non_voice_evidence`,
  `voice_command_observation` including `audio_output_device_label`, schema
  identity, and proof boundary, then preserve the
  `target/release-live-device-qa-report.json`
  artifact, or the `JARVIS_QA_REPORT_PATH` override, with the release notes.
  Preserve `notification_observation` fields for kind, title, body, thread
  identifier, and timestamp in the same report; the assertion path rejects
  blank title/body values, unsupported kinds, non-`jarvis.scheduler` threads,
  malformed timestamps, and notification observations before the voice-check
  start.
  Then rerun `jarvis release evidence-status` and
  `jarvis release readiness` against a core started or restarted with
  `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external` and confirm the live
  voice/audio readiness item is cleared only from valid owner-recorded evidence.
- Activity view shows current task state, active/status counts, redacted recent
  task metadata, and recent audit progress through `/activity/summary` without
  exposing recent task command bodies.
- CLI activity watch receives bounded `/activity/events` progress events.
- Swift Runs tab can request a bounded `/activity/events` stream, render recent
  activity-summary/error frames, and update the visible activity summary without
  starting an unbounded background listener.
- Memory tab can create, edit mutable fields, mark reviewed, soft-delete,
  restore, include deleted items, and render the redacted retention-plan queue
  through the supervised core IPC contract.
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
- Run `./scripts/package-distribution.sh --check` on packaging-related PRs and
  in the default local release gate to validate release packaging prerequisites
  and entitlement templates without performing signing, notarization, stapling,
  installation, or live-device QA.
- `./scripts/package-distribution.sh --check-guidance-self-test` is in the
  default local gate and verifies the no-sign package preflight still prints the
  signed-distribution, live-device, plugin-trust, final-bundle, and doctor
  handoff commands. Its live-device handoff must include the release-core
  command evidence capture, `task:<uuid>`/`audit:<uuid>` evidence-ID recording
  guidance, and endpoint-aware external evidence-mode evidence-status/readiness
  checks before plugin-trust and final bundle handoff. The live-device template
  and runbook use `JARVIS_RELEASE_CORE_ENDPOINT` as the single endpoint value
  for command evidence and post-report readiness checks.
- Run `./scripts/package-distribution.sh --unsigned-structure-check` on
  distribution-layout PRs to build the release app, create an unsigned installer
  package, inspect the payload, and validate package identifier, version, and
  `/Applications` install location metadata without requiring Apple
  credentials. Treat it as structure evidence only, not signing, notarization,
  stapling, installation, Finder/LaunchServices, live device, or manual QA
  proof.
- Run `./scripts/package-distribution.sh --unsigned-launch-check` when a
  packaging change should prove the release-built `Jarvis.app` executable can
  supervise its bundled core from an isolated HOME. This also validates the
  unsigned package metadata. Treat it as local launch and IPC evidence only; it
  still does not prove signing, notarization, stapling, installation,
  Finder/LaunchServices, live device, or manual QA.
- Confirm `jarvis --version` reports the canonical release version and that
  `release-evidence-doctor.sh` / `release-evidence-bundle.sh` accept the
  bundled `Contents/Resources/bin/jarvis-cli --version` output for the same
  version before treating local distribution artifacts as valid evidence.
- For a release candidate, set `JARVIS_DEVELOPER_ID_APPLICATION`,
  `JARVIS_DEVELOPER_ID_INSTALLER`, and either `JARVIS_NOTARYTOOL_PROFILE` or
  the Apple ID/team/password notarytool variables, then run
  `./scripts/package-distribution.sh`.
- Confirm the resulting app zip and installer package are Developer ID signed,
  notarized, and stapled. The script also verifies signed installer package
  identifier/version/`/Applications` metadata, app signature, installer package
  signature, app staple, package staple, notary submission IDs, preserved notary
  log SHA-256 bindings, and Gatekeeper acceptance from the Apple tool output
  recorded in signed provenance.
- Still perform clean-profile installer run, Finder launch, microphone/Speech
  permission prompts, spoken transcript handoff into the command path, live
  audio-output, and manual QA before any broader production distribution claim.
  `./scripts/release-live-device-qa.sh --assert-complete` is the repo-owned way
  to record that those checks were completed; it remains an owner assertion,
  not automated live-device proof. The resulting JSON report records
  owner-asserted validation flags, voice-loop evidence fields, owner-recorded
  live voice and non-voice evidence notes, structured spoken-command
  observation fields, installed-app metadata, schema identity, and proof boundary.
  Confirm the same report is visible through `jarvis release evidence-status`
  without missing, placeholder, or invalid live voice evidence fields before
  using evidence-aware readiness language. Missing required live voice evidence
  notes, the command-result evidence ID, the audio-output device label, the
  notification title/body/thread/timestamp, or the proof boundary keep
  `live_device_qa_report` invalid and keep `live_voice_loop` pending; CLI E2E
  proves `/release/evidence-status` plus external-mode readiness fail closed for
  those missing fields.
  Confirm CLI E2E coverage still runs
  `release-live-device-qa.sh --assert-complete` with a repository-backed
  command result, verifies the script-generated live-device QA report through
  `jarvis release evidence-status`, and confirms external-mode readiness moves
  `live_voice_loop` to implemented while production readiness remains blocked by
  the remaining signed-distribution and final evidence gates. This is
  script/status/readiness compatibility for owner-recorded evidence only, not
  automated real-device microphone, Speech, audio-output, or notification proof.

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
