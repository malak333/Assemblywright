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
  execution and records audit evidence. For provider-originated tool requests,
  `status` is an action on the registered `fake_status` plugin, not a valid
  standalone `plugin_id`; `chrome_extension` is also unavailable unless it
  appears in `/tools/model` or `jarvis tools list`.
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
  counts, active task count, redacted recent task metadata, and recent audit
  entries. It omits command bodies from recent tasks and is deterministic
  repository evidence for current activity.
- Repository-backed IPC also exposes `/activity/events`, and the CLI exposes
  `jarvis activity watch`, as bounded server-sent events carrying activity
  summary snapshots with redacted recent task metadata and redacted
  installed-plugin progress frames. This is local progress-streaming evidence
  for current task/audit state, not per-token model streaming. The Swift Runs tab can manually watch a bounded event stream,
  decode `activity_summary`, `activity_progress`, and `activity_error` frames,
  update the visible activity summary from the latest summary event, and render
  plugin progress stage/message text without opening an unbounded background
  listener.
- Installed `local_subprocess` plugins can emit bounded newline-delimited
  stderr JSON frames with `jarvis_progress: true`, `stage`, and `message`.
  Jarvis records parsed sequence/stage/message events in the run response and
  append-only audit entries, then emits redacted `activity_progress` SSE frames
  from recent audit evidence while redacting raw stderr. This is bounded,
  audit-backed plugin progress evidence, not per-token or unbounded real-time
  plugin UI streaming.
- `/contract` includes a `compatibility` block with supported version range,
  additive-change, deprecation, removed/deprecated endpoint, and client
  requirement policy, plus a `features` list with stable keys, status, proof,
  and boundary fields so Swift and release docs can distinguish implemented
  repo-owned surfaces from manual or target production claims without scraping
  prose.
- `/release/readiness` and `jarvis release readiness` derive a conservative
  read-only release summary from contract feature metadata, release-checklist
  blockers, and explicitly enabled release evidence status. Default readiness
  treats standard `target/` evidence files as inventory only; with
  `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external`, readiness can compute
  `production_ready: true` only when every required `/release/evidence-status`
  item is present, no missing or invalid evidence remains, and
  evidence-cleared features leave no pending readiness features. This remains
  validated owner-recorded release evidence, not Jarvis-performed signing,
  notarization, stapling, live-device QA, plugin trust QA, or manual release
  QA. The CLI command prefers the IPC endpoint when it is running
  and falls back to the same local `IpcState` readiness summary when the server
  is unavailable, so operator triage does not require a prestarted core.
  `operator_release_qa_smoke` is an implemented readiness feature for the local
  repository-backed operator QA lane; it does not clear clean-profile
  installed-app or live-device manual gates.
- `/release/evidence-status` and `jarvis release evidence-status` expose the
  standard release evidence doctor inventory as structured JSON with present,
  missing, or invalid status for signed artifact paths, signed-distribution
  provenance report, live-device QA report, plugin-trust QA report, and final
  evidence bundle. The app bundle item additionally validates `Info.plist`
  bundle id, short version, and build version against expected release metadata,
  and the bundled core item validates the packaged `jarvis-cli.version` marker
  without executing the artifact path. This is file/report inventory only; it
  does not prove signing, notarization, installation, Finder launch, executable
  runtime behavior, live-device QA, marketplace review, malware scanning, or OS
  sandboxing.
- The live-device QA evidence item is stricter than generic JSON presence:
  `/release/evidence-status` validates schema/type, rejects `self_test_fixture`,
  checks the installed app path, expected bundle identifier, short/build
  version, requires UTC voice-check timestamps ending in `Z`, rejects
  future-dated generated reports, and requires completion to be at or after
  start. It also requires the observed transcript
  to match the spoken test phrase after trimming and the observed command text
  to match the expected command text after trimming, with
  `voice_command_observation.command_result_evidence_id` shaped as
  `task:<uuid>` or `audit:<uuid>` from live command/audit evidence. The
  `release-live-device-qa.sh --assert-complete` path rejects whitespace-only
  owner evidence values and reserves `JARVIS_QA_SELF_TEST_FIXTURE=true` for the
  script's internal fake-fixture self-test. Invalid or stale hand-written reports stay
  `invalid` and cannot clear `live_voice_loop` in evidence-aware readiness mode.
- Signed provenance, plugin-trust, and final bundle evidence items are also
  stricter than generic JSON presence: `/release/evidence-status` validates
  signed provenance version/bundle metadata, bundled core version,
  signing/notary/staple/Gatekeeper evidence fields, required signed-distribution flags, plugin-trust UTC review
  timestamp ordering, rejects future-dated generated reports and self-test
  review sources, validates final bundle version, requires SHA-256-shaped
  artifact/report digests including the signed provenance digest, verifies
  signed-provenance zip/pkg digests against the current artifact files in
  evidence-status, bundle, and doctor assertions, verifies final-bundle
  artifact/report paths and digests against the current configured files, and
  requires `validation_flags.local_signature_validation=true`.
- `release-evidence-doctor.sh --assert-complete` must stay aligned with that
  final-bundle semantic floor. It should reject minimal or hand-written final
  bundles that omit artifact/report paths, point at stale artifact/report paths,
  omit, malform, or stale SHA-256 digests, omit a UTC generation timestamp, use
  the wrong release version, set `validation_flags.local_signature_validation=false`,
  or pair the packaged bundled core with a stale `jarvis-cli.version` marker.
- The Swift shell also decodes `/release/readiness` through
  `ReleaseReadinessModel` and renders a Release tab with blocking manual gates,
  recommended commands, implemented proofs, pending features, the proof
  boundary, stale cached-readiness warning, and `/release/evidence-status`
  inventory. This remains inspection-only and does not perform signing,
  notarization, installation, Finder/LaunchServices validation, or live-device
  validation.
- `ConversationRuntime` supports bounded fake-model and provider-envelope
  planned first-party tool calls with schema validation, policy checks, approval
  stops, tool-result audit entries, and feedback of tool results into later
  model steps. Ollama-compatible and ChatGPT/OpenAI-compatible text responses
  can return a strict JSON envelope with `message`, `complete`, and
  `tool_requests`; ChatGPT/OpenAI-compatible responses can also return native
  OpenAI `tool_calls` for advertised first-party tool definitions. Plain text
  remains backward-compatible. This is not installed-plugin orchestration or
  broad third-party tool execution.
- Live local testing with Ollama `llama3.2` has proven the opt-in
  Ollama-compatible HTTP route can complete real model commands. The runtime
  derives the provider-visible first-party tool catalog from validated
  first-party manifests, exposes the same redacted catalog through
  `/tools/model` and `jarvis tools list`, advertises it as an Ollama JSON
  allowlist and ChatGPT/OpenAI-compatible native tool definitions, and rejects
  hallucinated provider plugin IDs/actions before policy checks or tool
  execution. Those recoverable validation misses emit `tool_request_rejected`
  audit evidence plus registered-tool guidance and are fed back to the next
  model step as `rejected` tool results. Malformed provider envelopes,
  including prose mixed with JSON `tool_requests`, still fail as redacted model
  errors instead of leaking tool-planning text as a normal answer.
- The registered model-tool contract is first-party only. Ollama envelope
  requests use `plugin_id` plus `action`; native ChatGPT/OpenAI-compatible tool
  names use `plugin__action`; both must map back to the same registered
  first-party catalog before any policy check or execution. Installed plugin
  registry records are inspectable and separately executable through explicit
  grants, but model-originated tool calls cannot target them and `/tools/model`
  excludes installed plugin paths, subprocess configuration, provenance hashes,
  audit payloads, memory values, and provider route context.
- The CLI interaction contract is now split between human and machine output:
  `jarvis command`, visible alias `jarvis ask`, `jarvis tools list`,
  `jarvis tasks list/get/audit`, `jarvis routes list/get`,
  `jarvis activity summary`, `jarvis release readiness`, and
  `jarvis release evidence-status` default to concise operator-readable text,
  while `jarvis release readiness --all-commands` prints the complete readable
  verification runbook and `--json` returns the exact IPC payload for scripts,
  diagnostics, task records, route evidence, readiness evidence, release
  evidence inventory, and E2E assertions. Human task inspection omits stored
  command text; use `--json` only when the exact task record is needed. Test
  harnesses may set
  `JARVIS_CLI_JSON=1` to keep legacy JSON parsing across command invocations.
  Read-only release/contract/plugin/tool fallback commands treat loopback
  `PermissionDenied` as transport-unavailable so restricted shells can still
  inspect conservative local metadata instead of failing with a raw OS error.
- Provider-envelope coverage includes
  `ollama_http_provider_parses_tool_request_envelope`,
  `chatgpt_http_provider_parses_tool_request_envelope`,
  `ollama_prompt_uses_request_supplied_first_party_tool_inventory`,
  `chatgpt_tools_use_request_supplied_first_party_tool_inventory`,
  `model_request_advertises_registered_first_party_tools_only`,
  `provider_tool_request_envelope_rejects_malformed_tool_requests_without_leaking_prompt`,
  `provider_originated_tool_request_executes_first_party_tool_and_feeds_result`,
  and the cross-process `serve_executes_ollama_provider_tool_request_envelope`
  E2E with an Ollama-compatible stub that asserts the advertised registered
  first-party catalog is a JSON allowlist and excludes invented browser plugin
  IDs. CLI smoke and local IPC E2E also cover `jarvis tools list` over
  `/tools/model`.
- Native ChatGPT/OpenAI-compatible tool-call coverage includes
  `chatgpt_http_provider_parses_native_tool_calls` and the cross-process
  `serve_executes_chatgpt_native_tool_call` E2E.
- Invalid provider-planned tool coverage includes
  `rejects_hallucinated_model_planned_plugin_with_registered_tool_guidance`,
  `rejects_hallucinated_model_planned_action_with_registered_tool_guidance`,
  and the cross-process
  `serve_rejects_ollama_hallucinated_tool_with_registered_tool_guidance` E2E.
  Malformed mixed-format provider output is covered by
  `provider_tool_request_envelope_rejects_mixed_prose_and_tool_json` and
  `serve_rejects_ollama_mixed_prose_tool_json_as_malformed_model_output`.
- Repository-backed IPC state exposes task, audit, model-route, and memory
  inspection routes, persists scheduler jobs, restores them at startup, and all
  IPC states expose `/plugins/manifests` for deterministic first-party plugin
  manifests plus `/tools/model` for the redacted first-party model-tool catalog.
  Repository-backed IPC also exposes `/plugins/installed` for metadata-only
  local plugin installation. Installed records are persisted with
  `execution_enabled: false` and `execution_grant: metadata_only` by default.
  Installed records also carry a local provenance snapshot with deterministic
  source-tree SHA-256/file count, manifest SHA-256, and, for
  `local_subprocess`, command SHA-256 hashes. Verification detects helper or
  resource drift under `source_path`, rejects symlinks and ambiguous path
  collisions, and keeps generated caches/artifacts out of the digest. This
  proves only local file integrity against the install snapshot, not malware
  safety or cryptographic publisher identity.
  Installed plugin run requests can perform contract-only dry runs that
  validate manifest/action/input schema and audit `side_effect_executed: false`
  without loading or executing plugin code. `local_subprocess` manifests can be
  explicitly enabled through `/plugins/installed/:id/execution` or
  `plugins enable-installed` with an action-scoped grant:
  `execution_grant: subprocess_stdio` for non-network actions, or
  `subprocess_stdio_network` for network-declaring actions, after
  `plugins verify-installed` confirms `matches_install_snapshot`. The runner
  treats the network grant as authority only for network-declaring actions, not
  a superset that can run non-network actions in mixed manifests; only
  currently granted action classes can run through the constrained
  subprocess-stdio JSON boundary.
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
  still does not prove marketplace approval, malware safety, or OS-level
  process/network sandbox completeness.
- Plugin actions that request the existing `network` permission must now
  declare `network_access.mode: declared_hosts` and exact plain-hostname
  `allowed_hosts`. Invalid host declarations fail manifest validation, and
  `/permissions/policy-review` emits `network_plugin_action` items for installed
  plugins with declared network access. Executable installed plugins with
  network-declaring actions fail closed unless enabled with
  `subprocess_stdio_network`; non-network actions fail closed while the
  installed plugin is enabled under that network grant. This is action-scoped
  runtime grant gating plus manifest governance and review evidence, not
  OS-level network sandboxing or host-level egress filtering.
- `./scripts/release-plugin-trust-qa.sh` keeps the plugin trust release gate
  explicit. `--check` validates repo-owned plugin trust prerequisites and
  prints the marketplace review, malware scan, signed publisher policy, OS
  process/network sandbox and host-level egress runbook. `--self-test` proves JSON report
  mechanics with fake validation flags and fake evidence notes only.
  `--assert-complete` writes an owner-recorded JSON report after every
  `JARVIS_PLUGIN_QA_*` flag is true and the owner/timestamp/evidence-note fields
  are populated. Host-level egress evidence must also include the reviewed
  policy/profile label, ordered UTC egress validation timestamp, denied
  undeclared-host fixture note, and declared-host allow fixture note. The review
  timestamps must be UTC `Z` values, the completed timestamp must be greater
  than or equal to the started timestamp, and the completed timestamp must not
  be later than report generation. `--write-template
  target/release-plugin-trust-qa.env` generates a sourceable checklist with all
  plugin trust validation flags defaulted to `false` and all evidence fields
  blank. `/release/readiness` and `jarvis release readiness --all-commands`
  include the template-backed source command for `--assert-complete` before the
  long inline owner-flag example. This is manual external release evidence, not
  repo-local proof of those systems.
- `./scripts/release-evidence-bundle.sh` is the final release evidence
  manifest gate. `--check` prints the required signed distribution artifact
  paths, live-device QA report, plugin-trust QA report, and owner validation
  flags. `--check`, `release-evidence-doctor.sh`, and `/release/evidence-status`
  are presence/JSON inventory surfaces only; they do not validate Developer ID
  signing, notarization, stapling, installation, live-device QA, plugin-trust QA,
  owner assertions, or final bundle creation. `--self-test` uses fake
  artifacts/reports to prove bundle mechanics only. The `--check` output points
  operators to `--write-template`, and `--write-template`
  generates a sourceable final-bundle environment template whose
  `JARVIS_EVIDENCE_*` validation flags default to `false`, so operators record
  external checks explicitly before any final bundle claim. `/release/readiness`
  and `jarvis release readiness --all-commands` include the template command and
  the template-backed source command for `--bundle` before the
  owner-flagged `--bundle` command so operators do not have to reconstruct the
  final evidence environment by hand. `--bundle` writes
  `target/release-evidence-bundle.json` after referenced artifacts/reports exist,
  every `JARVIS_EVIDENCE_*` flag is true, and local artifact checks validate the
  app signature, app stapling ticket, installer signature, installer stapling
  ticket, and app zip payload. Production bundles must keep local signature
  validation enabled; the script parses every required live-device and
  plugin-trust report flag, requires non-empty owner-recorded evidence fields in
  both QA reports, requires plugin-trust `generated_at`, `review_started_at`,
  and `review_completed_at` to be UTC with
  `review_started_at <= review_completed_at <= generated_at`, requires the
  live-device QA report's app bundle identifier/version/build metadata to match
  the expected release, and records SHA-256 digests for the distribution zip,
  installer package, live-device QA report, and plugin-trust QA report before
  writing the bundle manifest.
- `./scripts/release-evidence-doctor.sh` inventories release evidence readiness
  before final bundling. `--check` reports present, missing, or invalid
  signed-artifact, live-device QA, plugin-trust QA, and final bundle evidence
  without failing the default local gate, checks the bundled core version marker
  beside the packaged executable, tells operators to rerun
  `./scripts/package-distribution.sh --unsigned-launch-check` or the signed
  packaging lane when that marker is missing or stale, and prints the next signing,
  live-device template/assertion, plugin-trust template/assertion, and final
  evidence-bundle template/bundle commands when evidence is missing.
  `/release/readiness` and `jarvis release readiness --all-commands` include
  `./scripts/release-evidence-doctor.sh --assert-complete` as the final
  inventory assertion after the bundle command.
  `--self-test` uses fake artifacts/reports to prove the inventory mechanics
  and the next-step guidance only. Its complete path enforces the same
  plugin-trust UTC timestamp order as the bundle path. A
  complete doctor run is diagnostic status, not proof that signing,
  notarization, stapling, installation, or external validation happened.
- `jarvis release readiness --all-commands` is ordered for release execution:
  local gates, unsigned distribution launch check, signed/notarized packaging,
  live-device QA, plugin-trust QA, final evidence bundle generation,
  evidence-doctor assertion, and then the external evidence-mode readiness
  check.
- The structured release evidence status endpoint mirrors the doctor inventory
  for app/installer artifacts and JSON reports, including required owner-recorded
  live-device and plugin-trust evidence fields plus app bundle `Info.plist`
  metadata checks, live-device bundle/version and timestamp semantic checks,
  plugin-trust review timestamp checks, and final bundle version/SHA/local-signature
  checks, so the CLI and Swift Release tab can show missing or invalid release
  evidence without parsing script text.
- Enabled `local_subprocess` plugins run with an environment boundary: Jarvis
  clears the inherited app/core process environment before spawn and provides
  only a deterministic `PATH` plus `JARVIS_PLUGIN_ID`,
  `JARVIS_PLUGIN_ACTION`, and `JARVIS_PLUGIN_SOURCE_PATH`. Rust unit coverage
  and CLI IPC E2E assert that a secret inherited by the core process is not
  visible inside the plugin subprocess.
- Enabled `local_subprocess` plugin output is bounded before parsing or audit:
  stdout is capped at 1 MiB, stderr is capped at 256 KiB, and either stream
  exceeding its cap kills the child and returns a fail-closed plugin error.
  Normal JSON stdout and bounded `jarvis_progress` stderr lines still execute
  and parse under the same runner. CLI IPC E2E now covers stdout and stderr
  over-limit failures through the installed-plugin run endpoint, including
  failed audit evidence.
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
  actions, active scheduler triggers, unreviewed memory items, and deleted
  sensitive memory retained in local storage into explicit severity-ranked
  review items. Memory review and retention-review items include category/key
  and sensitivity only; memory values are redacted from policy review. The Swift
  Approval Center renders this summary alongside grant history. It is
  inspection-only and does not execute, enable plugin side effects, or
  autonomously rewrite/delete memory.
- Approved first-party approval records can be explicitly executed once through
  `/approvals/:id/execute` or `jarvis approvals execute <approval-id>`.
  Approve/deny remains side-effect-free; execution replays only the original
  first-party plugin command, verifies the current action and scope contract
  against the approval record, applies an approval grant for that replay, moves
  the task out of `waiting_for_approval` on completion, and records
  `approval_executed` plus plugin completion audit evidence with
  `side_effect_executed: true`.
- The Swift Approval Center loads pending approvals for grant/deny controls and
  approved-unexecuted approvals for a Run Approved action when the IPC contract
  exposes `/approvals/:id/execute`. It checks the approval task audit for
  `approval_executed` and hides records that already have execution evidence,
  so a refresh does not invite duplicate approved replay.
- The Swift Plugin tab decodes `/plugins/installed` registry records and shows
  installed plugin source path, execution grant, provenance integrity status,
  origin-review state, and executable/not-executable status alongside
  first-party manifests. This surface is read-only and degrades to a warning
  while keeping first-party manifests visible when the repository-backed
  installed registry endpoint is unavailable.
- The CLI has matching `release readiness`, `release evidence-status`,
  `command`/`ask`, `tools`, `tasks`, `memory`, `scheduler`, `diagnostics`, and
  `plugins` subcommands, including
  `plugins install`,
  `plugins installed`, `plugins installed-get`, `plugins enable-installed`, `plugins
  verify-installed`, `plugins verify-publisher`,
  `plugins verify-publisher-signature`, `plugins disable-installed`, and
  `plugins run-installed` for disabled-by-default local manifests, auditable
  publisher-origin review, trusted-key signature verification, and explicit
  subprocess execution.
- A local packaged app release smoke exists, and installed plugin execution now
  has a constrained local subprocess proof. Developer ID signing,
  notarization, installer validation, App Store distribution, owner-recorded
  live-device voice-loop validation, broader plugin
  marketplace/WASM/OS-network sandboxing, plugin malware analysis, and broader
  production operations are not yet complete in this worktree.
  The SwiftUI shell scaffold and IPC client live under `apps/mac`, including a
  command transcript, activity/audit panel, approval decision and approved-run
  controls,
  management tabs, permission grant-history summary, degraded-mode handling,
  typed transcript staging, adapter-backed voice input/output controls,
  final-transcript handoff into the text command path, and a core supervisor
  abstraction for configured or bundled local core binaries.
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
  above the item list. `/contract.safe_inspection_paths` includes this
  aggregate classification route but intentionally excludes raw `/memory` and
  `/memory/:id` because those explicit memory-management routes return stored
  values.
- Diagnostics export now includes aggregate active, unreviewed, and sensitive
  memory counts when repository backing is enabled. It still omits memory
  values, and memory policy review similarly redacts values while surfacing
  unreviewed memory plus deleted sensitive retained memory for user review.
- The Swift shell has a Keychain-backed launch credential boundary for
  app-supervised model provider secrets. `JarvisCoreCredentialProvider` reads
  known credentials such as the OpenAI API key from Keychain and injects only
  missing process environment values when launching the bundled core; explicit
  environment values still win, and the provider does not auto-enable ChatGPT.
- The Swift shell now exposes production-facing scaffold tabs for approval
  evidence, runs/audit, scheduler create/inspect/cancel, redacted diagnostics,
  release readiness, and voice state. Voice supports typed transcript staging,
  manual submit, and opt-in final-transcript auto-submit into the same text
  command path. The scaffold now models
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
  Scheduler-originated first-party plugin calls are submitted as proactive
  calls, so actions must opt in with manifest `proactive` plus
  `proactive_run` permission. Non-opted-in scheduled plugin actions fail closed,
  record redacted `plugin_execution_blocked` evidence, and do not execute side
  effects.
  `jarvis scheduler recover-stale` and `/scheduler/recover-stale` provide
  explicit operator recovery for persisted stale `Running` jobs after a crash
  or killed process. Recovery marks matching jobs failed, returns diagnostic
  scheduler job fields without commands, and records
  `scheduler_stale_running_recovered` with command redaction evidence. This is
  explicit recovery unless `jarvis serve --scheduler-recover-stale-on-startup`
  is provided. Startup recovery runs the same stale recovery path before the
  server accepts IPC traffic, marks the audit payload with `automatic_recovery:
  true`, and remains bounded by age/limit flags.
  Release-readiness feature metadata should describe this as explicit plus
  opt-in startup recovery, with no default background recovery or distributed
  lease claim.
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
  jarvis-cli -- smoke`, `./scripts/release-operator-qa-smoke.sh`, `cargo
  package --workspace --allow-dirty`,
  `./scripts/package-distribution.sh --unsigned-launch-check`,
  `./scripts/release-live-device-qa.sh --check`,
  `./scripts/release-live-device-qa.sh --self-test`,
  `./scripts/release-plugin-trust-qa.sh --check`,
  `./scripts/release-plugin-trust-qa.sh --self-test`,
  `./scripts/release-evidence-bundle.sh --check`,
  `./scripts/release-evidence-bundle.sh --self-test`,
  `./scripts/release-evidence-doctor.sh --check`,
  `./scripts/release-evidence-doctor.sh --self-test`, `swift test
  --package-path apps/mac`, and `swift build --package-path apps/mac`.
  It also runs `./scripts/storage-migration-backup-smoke.sh` so file-backed
  migration backup/recovery and representative schema v1-v8 fixture
  preservation stay part of the default local release evidence.
- Local-model proof now includes stubbed provider-envelope E2E plus live
  Ollama route viability observed during manual testing. The proof is still a
  local runtime boundary claim, not a finished conversational assistant claim:
  model-specific tool discipline can vary, and Jarvis relies on the runtime
  advertised inventory plus fail-closed validation for safety.
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
  signing/notarization/stapling evidence, clean-profile install and Finder
  validation, owner-recorded live voice/audio validation, marketplace/plugin
  trust QA, malware analysis, OS-level sandbox/egress evidence where
  marketplace claims are made, final evidence-bundle archival, and manual
  clean-profile release QA.
- Do not describe Jarvis as production assistant ready based only on the Rust
  and Swift local gates. The stronger claim requires signed/notarized
  distribution evidence, clean-profile install and Finder validation,
  owner-recorded live voice/audio QA, plugin-trust QA, and a final archived
  evidence bundle.
- `./scripts/packaged-supervision-proof.sh` is local packaged-layout evidence:
  it builds the Rust CLI, copies it into
  `Jarvis.app/Contents/Resources/bin/jarvis-cli`, runs Swift supervisor tests
  against that executable, starts the copied binary with repository-backed
  state, and verifies health, command, audit, diagnostics, emergency pause,
  blocked command, pause status, and resume over IPC. It is not signed,
  notarized, clean-profile packaged app release evidence.
- `./scripts/release-operator-qa-smoke.sh` is local operator-facing QA
  evidence: it starts a repository-backed loopback core with an isolated
  SQLite database, verifies command, audit, model-route, memory
  create/update/review/delete/restore, scheduler attention/run-due, activity,
  permission review, diagnostics, emergency pause, release readiness, and
  restart recovery paths, then removes the temporary state. It is not
  clean-profile installed-app QA, Finder/LaunchServices validation, live
  microphone/Speech validation, spoken transcript handoff, live audio-output
  validation, live OS notification validation, or Developer ID
  signing/notarization evidence.
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
  core model can request authorization, build due, failed, and
  emergency-pause-blocked notification requests, suppress duplicate deliveries
  for the same attention item, and fail closed when permission is denied. This
  is not a substitute for manual clean-profile
  macOS notification prompt and delivery validation.
- `./scripts/package-distribution.sh` is the repo-owned distribution packaging
  lane. Its `--check` mode is credential-free and validates local tools plus
  entitlements. Its `--unsigned-structure-check` mode builds release Rust/Swift
  artifacts, assembles `target/distribution/Jarvis.app`, optionally ad-hoc signs
  when `codesign` is available, creates an unsigned `/Applications` installer
  package, and inspects the package payload for the app executable, bundled
  core, and `Info.plist`. Its `--unsigned-launch-check` mode is part of
  `./scripts/release-local.sh`, launches the release-built app executable with
  an isolated temporary HOME, verifies the bundled core over loopback IPC, and
  checks command, audit, diagnostics, pause/block/resume, and SQLite state
  through the release app layout. The CLI exposes `jarvis --version`, and the
  packaging/evidence scripts require the bundled `jarvis-cli --version` output
  to match the expected release version before local artifact evidence can pass.
  Full mode requires the owner's Developer ID
  Application, Developer ID Installer, and notarytool credentials; signs with
  hardened runtime and microphone entitlements; notarizes and staples the app
  zip; then creates, signs, notarizes, and staples a `/Applications` installer
  package. `./scripts/release-version-consistency.sh --check` derives the
  release version from Rust package metadata and keeps package, live QA,
  evidence bundle, and evidence doctor defaults aligned with the CLI/core crate
  versions in the default local release gate. The unsigned structure and launch
  checks still do not prove Developer
  ID signing, notarization, stapling, installation, Finder launch, live
  microphone/Speech validation, spoken transcript handoff, App Store review,
  live audio-output validation, or manual QA.
- `./scripts/release-live-device-qa.sh --check` is part of
  `./scripts/release-local.sh`. It validates repo-owned live QA preconditions
  and prints the manual clean-profile install, Finder/LaunchServices,
  microphone/Speech permission prompts, spoken transcript handoff into the
  command path, live audio-output, notification, restart, and release-QA runbook.
  Its `--assert-complete` mode requires an installed app plus explicit
  `JARVIS_QA_*` owner flags, including
  `JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true`, then writes a JSON evidence
  report to `JARVIS_QA_REPORT_PATH` or
  `target/release-live-device-qa-report.json`. The report records installed-app
  metadata, voice-loop evidence fields, owner-recorded live voice evidence
  fields for owner/device/profile/non-future timestamps/notes, structured spoken-command
  observation fields with observed transcript matching the spoken test phrase
  and expected command text matching observed command text, validation flags,
  schema identity, UTC report generation timestamp, and proof boundary.
  This standardizes manual evidence only; `--check` does not prove live device
  behavior, and the report remains an owner assertion. When the
  release operator explicitly enables evidence-aware readiness, this report can
  support the narrow claim that the live voice loop was validated for that
  release candidate, not a generalized claim that voice is validated for every
  device or future release. Use
  `./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env`
  to generate a sourceable checklist for all required `JARVIS_QA_*` fields.
  `--self-test` uses a fake app fixture to validate assertion/report mechanics
  in the local release gate without claiming live device validation.
- It is fair to describe the current repo as a Rust foundation with tested
  scaffolding for IPC, storage, policy, routing, runtime, scheduler, plugin
  contracts, deterministic first-party plugin command execution, bounded
  fake-model and strict provider-envelope planned first-party tool orchestration,
  opt-in Ollama-compatible local HTTP provider behavior, opt-in
  ChatGPT/OpenAI-compatible provider behavior, CLI behavior, and a Swift
  command/management shell with supervisor abstraction, approval decisions,
  adapter-backed voice input/output controls, typed transcript handoff, and
  opt-in final-transcript auto-submit proof when the local gate passes. Live
  microphone/Speech capture, spoken transcript handoff, and live audio-output
  remain pending until owner-recorded live-device QA evidence is explicitly
  enabled.
- Do not claim autonomous external communication, smart-home control, or
  third-party plugin marketplace readiness for v1.
- Keep public-facing claims scoped to tested local behavior.

## Workflow

- Work in isolated worktrees and branches for reviewable slices.
- Use topic branches and PRs for production work. Treat older phase/worktree
  names as historical coordination context unless the branch is verified active
  in the current checkout.
- Historical phase-3 slices included model-route persistence, plugin-subprocess
  execution, voice adapter controls, packaged app smoke, permission grants UX,
  and docs architecture alignment. Verify current status from
  `/release/readiness` and the checkout before treating any old worktree name
  as active.
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
  until distribution-grade app/installer signing, notarization/stapling,
  clean-profile install/Finder validation, owner-recorded live voice/audio QA,
  marketplace/plugin-trust plus OS-level sandbox/egress evidence, final
  evidence bundle archival, and manual release QA gates exist.
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
- Storage migration coverage includes a representative schema v1-v8 fixture
  matrix that preserves task, audit, emergency-pause, memory, scheduler,
  approval, installed-plugin, plugin-provenance, and route records through the
  current schema. This is repo-owned migration proof, not installer upgrade or
  Finder/LaunchServices validation.
- `cargo run -p jarvis-cli -- smoke` now covers baseline command/pause smoke,
  plugin manifest listing, and repository-backed task, model-route, explicit
  memory-management paths, diagnostics redaction, and repository-backed
  scheduler/job state surfaces.

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
  output schemas, run with the declared timeout, clear inherited environment
  variables, and emit audit evidence including whether the subprocess started.
  These grants are action-scoped; a network grant does not execute plain
  non-network actions in mixed manifests.
  Subprocess stderr may contain bounded progress frames, but raw stderr remains
  redacted from response and audit payloads. Any broader executable path or
  real-time plugin progress
  stream needs a stronger OS-level process/network sandbox or equivalent host isolation boundary,
  explicit grant state beyond `metadata_only`, policy checks,
  timeout/cancellation behavior, and E2E audit coverage.
