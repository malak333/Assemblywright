# Jarvis Project Facts

These notes capture durable facts for future agents working on this repository.

## Repository And Scope

- The repository is public at `https://github.com/malak333/Jarvis`.
- Production implementation work should assume public-repo hygiene: no secrets,
  no private-source material, no hidden readiness claims, and release evidence
  that can be reviewed from the branch/PR.
- Public PR/release evidence includes `.github/workflows/release-local.yml`,
  which runs `./scripts/release-local.sh` on macOS for pull requests, pushes to
  `main`, and manual dispatch. `./scripts/release-ci-workflow-smoke.sh` is part
  of the local gate and validates that the workflow still points at the
  canonical release-local script. The workflow uses `actions/checkout@v5`, whose
  official action metadata runs on Node 24, to avoid Node 20 deprecation drift
  in the public release gate. This is CI evidence for repo-owned local
  verification only, not Developer ID signing, notarization, clean-profile
  install, Finder launch, live-device QA, or plugin marketplace trust evidence.
  The same boundary is exposed as the `release_ci_gate` feature in `/contract`
  and release readiness.
- The product direction is a local-first macOS assistant foundation, legally
  distinct from Marvel/JARVIS branding and assets.
- The current repo contains a Rust workspace with `jarvis-core` and
  `jarvis-cli`, plus a Swift shell under `apps/mac` with management tabs, core
  supervision, voice input/output adapters, release/evidence-status
  inspection, scheduler notifications, Keychain credential launch injection,
  and packaged-smoke support.
- Implemented `jarvis-core` surfaces include shared task/audit/safety types,
  an Axum loopback IPC server, runtime-backed command execution with
  `FakeLocalModel` by default, an opt-in Ollama-compatible local HTTP provider,
  or an opt-in ChatGPT/OpenAI-compatible HTTP provider behind explicit
  env/config, sensitivity, redaction, and audit guardrails, emergency-pause
  state, inspectable scheduler state, scheduler recovery/attention, a
  conversation runtime with SQLite task/audit persistence hooks, local-first
  model routing policy, SQLite repository migrations, memory item persistence,
  append-only audit table triggers, release readiness/evidence-status,
  approval execution and permission-center review, bounded activity
  events/progress, installed-plugin metadata/provenance/grants, diagnostics,
  plugin manifest validation, and deterministic first-party test plugins.
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
  from recent audit evidence while redacting raw stderr. Installed-plugin run,
  audit, and activity-summary evidence also use the redacted provenance view:
  local source paths, manifest paths, subprocess command paths, and provenance
  hashes stay out of those operator surfaces. This is bounded, audit-backed
  plugin progress evidence, not per-token or unbounded real-time plugin UI
  streaming.
- Installed subprocess audit evidence distinguishes process execution from OS
  sandbox enforcement. A completed local subprocess can report
  `subprocess_started: true`, but the current runner reports
  `os_sandbox_enforced: false` and an explicit sandbox boundary because it
  validates manifest/provenance/grants and clears inherited environment
  variables without enforcing an OS sandbox or host-level egress policy. Those
  external controls remain part of plugin-trust QA evidence.
- `/contract` includes a `compatibility` block with supported version range,
  additive-change, deprecation, removed/deprecated endpoint, and client
  requirement policy, plus a `features` list with stable keys, status, proof,
  and boundary fields so Swift and release docs can distinguish implemented
  repo-owned surfaces from manual or target production claims without scraping
  prose. `jarvis contract` emits JSON by default and also accepts `--json` so
  scripts can use the same explicit machine-output flag pattern as other
  inspection commands.
- `/release/readiness` and `jarvis release readiness` derive a conservative
  read-only release summary from contract feature metadata, release-checklist
  blockers, and explicitly enabled release evidence status. Default readiness
  treats standard `target/` evidence files as inventory only; with
  `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external` on the running core,
  readiness can compute `production_ready: true` only when every required
  `/release/evidence-status` item is present, no missing or invalid evidence
  remains, and evidence-cleared features leave no pending readiness features.
  This remains validated owner-recorded release evidence, not Jarvis-performed
  signing, notarization, stapling, live-device QA, plugin trust QA, or manual
  release QA. The CLI command prefers the IPC endpoint when it is running and
  falls back to the same local `IpcState` readiness summary when the server is
  unavailable, so operator triage does not require a prestarted core. When an
  IPC core is already running, setting the external-mode env var only on the CLI
  process does not clear readiness; the core must be started or restarted with
  that env var after owner evidence is complete.
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
  without executing the artifact path. This is read-only file/report inventory
  plus semantic validation. It does not perform signing, notarization,
  stapling, installation, Finder launch, executable runtime behavior,
  live-device QA, marketplace review, malware scanning, OS sandboxing, or
  host-level egress enforcement.
- The live-device QA evidence item is stricter than generic JSON presence:
  `/release/evidence-status` validates schema/type, rejects `self_test_fixture`,
  checks the installed app path, expected bundle identifier, short/build
  version, requires UTC voice-check timestamps ending in `Z`, rejects
  future-dated generated reports, and requires completion to be at or after
  start. It also requires the observed transcript
  to match the spoken test phrase after trimming and the observed command text
  to match the expected command text after trimming, with
  `voice_command_observation.command_result_evidence_id` shaped as
  `task:<uuid>` or `audit:<uuid>` from live command/audit evidence. When the
  check runs through CLI/IPC evidence-status, that ID must resolve through
  repository-backed IPC state to an existing task row or a task-associated audit
  row; fallback/no-server CLI evidence-status fails closed instead of accepting
  shape-only IDs. The live-device and bundle scripts keep shape preflights, and
  `release-evidence-doctor.sh --assert-complete` then delegates to
  `jarvis release evidence-status --json`, optionally through
  `JARVIS_EVIDENCE_STATUS_ENDPOINT`, so final doctor completion cannot accept
  unresolved task/audit evidence. It now
  also requires a `bundled_core` block that binds the installed
  `Contents/Resources/bin/jarvis-cli` path, `jarvis <version>` output, and
  SHA-256 digest to the same live-device report, all live-device
  `validation_flags` and `voice_loop` flags set to true, non-empty
  microphone/Speech usage descriptions, non-empty audio output device label,
  and non-voice owner notes for clean-profile, Finder launch, notification,
  restart, and manual QA with an ordered UTC notification timestamp. The
  `release-live-device-qa.sh --assert-complete` path rejects whitespace-only
  and placeholder owner evidence-note values such as `TODO`, `pending`, `n/a`,
  `fixture`, and `self-test fixture`; `/release/evidence-status` enforces the
  same non-placeholder live-device evidence-note checks before that report can
  clear readiness, and
  `JARVIS_QA_SELF_TEST_FIXTURE=true` is reserved for the
  script's internal fake-fixture self-test. Invalid or stale hand-written reports stay
  `invalid` and cannot clear `live_voice_loop` in evidence-aware readiness mode.
- Signed provenance, plugin-trust, and final bundle evidence items are also
  stricter than generic JSON presence: `/release/evidence-status` validates
  signed provenance version/bundle metadata, bundled core path/version/SHA-256
  binding, Apple-tool-derived signing/notary/staple/Gatekeeper evidence fields
  from `codesign`, `pkgutil --check-signature`, `xcrun notarytool`,
  `xcrun stapler`, and `spctl`, required signed-distribution flags, plugin-trust UTC review
  timestamp ordering, rejects future-dated generated reports and any
  plugin-trust `review_source` other than `owner-asserted-manual-review`,
  validates final bundle version, requires SHA-256-shaped
  artifact/report digests including the signed provenance digest, verifies
  signed-provenance zip/pkg/core digests against the current artifact files in
  evidence-status, bundle, and doctor assertions, verifies final-bundle
  schema/type identity, artifact/report paths, and digests against the current
  configured files, revalidates the signed-provenance, live-device QA, and
  plugin-trust QA child reports referenced by the final bundle, and requires
  `validation_flags.local_signature_validation=true`. Final bundle owner
  evidence also requires non-placeholder owner evidence notes plus
  `reports_archive_uri` to be URI-shaped and durable; blank values, missing
  schemes, placeholders, examples, fixtures, and self-test archive paths are
  invalid production evidence.
- `release-evidence-doctor.sh --assert-complete` must stay aligned with that
  final-bundle semantic floor. It should reject minimal or hand-written final
  bundles that omit artifact/report paths, point at stale artifact/report paths,
  omit, malform, or stale SHA-256 digests, omit a UTC generation timestamp, use
  the wrong release version, reference semantically invalid signed-provenance,
  live-device QA, or plugin-trust QA child reports even when their digests
  match, set `validation_flags.local_signature_validation=false`,
  or pair the packaged bundled core with a stale `jarvis-cli.version` marker.
- The Swift shell also decodes `/release/readiness` through
  `ReleaseReadinessModel` and renders a Release tab with blocking manual gates,
  recommended commands, implemented proofs, pending features, the proof
  boundary, stale cached-readiness warning, and `/release/evidence-status`
  inventory. Present presence-only evidence rows show the caveat on the status
  line. Its effective production-ready display must remain fail-closed unless
  readiness is true, evidence status is complete, every evidence item is
  present, and the refresh is not stale or failed. This remains inspection-only
  and does not perform signing, notarization, stapling, installation,
  Finder/LaunchServices validation, or live-device validation.
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
- Installed-plugin safe inspection is redacted by default:
  `/plugins/installed` and `/plugins/installed/:id` omit local `source_path`
  values, manifest paths, subprocess command paths, publisher-signature
  material, and provenance SHA-256 hashes. They keep execution grant,
  integrity status, publisher-origin review state, action metadata, install
  timestamp, and explicit redaction markers for operator review. Mutating
  install, verification, enablement, and run paths remain separate operational
  surfaces.
- The CLI interaction contract is now split between human and machine output:
  `jarvis command`, visible alias `jarvis ask`, `jarvis plugins list/get`,
  `jarvis tools list`, `jarvis tasks list/get/audit`,
  `jarvis routes list/get`, `jarvis activity summary`,
  `jarvis release readiness`, and `jarvis release evidence-status` default to
  concise operator-readable text,
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
  `jarvis health` and strict IPC commands such as `jarvis command`,
  pause/resume, scheduler, task/audit/activity/route, memory, approval,
  diagnostics, installed-plugin, and permission-center operations exit
  non-zero when the server is unavailable, but the failure is
  operator-readable and points to `jarvis serve`, `jarvis smoke`, and the
  read-only fallback inspection commands instead of surfacing only a raw
  connection error.
- Release command help text is part of the operator contract.
  `jarvis release evidence-status --help` must describe default
  operator-readable output, `--json` for exact payloads, file/report inventory
  plus semantic validation, owner-asserted plugin-trust review source,
  host-egress evidence fields, child report validity, final-bundle archive URI
  validation, and final-bundle local signature-validation status without
  implying Jarvis performs signing, notarization, live-device QA, marketplace
  review, malware scanning, OS sandboxing, or host-level egress enforcement. CLI E2E covers this with
  `release_help_surfaces_current_evidence_boundaries`.
- `/contract` feature metadata is also release-boundary evidence. The
  `release_evidence_status` proof should name repository-backed live
  command-result evidence, plugin-trust owner-source and host-egress fields,
  final-bundle archive-URI validation, and final-bundle child-report semantic
  revalidation. The `release_evidence_bundle` proof should name live-device
  command observation, plugin-trust review source and host-egress fields,
  durable reports archive URI evidence, SHA-256-bound manifest entries, and
  doctor/status revalidation of child reports. CLI E2E asserts those strings so
  clients do not infer weaker release evidence semantics from `/contract`.
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
  IDs. CLI smoke and local IPC E2E also cover readable `jarvis plugins list`
  over `/plugins/manifests` and `jarvis tools list` over `/tools/model`.
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
  `allowed_hosts`. Invalid host declarations, including schemes, wildcards,
  paths, ports, whitespace, IP literals, and non-ASCII hostnames, fail manifest
  validation, and `/permissions/policy-review` emits `network_plugin_action` items for installed
  plugins with declared network access. Executable installed plugins with
  network-declaring actions fail closed unless enabled with
  `subprocess_stdio_network`; non-network actions fail closed while the
  installed plugin is enabled under that network grant. This is action-scoped
  runtime grant gating plus manifest governance and review evidence, not
  OS-level network sandboxing or host-level egress filtering.
- Installed local subprocess plugin run audits include the requested action's
  manifest-declared `action_network_allowed_hosts` alongside
  `action_requires_network_grant`, while preserving the explicit
  `os_sandbox_enforced: false` and host-egress proof boundary. This makes
  network targets reviewable without claiming repo-local egress enforcement.
- `./scripts/release-plugin-trust-qa.sh` keeps the plugin trust release gate
  explicit. `--check` validates repo-owned plugin trust prerequisites and
  prints the marketplace review, malware scan, signed publisher policy, OS
  process/network sandbox and host-level egress runbook. `--self-test` proves JSON report
  mechanics with fake validation flags and fake evidence notes only.
  `--assert-complete` writes an owner-recorded JSON report after every
  `JARVIS_PLUGIN_QA_*` flag is true and the owner/timestamp/evidence-note fields
  are populated. The accepted report identity is `schema_version: 1` with
  `evidence_type: owner_recorded_plugin_trust_qa` and `self_test_fixture: false`;
  accepted operator reports must also use
  `review_source: owner-asserted-manual-review`. Doctor/status gates reject
  stale, self-test, misidentified, or non-owner-source plugin-trust report
  shapes, and they reject placeholder evidence values such as `TODO`, `pending`,
  `n/a`, or self-test/fixture text in owner-recorded evidence fields. Host-level egress evidence
  must also include the reviewed policy/profile label, ordered UTC egress
  validation timestamp, denied undeclared-host fixture note, and declared-host
  allow fixture note. The review timestamps must be UTC `Z` values, the
  completed timestamp must be greater than or equal to the started timestamp,
  and the completed timestamp must not be later than report generation.
  `--write-template
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
  are read-only inventory plus semantic-validation surfaces: they do not
  perform external validation, but they reject stale or weak signed-provenance,
  live-device, plugin-trust, and final-bundle reports before evidence-aware
  readiness can use them. They do not validate Developer ID signing,
  notarization, stapling, installation, live-device QA, plugin-trust QA,
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
  ticket, and app zip payload through Apple-tool-derived validation. Production bundles must keep local signature
validation enabled; the script parses every required live-device and
plugin-trust report flag, requires non-empty and non-placeholder
owner-recorded evidence-note fields in both QA reports and the final bundle,
requires plugin-trust `generated_at`, `review_started_at`,
  and `review_completed_at` to be UTC with
  `review_started_at <= review_completed_at <= generated_at`, requires the
  plugin-trust `review_source` to be `owner-asserted-manual-review`, requires the
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
  plugin-trust review timestamp and owner-review-source checks, final bundle
  version/SHA/archive-URI/local-signature checks, and repository-backed live
  command evidence resolution, so the CLI and Swift Release tab can show
  present, missing, or invalid release evidence without parsing script text.
- Release evidence status rejects false live-device validation flags, false
  live voice-loop flags, false plugin-trust validation flags, and false final
  evidence-bundle validation flags; CLI E2E now covers those semantics and
  proves invalid live-device QA keeps `live_voice_loop` pending even when the
  rest of the release evidence fixture is complete.
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
- The Swift Plugin tab decodes `/plugins/installed` registry records through
  the same redacted inspection contract used by the CLI and IPC surfaces. It
  shows execution grant, provenance integrity status, origin-review state,
  action metadata, executable/not-executable status, and redaction markers
  alongside first-party manifests, while local paths, subprocess command paths,
  signature material, and provenance hashes stay hidden. This surface is
  read-only and degrades to a warning while keeping first-party manifests
  visible when the repository-backed installed registry endpoint is unavailable.
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
  live-device voice-loop validation, broader plugin marketplace/WASM isolation,
  OS-level process/network sandboxing, host-level egress filtering, plugin
  malware analysis, and broader production operations are still external/manual
  gates.
  The SwiftUI shell and IPC client live under `apps/mac`, including a
  command transcript, activity/audit panel, approval decision and approved-run
  controls,
  management tabs, permission grant-history summary, degraded-mode handling,
  typed transcript staging, adapter-backed voice input/output controls,
  final-transcript handoff into the text command path, and a core supervisor
  abstraction for configured or bundled local core binaries.
- The architecture docs must preserve two diagrams: the current implemented
  Rust/Swift surfaces and the end-goal production architecture. Keep the
  current-vs-target phase table aligned with code before answering readiness
  questions, and show release evidence flow changes such as repository-backed
  command-result evidence validation in both current and target diagrams.
- The active architecture docs should also describe the current production
  sweep structure, but that workflow context must remain separate from
  readiness proof.
- The Swift shell has a core supervisor abstraction, management tabs,
  release/evidence-status inspection, scheduler notification controls, Keychain
  launch credential injection, adapter-backed voice input/output controls, and
  local packaged-app smoke evidence. It is not a Developer ID signed or
  notarized packaged app, and it still needs clean-profile Finder/LaunchServices
  and live-device validation before production app claims.
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
- The Swift shell exposes production-facing management tabs for approval
  evidence, runs/audit, scheduler create/inspect/cancel/run-due/recover-stale,
  redacted diagnostics, release readiness, and voice state. Voice supports typed transcript staging,
  manual submit, and opt-in final-transcript auto-submit into the same text
  command path. The voice model handles interruption, resume/cancel,
  unavailable, and degraded typed-fallback states,
  owns a protocol-backed macOS Speech/AVFoundation adapter model from the
  SwiftUI Voice tab, and exposes permission request, start/stop capture, and
  interrupt controls. Production builds of the Voice tab do not expose manual
  state override buttons that can forge release-visible voice status, and the
  auto-submit toggle is disabled with an explicit reason when no submit handler,
  unavailable voice capture, or busy command submission prevents real
  auto-submit. The Voice tab also owns a protocol-backed AVFoundation
  speech-output adapter with preview, stop, interrupt, and natural completion
  handling. Swift tests cover both adapter boundaries with fakes, including
  speech-output completion returning the model to idle, utterance identity
  protection so stale completion/cancel callbacks cannot mark newer playback
  idle, and auto-submit availability reasons, and do not require live microphone
  access or live audio output. The app still must not claim real
  voice parity until entitlements, clean-profile permission prompts, live
  microphone capture, spoken transcript handoff, live audio output,
  owner-recorded manual device validation, and repository-backed command-result
  evidence are complete for the release candidate.
- The scheduler is inspectable, cancellable, explicitly runnable through
  `scheduler run-due`, and opt-in runnable as a bounded background loop with
  `jarvis serve --scheduler-background`. Scheduler jobs are in-memory without
  repository backing and durable when the IPC state is started with
  `SqliteRepository`. The background loop uses the same audited run-due path,
  per-tick limit, deterministic due ordering, and fail-closed emergency-pause
  behavior as manual execution. Repository-backed IPC exposes
  `/scheduler/attention`, and the CLI exposes `jarvis scheduler attention`, as
  a redacted app handoff summary for due, running, failed, and
  emergency-pause-blocked scheduler jobs.
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
  Release-readiness feature metadata describes this as explicit plus opt-in
  startup recovery, with no default background recovery or distributed lease
  claim.
  The Swift Scheduler tab renders this summary above the job list and owns
  typed controls for bounded `/scheduler/run-due` and
  `/scheduler/recover-stale`, refreshing jobs and attention after each action
  and rendering concise last-action state without exposing scheduler command
  bodies. It also owns a protocol-backed notification model plus macOS
  `UserNotifications` adapter controls for due, failed, and
  emergency-pause-blocked attention items. Swift tests use fake IPC and
  notification adapters to cover run/recovery routing, model refresh,
  authorization, delivery, duplicate suppression, and denied-permission
  fail-closed behavior. Broader production trigger policy and live OS
  notification validation remain target architecture.

## Proof Boundaries

- Local release proof currently means `./scripts/release-local.sh`, which wraps
  `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo
  test --workspace -- --ignored`, `cargo build --workspace`, `cargo run -p
  jarvis-cli -- smoke`, `./scripts/release-operator-qa-smoke.sh`,
  workspace package tarball creation, packaged CLI verification against the
  freshly packaged core source, package distribution no-sign preflight,
  package preflight handoff guidance self-test, version-consistency self-test,
  signed-provenance self-test,
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
  The CLI E2E also reuses the complete release-evidence fixture across
  `jarvis release evidence-status` and
  `./scripts/release-evidence-doctor.sh --assert-complete`, including the
  bundled core executable `--version` check, so the Rust CLI status and shell
  doctor inventory do not drift independently.
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
  Swift scheduler action coverage is in `apps/mac/Tests/JarvisMacCoreTests`,
  including typed client paths for `/scheduler/run-due` and
  `/scheduler/recover-stale` plus `SchedulerModel` run/recovery refresh
  behavior.
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
  package. It records and validates Developer ID, notary UUID, stapler success,
  exact Gatekeeper acceptance, and top-level `Jarvis.app` zip payload shape
  before writing the signed-distribution provenance report. The provenance
  self-test includes negated Gatekeeper and nested app-zip negative fixtures.
  `./scripts/release-version-consistency.sh --check` derives the
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
  command path, live audio-output, notification, restart, and release-QA
  runbook, including the exact template, source, and `--assert-complete`
  commands for owner evidence capture.
  `cargo run -p jarvis-cli -- release live-device-runbook` is the side-effect-free
  CLI companion for operators; it combines conservative readiness with current
  `live_device_qa_report` evidence status and prints the exact template,
  assertion, evidence-status, and evidence-aware readiness commands to run. It
  is part of the default local release gate so the runbook remains executable.
  Its `--assert-complete` mode requires an installed app plus explicit
  `JARVIS_QA_*` owner flags, including
  `JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true`, then writes a JSON evidence
  report with `voice_command_observation.command_result_evidence_id`. The
  script validates the ID shape offline, while `/release/evidence-status`,
  `release-evidence-doctor.sh --assert-complete`, and evidence-aware
  `/release/readiness` require the ID to resolve against task/audit records
  through repository-backed IPC state before the report can clear readiness.
  The script writes the report
  to `JARVIS_QA_REPORT_PATH` or
  `target/release-live-device-qa-report.json`. The report records installed-app
  metadata, voice-loop evidence fields, owner-recorded live voice evidence
  fields for owner/device/profile/non-future timestamps/notes, structured
  spoken-command observation fields with observed transcript matching the spoken
  test phrase, expected command text matching observed command text, validation
  flags, schema identity, UTC report generation timestamp, and proof boundary.
  Live macOS notification prompt/delivery validation is part of the manual
  clean-profile release QA runbook and final release evidence boundary; it is
  not currently a separate field in the live-device voice report.
  `release-live-device-qa.sh --assert-complete` and `/release/evidence-status`
  both reject empty or placeholder owner evidence-note fields before this
  report can clear `live_voice_loop`.
  This standardizes manual evidence only; `--check` does not prove live device
  behavior, and the report remains an owner assertion. When the release operator
  explicitly enables evidence-aware readiness, this report can support the
  narrow claim that the live voice loop was validated for that release
  candidate, not a generalized claim that voice is validated for every device or
  future release. Use
  `./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env`
  to generate a sourceable checklist for all required `JARVIS_QA_*` fields. The
  generated template materializes `JARVIS_QA_EXPECTED_VERSION` from the
  canonical Rust package release version so sourced operator evidence stays
  aligned with the app/core version under validation.
  `--self-test` uses a fake app fixture to validate assertion/report mechanics
  in the local release gate without claiming live device validation.
- `cargo run -p jarvis-cli -- release signed-distribution-runbook` is part of
  `./scripts/release-local.sh` as a read-only operator companion for signed
  distribution. It combines conservative readiness with current
  `/release/evidence-status` inventory for the app bundle, app executable,
  bundled core, signed app zip, signed installer package, and signed provenance
  report, then prints the package-distribution, evidence-status,
  evidence-doctor, and live-device runbook follow-up commands. It does not
  perform signing, notarization, stapling, Gatekeeper assessment, installation,
  live-device QA, or plugin-trust QA.
- `cargo run -p jarvis-cli -- release plugin-trust-runbook` is part of
  `./scripts/release-local.sh` as a read-only operator companion for plugin
  trust QA. It combines conservative readiness with current
  `/release/evidence-status` inventory for `plugin_trust_qa_report`, then
  prints the plugin-trust check, template, assertion, evidence-status,
  evidence-doctor, and signed-distribution follow-up commands. It does not
  perform marketplace review, malware scanning, sandbox deployment, host-level
  egress enforcement, signing, notarization, live-device QA, or final evidence
  bundling.
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
  remain pending until a valid owner-recorded live-device QA report passes
  `/release/evidence-status` semantics and evidence-aware readiness is
  explicitly enabled for that release candidate.
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
- Older scheduler notification, activity summary, and activity event-stream
  worktree names are historical only unless the branch is re-created and
  verified active in the current checkout.
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
- The June 10, 2026 autonomous production-readiness sweep used six parallel
  audit lanes for release readiness, architecture/KB consistency, E2E coverage,
  Swift voice coverage, release evidence scripts, and GitHub/PR state. The
  live readiness snapshot at sweep start reported `production_ready: false`,
  17 verified features, and one pending feature: `live_voice_loop`. That
  pending feature remains a manual external validation gate, not a missing
  repo-local docs-only task.
- PRs #214 through #222 added structural release-evidence hardening, plugin
  trust evidence hardening, package provenance hardening, Mac scheduler action
  controls, GitHub release-local runtime compatibility, archive URI validation,
  release contract wording, evidence-status proof-boundary wording, and current
  sweep snapshot updates while preserving the same readiness boundary: 17
  verified repo-owned features, one pending manual `live_voice_loop` feature,
  and six missing external/manual evidence artifacts.
- `jarvis release readiness` and `jarvis release evidence-status` preserve
  operator-readable defaults. Use `--json` for the canonical machine-readable
  flag, while `--format json` is accepted as a compatibility alias for older
  release scripts or operator notes that used format-style JSON output.
- The release runbook commands follow the same convention:
  `jarvis release live-device-runbook --format json`,
  `jarvis release signed-distribution-runbook --format json`, and
  `jarvis release plugin-trust-runbook --format json` are compatibility aliases
  for their structured `--json` summaries.
- `./scripts/package-distribution.sh --check` is now part of
  `./scripts/release-local.sh`, the readiness recommended-command list, and the
  signed-distribution runbook before the unsigned launch and credentialed
  signing commands; it remains a no-sign preflight for packaging prerequisites
  and entitlement templates, then prints the signed-distribution runbook,
  credentialed packaging, live-device, plugin-trust, final bundle, and evidence
  doctor commands without proving signing, notarization, stapling,
  installation, or live-device QA. `--check-guidance-self-test` is also part of
  `./scripts/release-local.sh` and fails if those handoff commands drift out of
  the package preflight output.
- Release evidence structural hardening now treats the final evidence chain as
  cross-bound evidence, not independent files: app zips are rejected unless they
  contain exactly one top-level `Jarvis.app` payload with `Info.plist`, the app
  executable, and the bundled core; live-device QA `bundled_core.sha256` must
  match signed-provenance `artifacts.bundled_core_sha256`; and final bundle
  owner completion must occur after signed-provenance, live-device QA, and
  plugin-trust child reports are generated but no later than the final bundle
  generation timestamp.
- `release-evidence-doctor.sh --check` remains a read-only inventory and report
  semantics check: it validates the bundled-core version marker and report/file
  bindings without executing the bundled core. Its missing-evidence next-step
  guidance starts with `./scripts/package-distribution.sh --check` before the
  credentialed signing command. `--assert-complete` keeps the stronger
  executable bundled-core `--version` check for final local inventory assertion
  after owner evidence exists.
- For docs-only readiness synchronization phases, record the relevant existing
  E2E or focused integration coverage instead of adding artificial tests.
  Behavior changes still require matching coverage before broader readiness
  language can be used.
- The June 11, 2026 production-readiness sweep refresh was updated again after
  PR #238 from `main` at `4a4661e`: readiness still reported
  `production_ready: false`, 17 verified features, and one pending feature
  (`live_voice_loop`). In the main checkout, evidence-status reported 3
  satisfied generated local app/core paths, 6 missing external/manual evidence
  items, and 0 invalid items; fresh worktrees can still report the generated
  local app paths as missing until local distribution commands create them.
  Production readiness still requires signed/notarized artifacts,
  live-device QA, plugin-trust QA, and final evidence bundle reports. PR #231
  made Swift/readiness display fail closed on effective readiness unless
  evidence status is complete, PR #232 clarified exact release evidence script
  handoff commands, and PR #233 hardened the Swift voice UI so unavailable
  capture, missing submit handlers, and busy submitters cannot imply live voice
  loop readiness. PR #234 rejected placeholder owner evidence notes in core
  evidence-status/final-bundle paths, PR #235 ignored stale AVSpeech callbacks,
  PR #236 rejected placeholder live-device QA notes in the shell assertion path,
  PR #237 added a package-check guidance self-test, and PR #238 locked readable
  evidence-status present-item path/detail coverage in CLI E2E plus docs.
- Swift release-readiness fixtures should stay aligned with live
  `jarvis release evidence-status --json` wording, including presence-only
  executable details, `expected evidence path is missing`, `Plugin-trust QA
  report`, and `Release evidence bundle`. The live-device QA shell self-test
  should compare bundled-core version output against `EXPECTED_VERSION`, not a
  hard-coded release string, so version bumps do not create false QA failures.
- The Swift speech-output adapter tracks the active AVSpeech utterance by object
  identity and ignores completion/cancel callbacks for older utterances, so
  stopping or replacing speech cannot let a stale delegate callback mark newer
  playback idle. Swift tests cover this without invoking live audio output.
- Release evidence placeholder hardening now rejects owner-recorded placeholder
  notes in live-device QA reports and final release evidence bundles through
  core IPC/evidence-status validation. The final bundle script rejects the same
  placeholders before writing a bundle, and readable
  `jarvis release evidence-status` output includes each evidence item's path and
  detail, including present presence-only caveats on the item line, so
  operators do not need `--json` for basic triage.
- `JarvisMacAppTests` covers app-level Release tab presentation for
  presence-only evidence rows, while `JarvisMacCoreTests` continues to cover
  the release-readiness model and evidence-status decoding.
- `jarvis release evidence-status --help` documents that the default readable
  output includes per-item paths/details and same-line presence-only caveats;
  keep this help text, CLI E2E assertions, and `docs/release-checklist.md`
  aligned when the readable release-evidence format changes.
- `jarvis release plugin-trust-runbook` is the handoff from plugin-trust QA
  into final evidence bundling: after `release-evidence-doctor.sh --check`, it
  should list `release-evidence-bundle.sh --check`, template writing, source
  plus `--bundle`, and `release-evidence-doctor.sh --assert-complete`.
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
  Subprocess stderr may contain bounded progress frames, but raw stderr plus
  local plugin paths and provenance hashes remain redacted from response and
  audit payloads. Any broader executable path or
  real-time plugin progress
  stream needs a stronger OS-level process/network sandbox or equivalent host isolation boundary,
  explicit grant state beyond `metadata_only`, policy checks,
  timeout/cancellation behavior, and E2E audit coverage.
