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
  bounded tool requests, invalid-tool rejection, and redacted provider failure
  responses. `/tools/model` remains the default first-party catalog. Installed
  model tools require the per-command default-false `installed_wasm_tools`
  opt-in, a reactive local-model route, and current eligible `local_wasm`
  records; cloud/proactive routes, ineligible or stale records, identifier
  collisions, and every `local_subprocess` action remain excluded.
- Confirm the Swift console keeps both installed-WASM advertisement and tool
  execution disabled by default. Enabling installed tools alone remains dry-run;
  the separate execution toggle must explicitly disable dry-run and clearly
  applies to all model-planned tools in that console.
- Confirm Swift Model tab behavior remains represented in docs and tests:
  streamed Ollama `/api/pull` progress, automatic `/api/tags` reload after
  download completion, `:latest` installed-model alias handling, and Start gated
  until the selected model is installed. Update-required Ollama pull failures
  should stay normalized into actionable update guidance. The confirmed Ollama
  upgrade action must remain loopback-only and Homebrew-formula-only, use fixed
  executable paths without a shell or user-derived arguments, filter unrelated
  environment values, verify the version before and after mutation, restart only
  an already-running Homebrew service, preserve a stopped service, and surface
  remote/non-formula/command failures without claiming success.
  Keep `ollamaUpgradeProcessEndToEnd` green so the model-to-real-process boundary,
  version transition, exact command sequence, and already-running service restart
  are verified through a temporary fake Homebrew executable without touching the
  host package installation.
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
- For the distributed protocol foundation, run
  `cargo test -p jarvis-protocol --test distributed_protocol_contract_e2e --locked`
  and confirm the serialized Windows-master/Mac-worker handshake, capability,
  leased-job, exact-result, and wrong-lease rejection story remains green.
  Then run
  `cargo test -p jarvis-master --test master_lifecycle_e2e --locked` and confirm
  the file-backed fake worker proves durable success, cancellation, expiry,
  capability bounds, restart abandonment, late-result rejection, and safe
  reissue. Then run
  `cargo test -p jarvis-master --test master_process_e2e --locked` and confirm
  the real master and fixture-worker child processes prove exclusive database
  ownership, bearer non-disclosure, unauthorized and oversized-body denial,
  authenticated loopback health, bounded job completion, and restart
  reconciliation. Treat this as a local development boundary only; Windows
  service installation, remote transport, mTLS, enrollment CA, live MLX
  inference, unified state migration, Codex execution, and cross-machine
  recovery remain unimplemented and unproven.
- For workspace grants, confirm app-selected paths are absent from child argv,
  environment, health, UI presentation, diagnostics, and audit; malformed or
  stale bookmarks block the complete launch; trusted-wake restarts share the
  bounded versioned startup envelope; delivery timeout/failure force-reaps the
  child; access is released on every stop/failure/unexpected child exit;
  and the proof boundary still excludes App Sandbox, child sandbox-extension
  inheritance, same-user/process IPC isolation, signing, and live-device QA.
- Confirm app-supervised IPC defaults to a generation-random UDS in a
  current-owner `0700` runtime directory, creates a `0600` socket at a bounded
  absolute path, uses `unix_socket_peer_identity_v1` with exact
  `peer_code_requirement` and `peer_identity_profile`, validates both running
  peers from `LOCAL_PEERTOKEN` through Security.framework before framing,
  checks current EUID with `getpeereid`, and still requires the per-launch
  bearer on every route. Confirm `adhoc_exact` is cdhash-bound to one build and
  `developer_id_hardened` requires stable app/core identifiers, matching
  nonempty team identity, Developer ID Application requirements, and hardened
  runtime. Confirm the one-frame strict
  JSON protocol requires client write-half EOF before dispatch, rejects trailing
  input, and enforces method/schema/base64 validation plus frame/body/hard-deadline/concurrency
  bounds, restart invalidation, and validated leaf-only cleanup fail closed.
- Confirm only exact `JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true` replaces UDS with
  authenticated loopback TCP and the hardened owner-only CLI token file. If
  `JARVIS_MAC_IPC_AUTH_FILE` is used, confirm it is absolute and effective only
  with that opt-in. Confirm app-only variables and `JARVIS_IPC_TOKEN_FILE` are
  absent from the child; restart/stop/failure clears matching state. Treat the
  compatibility path as weaker same-user-readable bearer possession. The
  default path proves requirement evaluation only for its actual signature
  profile; ad-hoc proof is not Developer ID publisher evidence. Neither path
  proves device authentication, XPC, App Sandbox, notarization, or live-device
  behavior.

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
- `./scripts/package-distribution.sh --running-app-guard-self-test`
- `./scripts/package-distribution.sh --running-app-guard-e2e`
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
- Confirm concurrent trusted-wake provision tests use an async bounded provider
  readiness wait and never block the main actor on a semaphore.
- Distribution packaging preflight for branches that touch release packaging,
  signing, entitlements, or notarization:
  `./scripts/package-distribution.sh --check`
- Unsigned distribution launch proof is part of the default local gate:
  `./scripts/package-distribution.sh --unsigned-launch-check`
- Confirm that unsigned launch proof passes the exact
  `--supervised-parent-pid`, abruptly kills the app, requires the core to
  self-exit and release its UDS/database owner lease, and relaunches against the
  same database without manual cleanup.
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
- Confirm the trusted system-wake rule is enrolled only through bounded
  supervisor stdin after an explicit Provision action, starts disabled,
  requires generation-bound enablement, and
  fails closed for signature/session/replay/skew/cap/pause/policy failures.
  Confirm ordinary app startup never reads the wake Keychain items; failed
  bootstrap preparation leaves the app-owned core running; stop/restart failure
  is visibly degraded; model/provider launch overrides survive the one-shot
  restart; and bootstrap stdin closes with EOF and is not reused.
  Confirm replay-counter recovery from Keychain loss/backward clock, overflow
  failure, restart reconciliation, ambiguous-dispatch visibility, and explicit
  resolve-without-retry evidence.
  Confirm legacy bootstrap cannot mutate an existing enrollment. Confirm
  normal rotation requires old-key session/domain proof, recovery renders and
  requires its stronger exact warning, prepare blocks ambiguous/accepted work
  and quarantines enablement, install consumes one grant and remains disabled,
  expired/near-expiry grants preserve the healthy core, token replay/wrong key/
  old signatures fail, and Swift journal promotion/cancel/crash reconciliation
  preserves the old active key until fingerprint/generation proof. Confirm no
  raw private key, grant token, public-key bytes, or signed proof reaches audit.
  Confirm `key-prepare` exposes only bounded document stdin, has no proof/key/
  confirmation/token argv, and warns that its one-time token response must flow
  directly to trusted device-only journal code. Confirm that code constructs a
  distinct install document and neither form reaches terminal, history, logs,
  or files.
  In the packaged app, recovery crosses the app-supervised bearer-authenticated
  loopback boundary; with an explicitly operator-launched legacy server it is
  unauthenticated. In either case the phrase is destructive-action accident
  prevention, not device authentication, ownership proof, OS identity, or
  same-user/process isolation. Manual SQLite or Keychain mutation is not
  supported recovery.
  Do not treat this as Apple attestation, OS provenance, background-launch,
  exactly-once, live-device, or production evidence.
- Confirm scheduler due-job execution fails closed by activating emergency
  pause, cancelling remaining open scheduler jobs, and recording scheduler
  audit evidence when a due command is not accepted.
- Confirm runtime emergency pause and cancellation tests still cover active
  command cancellation.
- Confirm current Swift and CLI clients generate an optional UUID
  `cancellation_id` before `POST /commands`, duplicate or over-capacity handles
  fail closed, and Swift exposes Cancel only while its own submission remains
  active. Run
  `explicit_command_handle_cancels_only_its_active_model_transport`,
  `command_cancellation_response_distinguishes_active_from_not_found`,
  `active_command_cancellation_is_end_to_end_and_finalized_handles_report_not_found`,
  and `commandConsoleCancelsItsActiveSubmission`. Verify authenticated
  `/runtime/cancellations/:id` reports `cancellation_requested` only for an
  active exact handle and `not_found` after final acceptance, does not cancel
  unrelated work, and suppresses late model steps/tool results when
  cancellation wins. Record the cooperative boundary: it cannot reverse an
  external effect already applied and is not distributed cancellation or crash
  recovery. Verify the 1,024-entry bounded FIFO consumed-ID tombstone rejects
  recent UUID reuse and a stale cancellation remains `not_found`; document that
  eviction or restart ends this process-local protection, so clients must
  generate fresh random UUIDs. Also run
  `commandConsoleSerializesConcurrentSubmissions` to prove keyboard/voice/direct
  overlapping submits cannot overwrite or orphan the active Swift handle.
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
- Confirm every installed subprocess starts in a dedicated Unix process group.
  Active cancellation and emergency pause, plus timeout, output-limit,
  input/output failure, and leader-exit cleanup, must terminate the full group
  with bounded TERM-to-KILL escalation, reap the leader, join bounded I/O
  workers, suppress output, and prevent the in-group descendant heartbeat from
  after return. Authenticated approved-execution E2E must prove the cancellation
  remains effect-possible, automatic retry stays disabled, and restart cannot
  replay the consumed approval. Do not describe process-group termination as
  containment of deliberate `setsid`/`setpgid` escape, effect rollback, OS
  sandboxing, or host-level egress enforcement.
- Confirm direct installed-plugin requests evaluate action risk and explicit
  sensitivity through `PermissionEngine`: contract dry runs remain
  non-executing, eligible Low/default-sensitivity actions remain compatible,
  and Confirm or sensitive invocations return a pending approval without Wasmi
  or subprocess entry. Confirm schema v15 atomically binds the waiting task and
  approval to canonical input, exact manifest/provenance contract, and execution
  grant while approval-required run responses, approval records, audits,
  permission views, and diagnostics omit the bound input and binding digests.
  Do not confuse schema-validated plugin output after approved execution with
  disclosure of the private binding fields.
- Confirm installed subprocess progress frames are bounded to parsed
  sequence/stage/message events, append `installed_plugin_progress` audit
  evidence, emit redacted `activity_progress` SSE frames through
  `/activity/events`, and do not expose raw stderr in responses, event streams,
  or audit payloads.
- Confirm model responses append bounded `model_output_chunk` audit metadata,
  expose only sequence and byte/character counts with `content_redacted: true`
  through `/activity/events`, and do not expose raw model chunk text on safe
  inspection streams.
- Confirm Ollama requests native NDJSON streaming, rejects malformed, empty,
  oversized, unterminated, duplicate-terminal, post-terminal, and provider-error
  streams, handles split UTF-8 plus LF/CRLF frames, and exposes no partial text
  or tool envelope before terminal validation. Confirm emergency pause/task
  cancellation drops the active transport and wins a completion race before
  model audit or any registered tool execution. Swift must keep transcript output
  final-only and label post-validation transport counts as redacted metadata,
  not live token rendering.
- Confirm persistent audit entries remain append-only in SQLite tests.
- Confirm route, policy, approval, action, and failure evidence stay covered
  before claiming an end-to-end assistant release. The current command path
  persists runtime, route, and deterministic first-party plugin audit evidence
  when repository backing is used. It also persists append-only model-route
  records in SQLite and exposes redacted `/model-routes` CLI/IPC inspection
  that survives restart without retaining route context. Approval-required
  first-party command and direct installed-plugin scaffolds persist inspectable
  pending approvals and record
  CLI/IPC grant or denial decisions without executing side effects. Bounded
  fake-model first-party tool calls, strict Ollama-compatible and
  ChatGPT/OpenAI-compatible provider-envelope first-party tool requests, native
  ChatGPT/OpenAI-compatible first-party `tool_calls`, explicit reactive-local
  installed WASM tool requests, and provider
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
  backing, remain side-effect-free, and stay covered by local IPC tests. Each
  grant or denial must use one immediate transaction to recheck pending state,
  update the decision, and append a redacted decision audit. Injected audit
  failure must roll all decision fields back to pending across restart, keep
  `/execute` unauthorized, and leave no unaudited grant chain. Free-form actor
  and reason text must stay out of the audit payload.
- Confirm approved first-party or installed-plugin approval execution requires
  a one-shot explicit
  `/approvals/:id/execute` or `jarvis approvals execute <approval-id>` call,
  verifies the original task, action, risk, scope, input-schema, and current
  policy contract against the approval record, and requires matching
  approval_granted audit evidence before it uses schema v13 to
  atomically create one unique durable execution claim with redacted
  policy/claim audit evidence before plugin invocation. Confirm only one
  claimant runs, duplicate and post-restart replay returns conflict/HTTP 409,
  historical `approval_executed` rows migrate as consumed, and terminal
  execution state, task state, and terminal audits commit together. A durable
  claim permanently consumes the approval: failure, cancellation, timeout,
  restart, or storage interruption after claim can leave the effect ambiguous,
  so automatic retry is forbidden; the operator must inspect audit evidence
  and create a new approval when another attempt is appropriate.
  Installed-plugin replay must additionally verify canonical-input integrity and
  an unchanged schema-v15 manifest/provenance/grant binding before the claim;
  mutation, pause, cancellation, or current-policy failure must prevent runtime
  entry. Any failure after claim consumes authority and remains non-retryable.
  Confirm CLI and Swift attach a fresh approved-execution `cancellation_id`,
  authenticated cancellation targets only that active claimed run, a winning
  race discards output and durably records cancelled claim/task state, and a
  later cancellation reports `not_found`. Record that already-performed
  external effects cannot be reversed.
  An approved row without matching evidence must create no durable claim or
  policy/claim audit and must not enter the plugin, including after restart.
  Unrelated audit substitution must fail. The exact legacy raw-metadata audit
  shape remains compatible only when all authority and decision metadata match
  and its timestamp is not before the decision. Current redacted evidence must
  match actor/reason-presence booleans without raw keys; legacy evidence must
  match exact raw actor/reason values without redaction/presence keys;
  the claim path must never fabricate grant evidence.
  Confirm restart projects a pre-existing unresolved claim into the redacted
  approval-execution attention queue before serving IPC. Confirm the list omits
  action/input/reason/actor/path/digest data and reports a consistent true
  total, returned count, 100-item limit, and truncation flag; exact-revision
  `acknowledged_without_retry` succeeds once; stale and replayed CAS requests
  conflict; revision overflow is rejected before IPC; Swift requires the exact
  successor revision and identical execution/approval/task IDs before clearing;
  acknowledgement survives restart; and execution replay remains
  HTTP 409. Acknowledgement must not invoke a plugin, mutate/delete the claim,
  or create another approval.
  Confirm file-backed startup acquires a secure sibling owner lease before
  backup/version/migration and holds it for repository lifetime. A second core,
  symlink lock, non-`0600` lock, hard link, wrong owner, or unsupported locking
  platform must fail before database open/mutation; lease release must permit a
  later clean owner. Confirm two-process same-database E2E keeps a live schema-v15
  claim out of startup reconciliation until the first owner exits, then performs
  the v16 migration/reconciliation once. Do not claim this advisory lease blocks
  raw SQLite or other noncooperating writers.
- Confirm `/permissions/grants` and `jarvis permissions grants` expose
  read-only approval history/counts plus installed-plugin grant state,
  provenance integrity status, unverified plugin counts, and the
  `side_effects_require_approval` invariant. This inspection surface must not
  enable installed plugin code execution.
- Confirm the Swift Plugin tab renders installed-plugin registry records,
  including source type, execution grant, provenance integrity,
  origin-review state, executable status, and redacted runtime confinement
  fields while omitting source/module/command paths and bytes, and that
  first-party manifests remain visible with a warning when the
  repository-backed installed registry endpoint is unavailable.
- Confirm its lifecycle controls use typed provenance/execution endpoints,
  serialize mutations per plugin, require matching provenance before enable,
  require an explicit choice for mixed compatible grants, display exact
  declared permissions and network hosts, bind the exact confirmed grant and
  lifecycle-contract digest, reject stale/reinstalled records, disable only to
  `metadata_only`, and refresh authoritative records after both success and
  failure. A stale registry must disable all lifecycle actions. Verify and set
  responses must retain the redacted inspection projection. The controls
  must never install or run plugin code or optimistically update authority.
  Confirm the current subprocess warning states that OS sandboxing and
  host-level egress enforcement are absent.
- Confirm local plugin update requires an explicitly selected replacement
  manifest, validates it as untrusted input, requires the exact installed
  plugin identity and a valid SemVer candidate, and rejects same-version or
  lower-precedence updates for SemVer records. Confirm new local installs reject
  non-SemVer versions and a persisted pre-SemVer record can cross once to valid
  SemVer without bypassing any identity, source-kind, CAS, snapshot, disablement,
  or audit check. Confirm preview/apply binds to
  the inspected lifecycle digest and opaque `candidate_update_contract_sha256`,
  recomputes the exact candidate snapshot binding at apply time, captures fresh
  provenance, and atomically resets the record to disabled `metadata_only` with
  a redacted non-execution audit. Verify the
  prior record survives validation/audit failure unchanged and no update path
  starts plugin code. Confirm the new snapshot must be verified and explicitly
  re-enabled; prior verification, grant, and lifecycle digest do not carry
  forward.
- Confirm `POST /plugins/installed/:id/update/preview` is inspection only,
  `POST /plugins/installed/:id/update/apply` requires `confirmed: true`, and
  `GET /plugins/installed/:id/history` entries return only entry ID, plugin ID,
  lifecycle action, normalized outcome, and timestamp.
- Confirm the candidate update token is exposed only as opaque aggregate
  compare-and-set data, is not a raw component provenance hash or trust signal,
  and rejects manifest/source/entrypoint drift between preview and apply.
- Confirm preview echoes its validated lifecycle digest; CLI and Swift apply
  must require that exact reviewed lifecycle/candidate token pair and must not
  automatically fetch, preview, refresh, or substitute either value.
- Exercise the operator flow against a repository-backed core:
  `plugins update-preview`, then `plugins update-apply --confirm`, followed by
  `plugins history`. Copy the preview's `current_lifecycle_contract_sha256`
  into `--expected-lifecycle-contract-sha256` and its
  `candidate_update_contract_sha256` into
  `--expected-candidate-update-contract-sha256`; do not refresh either value.
  Confirm the
  applied record is disabled `metadata_only`, requires new verification, and
  each history entry exposes only the five documented public fields.
- Run `cargo test -p jarvis-core semantic_version_update -- --nocapture` and
  `cargo test -p jarvis-core
  installed_plugin_update_is_cas_bound_atomic_and_persistent -- --nocapture`,
  `cargo test -p jarvis-core
  installed_plugin_update_rejects_changed_candidate_and_rolls_back_on_audit_failure
  -- --nocapture`, `cargo test -p jarvis-core
  installed_plugin_history_is_plugin_scoped_newest_first_and_bounded --
  --nocapture`, and `cargo test -p jarvis-cli --test local_ipc_e2e
  installed_plugin_update_preview_apply_history_is_cas_bound_redacted_and_persistent
  -- --nocapture`. Also run
  the focused Swift filters `pluginUpdateClientUsesTypedRedactedContracts`,
  `pluginManagerUpdateRequiresPreviewAndConfirmation`, and
  `pluginLifecycleHistoryFailureDoesNotStaleRegistry`.
- Confirm lifecycle-history inspection is bounded, ordered, and redacted: it
  exposes only entry ID, plugin ID, lifecycle action, normalized outcome, and
  timestamp per entry plus fixed wrapper redaction/proof metadata while omitting
  paths, hashes, signature material, subprocess configuration,
  input/output, secrets, and free-form operator text. Do not treat that history
  as publisher identity, marketplace approval, malware analysis, OS-sandbox,
  host-egress, signing/notarization, or live-device proof.
- Confirm Rust commits execution authority and the redacted
  `installed_plugin_execution_authority_updated` audit in one immediate
  transaction and rolls authority back when audit insertion fails. Run
  `cargo test -p jarvis-core installed_plugin_execution_authority_and_audit_commit_atomically -- --nocapture`.
- Confirm authenticated loopback-TCP compatibility E2E covers enabled restart,
  disabled restart, persisted audit inspection after each restart, malformed
  disabled-grant and stale-digest rejection, raw mutation-response redaction,
  and a no-execution sentinel. Do not claim this as default-UDS transport proof.
- Confirm installed `local_wasm` records require source `local_wasm`, grant
  `wasm_compute`, exact module-byte provenance, and `jarvis_json_v1` exports
  `memory`, `jarvis_alloc`, and `jarvis_run`; reject every import including
  WASI, environment, filesystem, network, clock, and process authority.
- Confirm WASM actions are low-risk, non-proactive compute only with no
  memory/model/network permission, and fail closed above 4 MiB module,
  256 KiB request, 1 MiB output, 16 MiB memory, zero table elements, or 10 million fuel.
- Confirm emergency pause, cooperative cancellation, timeout, traps, and fuel
  exhaustion discard WASM output; dry-run does not compile or invoke code; and
  audit/inspection omit module bytes, paths, hashes, request/output bodies, and
  raw engine errors.
- Confirm installed WASM model planning defaults off in IPC, CLI, and Swift and
  is enabled only for the individual command. The runtime-derived extension
  catalog must appear only after selection of a reactive local-model route and
  contain only enabled `wasm_compute`, current exact-provenance, low-risk,
  non-proactive compute actions with no memory/model/network permission or
  imports. Cloud/proactive requests and commands without the opt-in must not
  advertise or execute an installed model tool. Confirm deterministic catalog
  limits of 16 actions, 1 KiB per description, 16 KiB per input schema, and
  64 KiB combined fail closed without displacing earlier eligible actions.
- Confirm private, credential-adjacent, and restricted model-planned installed
  WASM requests return `approval_required`, leave the task waiting, and do not
  enter the guest until the normal sensitivity confirmation policy is met.
- Confirm discovery prefilters enabled `wasm_compute`, snapshots at most 64
  candidates under the repository mutex, hashes provenance only after unlock,
  and rechecks an unchanged record before advertisement. Confirm source-tree
  provenance rejects more than 8,192 entries, 4,096 files, 64 levels, or 64 MiB.
- Confirm model-planned installed WASM rejects first-party identifier
  collisions, stale/mutated/disabled/ineligible records, and every
  `local_subprocess` action. Repeat grant, action schema, eligibility, and exact
  provenance validation immediately before guest entry; advertisement is not
  execution authority. Catalog, rejection, diagnostics, progress, and audit
  data must omit module bytes, paths, hashes, publisher material,
  subprocess configuration, request bodies, and output bodies.
- Confirm `cancellation_id` plus `/runtime/cancellations/:id` blocks a
  cross-process WASM run, and keep legacy subprocess late-result suppression
  distinct from prevention of already-issued effects.
- Confirm installed subprocess and WASM execution snapshot/revalidate state and
  current provenance under the repository mutex, release it before guest work,
  and check pause/cancel after unlock and before output/completion-audit
  acceptance; unrelated repository operations must not wait on guest execution.
- Confirm schema v12 migrates legacy installed-plugin state without enabling or
  broadening existing grants, then preserves WASM grant/provenance state across
  restart. Run `cargo test -p jarvis-core wasm -- --nocapture` and
  `cargo test -p jarvis-cli --test local_ipc_e2e installed_wasm -- --nocapture`.
- Confirm the Swift Plugin tab presents redacted WASM
  records as `WASM confined • no imports • no filesystem • no network`, while
  presenting `local_subprocess` as `not OS sandboxed`; lifecycle actions change
  authority only and do not install or execute code. Run
  `swift test --disable-sandbox --package-path apps/mac --filter
  pluginManager` plus the focused typed-client, app-presentation, and
  authenticated real-core lifecycle E2E filters documented in
  `docs/build-test-commands.md`.
- Do not use Wasmi evidence to claim an OS sandbox, host-egress enforcement,
  same-user IPC isolation, marketplace/publisher trust, malware analysis,
  signing/notarization, or live-device validation. Those remain separate
  external release gates.
- Preserve this feature proof boundary in `/contract`, readiness, release docs,
  and PR evidence: disabled by default and local-model-only; eligibility is
  limited to explicitly enabled installed `local_wasm` actions with
  `wasm_compute`, current exact-byte provenance, low-risk non-proactive
  compute-only schemas, and no memory, model, network, or import authority.
  `local_subprocess` and cloud routes remain excluded. Wasmi provides
  guest-language confinement, not OS sandboxing, marketplace/publisher trust,
  malware analysis, same-user/process IPC isolation, signing/notarization, or
  live-device evidence.
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
- Confirm packaged scheduler automation defaults off, persists only an explicit
  user opt-in, and takes effect through a deliberate app-supervised core
  restart with bounded background and optional stale-recovery arguments.
  Confirm the attention coordinator is single-flight and cancellable, skips an
  unavailable core, consumes only redacted attention plus the bounded durable
  occurrence outbox, never prompts for notification permission, and rechecks
  lifecycle acceptance after asynchronous authorization. Confirm due claim is
  durable before execution, failure/stale recovery atomically revision-escalates
  the same occurrence, acknowledgements use revision CAS after app submission or
  explicit no-authorization suppression, and restart replays unacknowledged
  occurrences. This is at-least-once handoff; a pre-ack crash may repeat the
  stable request. Repeated app starts must preserve active automation for
  the same supervised child and must not claim it for an external core. The
  unsigned packaged launch must expose the expected child
  arguments without claiming LaunchAgent, OS wake, or live notification proof.
- Confirm permission policy review includes unreviewed memory items and deleted
  sensitive memory retained in local storage without exposing memory values, and
  diagnostics export exposes only aggregate active, unreviewed, and sensitive
  memory counts.
- Confirm memory context remains disabled by default in CLI and Swift, and is
  accepted only for explicit non-proactive local-model commands with a current
  index. Reviewed active Public/Workspace/Personal records are the only eligible
  inputs; Private, CredentialAdjacent, Restricted, unreviewed, deleted,
  missing/stale/corrupt, and over-budget inputs fail closed before model use.
- Confirm bounded retrieval enforces the 4 KiB query, 64-term, 128-byte-term,
  16 KiB item, 1 MiB corpus, four-result, and 4 KiB context ceilings; checks
  pause/cancellation; frames context as untrusted data; rejects cloud transport;
  does not duplicate the query into retrieval-specific audit fields; and keeps
  retrieved values, keys, provenance, IDs, scores, and context out of audit,
  diagnostics, route evidence, errors, and debug output. Existing task, route,
  and model-request surfaces retain their normal user-command visibility.
- Confirm the Swift Approval Center renders permission policy review status
  alongside grant history when the IPC contract exposes the endpoint, stages
  approved-unexecuted first-party and installed-plugin approvals for Run
  Approved, and hides
  approvals that already have `approval_executed` task-audit evidence.
- Confirm scheduler job create/list/cancel and due-run execution state is
  restored and updated when repository backing is enabled. Due-run coverage
  proves explicit CLI/IPC runner behavior, including interval reschedule and
  fail-closed pause behavior, not background production trigger scheduling.
- Confirm diagnostics export remains redacted and does not include command
  bodies, scheduler commands, model route contexts, audit payloads, memory
  values, arbitrary emergency-pause reason text, raw cancellation reasons, or
  credentials. Its dedicated health projection may expose
  `emergency_pause_reason_present` plus a null or fixed `redacted` compatibility
  marker; explicit health/pause operator surfaces may retain the reason.
  Aggregate memory review counts are allowed. Run the core
  secret-sentinel test, authenticated real-server CLI E2E, and Swift
  decode/presentation test documented in `docs/build-test-commands.md`.
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
  matches the expected app bundle `Info.plist` bundle id/version/build and
  approved microphone/Speech privacy prompt copy, reject future-dated report
  timestamps, require the installed app executable SHA-256, code Identifier,
  TeamIdentifier, and CDHash to match the exact signed-provenance report path
  and SHA-256, reject cross-report artifact or identity drift, require plugin-trust
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
  failure, newer-schema diagnostics, and representative schema v1-v13 fixture
  preservation. Treat broad installer upgrade behavior as a separate
  release-candidate gate.
- Confirm local plugin metadata install/list/get coverage remains in that E2E
  path. Direct subprocess execution coverage applies only after an explicit
  `subprocess_stdio` grant. Model-planned installed execution coverage applies
  only to an explicitly opted-in reactive local route and an eligible
  `wasm_compute` record. Cross-process Ollama-stub E2E must include successful
  bounded execution plus default-off, subprocess exclusion, mutation denial,
  pre-entry non-execution evidence, and redaction. Runtime unit coverage must
  retain cloud/proactive and collision exclusion; the installed-WASM
  confinement E2E must retain disabled, pause, cancellation, timeout, and
  budget rejection paths.
- For each new executable feature phase, confirm E2E coverage is either part of
  `local_ipc_e2e`, Swift package tests, a focused integration proof, or the
  implemented packaged Mac smoke lane. Docs-only changes should still name the
  existing proof boundary they preserve.
- For menu-bar changes, run `swift test --package-path apps/mac` and preserve
  contract coverage for the stable main-window scene route plus every
  supervisor lifecycle presentation state. Treat actual menu-bar rendering,
  reopening after window closure, and lifecycle actions in a signed installed
  app as manual Finder/LaunchServices QA; Swift package tests do not prove
  those live GUI behaviors.
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
  and verifies that the app-owned Swift client completes bundled-core health,
  dry-run command, task/audit inspection, diagnostics, emergency pause, blocked
  command, and resume over the default UDS before a separate explicit TCP/token
  compatibility relaunch. It also verifies SQLite state. It is still not Developer
  ID signing, notarization, stapling, /Applications installation,
  Finder/LaunchServices validation, live device validation, or manual QA.

## Documentation Gate

- Architecture map is current.
- Both architecture diagrams render: the current implementation diagram and the
  end-goal production diagram.
- Current-vs-target implementation phase table is current.
- Plugin contract is current.
- Audited local plugin update and redacted lifecycle-history contracts are
  current, and the current/end-goal diagrams preserve their trust boundary.
- Production first-party inventory excludes `fake_*`, and configured-root
  workspace list/read coverage proves no-follow containment, secret/type/size
  denials, local-model-only results, cancellation/pause dominance, and
  metadata-only audit. No-root startup proves the workspace tools are absent.
- Upgraded repositories with unresolved historical `fake_*` approvals expose
  critical `removed_fixture_approval` policy-review attention; the removed
  fixture action remains unexecutable and history is not silently deleted.
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
  and verifies health, dry-run command, task/audit inspection, diagnostics,
  emergency pause, blocked command, pause status, and resume through the
  app-owned Swift client on default UDS, plus bundled-core version alignment and
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
  includes one sourceable `JARVIS_RELEASE_CORE_ENDPOINT` plus the app-owned
  `JARVIS_IPC_TOKEN_FILE` path, and embeds the
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
  and SHA-256 digest. It must also bind the installed app executable path,
  SHA-256, code Identifier, TeamIdentifier, and CDHash to the exact
  signed-provenance report path/SHA-256 after local codesign, stapler, and
  Gatekeeper validation. The report generation timestamp must be UTC, no earlier
  than the completed voice check, and not future-dated. Confirm the generated
  report includes installed-app metadata, app microphone/Speech usage
  descriptions, `bundled_core`, all live-device validation flags, `voice_loop`,
  `owner_recorded_live_voice_evidence`, `owner_recorded_non_voice_evidence`,
  `voice_command_observation` including `audio_output_device_label`, schema
  identity, and proof boundary, then preserve the
  `target/release-live-device-qa-report.json`
  artifact, or the `JARVIS_QA_REPORT_PATH` override, with the release notes.
  The installed app metadata must match the approved `Info.plist` copy exactly:
  `NSMicrophoneUsageDescription` is `Jarvis uses microphone input only when you explicitly start local voice capture.`, and
  `NSSpeechRecognitionUsageDescription` is `Jarvis uses speech recognition only to turn your spoken command into a local assistant request.`.
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
- Memory tab renders count-only index status and can explicitly rebuild the
  atomic local projection from canonical SQLite records; update/delete/restore,
  restart persistence, missing/corrupt recovery, and redaction have E2E proof.
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
  unsigned package metadata. Confirm the default lane uses a `0700` owner-only
  run directory, `0600` generation-random socket, audit-token requirement plus
  same-EUID peer checks, the per-launch bearer, no credential handoff file or TCP listener, and cleanup of
  only the validated socket leaf. Require the non-secret app readiness line only
  after the Swift client completes authenticated health, dry-run command,
  task/audit inspection, diagnostics, pause, blocked-command, and resumed-state
  verification over that UDS. Confirm failures suppress the line and a
  post-pause failure makes a bounded best-effort resume attempt. Then require the
  same-EUID wrong-code Python probe to be closed/reset before any framed `401`,
  while the legitimate Swift route remains healthy. Confirm the app and bundled
  core use stable `com.nobiletechnology.jarvis` and
  `com.nobiletechnology.jarvis.core` code
  identifiers. Then require the
  supervised child to exit and its socket to disappear before the compatibility
  relaunch. Treat it as local launch, bounded UDS/bearer, and exact-build ad-hoc
  code-identity mechanics only; it does not prove Developer ID publisher
  identity, device authentication, XPC, App Sandbox, notarization, stapling,
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
