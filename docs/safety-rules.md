# Safety Rules

Jarvis is designed for high autonomy with explicit boundaries. These rules are
release requirements, not optional UX guidance.

## Policy Defaults

- Local models are the default route.
- ChatGPT is the only approved cloud model and must be explicitly routed.
- Restricted, credential-adjacent, private personal, and sensitive system data
  cannot be sent to ChatGPT without explicit approval for that task.
- Risky actions fail closed when policy, identity, plugin validation, route
  checks, or permission state is uncertain.
- Planning and acting are separate. Generating a plan does not grant permission
  to execute side effects.

## Risk Tiers

| Tier | Meaning | Default behavior |
| --- | --- | --- |
| Low | Local, reversible, low-impact work | May run silently with audit logging |
| Notify | Low-risk but user-visible or state-changing | May run with visible status |
| Confirm | Meaningful side effects or sensitive context | Requires explicit approval |
| Block | Not allowed in current policy or product scope | Must not run |

## Required Controls

- Emergency pause stops new actions, pauses scheduled and event-driven jobs,
  cancels active non-critical tasks, and requires deliberate resume.
- Cancellation must propagate across tasks, tool calls, scheduled jobs, and
  proactive triggers.
- Interactive `POST /commands` accepts an optional client-generated UUID
  `cancellation_id` for backward compatibility. Swift and CLI clients must
  generate the handle before submission so Cancel does not depend on a task ID
  arriving first. The Swift console model must serialize submissions and reject
  keyboard, voice, or direct overlap before changing its active handle. Rust
  must register and activate that exact handle before
  command execution, bind it to only the task created by that command, cap the
  shared active registry at 128 handles, and reject duplicate/over-capacity
  registration. Finalization must retain the 1,024 most recently consumed UUIDs
  as bounded FIFO tombstones and reject their reuse, so a delayed stale cancel
  cannot target a later run within that process-local window. Clients must use
  fresh random UUIDs because tombstones are evicted after the cap and disappear
  on core restart. Authenticated `POST /runtime/cancellations/:id` may cancel only
  an active matching handle and must report `cancellation_requested` versus
  `not_found` honestly. Cancellation must dominate the provider/tool result-acceptance
  boundary: if it wins guard finalization, late model steps and plugin results
  are discarded and the task is cancelled; if finalization wins first, a later
  cancellation reports `not_found`. This cooperative boundary cannot reverse
  an external effect already performed and is not distributed cancellation or
  crash recovery.
- Ollama-native response streams must remain quarantined until a bounded body
  contains one terminal `done:true` frame and the complete response envelope
  validates. Partial text, partial JSON-looking tool envelopes, provider error
  detail, and post-terminal frames must not reach audit, IPC, Swift transcript,
  or tool execution. Cancellation and emergency pause must drop the active
  transport and dominate a simultaneous model completion before exposure.
- Degraded modes must be visible when local models, microphone access, TTS,
  ChatGPT, plugins, persistence, or IPC are unavailable.
- Model-originated tool calls must be constrained to runtime-derived inventory.
  The default inventory is the registered first-party `PluginHost` manifests.
  A separate installed-tool catalog may be added only when the individual
  command explicitly opts in with `installed_wasm_tools`, routing selected a
  reactive local-model provider, and each advertised installed record is an
  enabled, currently provenance-matching, eligible `local_wasm` plugin.
  Unknown plugin IDs, undeclared actions, non-object inputs, schema failures,
  and requests outside the exact per-step advertised inventory must fail closed before execution and be
  surfaced as rejected tool results for bounded model recovery; oversized plans
  and malformed provider envelopes must fail the task. Native provider
  function-call formatting cannot bypass registry lookup or schema validation.
  Cloud or proactive routes, commands without the opt-in, identifier collisions,
  ineligible or stale records, and every installed `local_subprocess` action
  must remain absent from the catalog and fail closed before execution.
- Production workspace inspection is disabled unless an operator configures an
  allowlisted root. Requests may contain only an opaque root ID and validated
  relative path. Descriptor-anchored no-follow traversal must reject absolute,
  empty, dot/parent, hidden, credential-like, symlink, special, binary, and
  oversized targets; list/read results are bounded and audits contain only
  root ID, normalized relative path, counts, limits, truncation, and outcome.
  File contents are untrusted data, never instructions, and may continue only
  through a local-model route. ChatGPT/cloud routes fail closed before a
  workspace action runs or a workspace result is exposed. Emergency pause,
  task cancellation, and timeout dominate completion and discard late output.
  `fake_*` tools are test fixtures and must not appear in production inventory.
  `workspace_inspect.list` may use only the explicit `@root` sentinel for the
  held root; other inputs are non-empty normal relative paths. The enforced
  ceilings are 200 listed entries, 64 KiB per UTF-8 read, 16 KiB per line, and
  128 KiB cumulative tool output per task.
- App-owned workspace grants must persist bookmark bytes and opaque IDs, never
  present stored or resolved absolute paths, and resolve the complete set before
  launching the core. Stale-unrecoverable, inaccessible, duplicate,
  non-directory, malformed, or oversized grants fail the launch atomically.
  Resolved paths travel only in the bounded versioned startup-stdin envelope;
  they must not enter argv, environment, health, diagnostics, audit, or errors.
  Startup delivery runs off the main actor under a hard timeout; failure or
  timeout force-terminates and reaps the child. Security-scope access is
  balanced across stop, launch failure, unexpected child exit, replacement,
  and deinitialization. This is capability-lifecycle discipline, not proof of
  App Sandbox enforcement, child sandbox-extension inheritance, or IPC caller
  identity.
- Every app-supervised core launch must rotate a 32-byte IPC bearer credential
  and default to a generation-random Unix domain socket. The strict v1 startup
  envelope is the only authority channel: it carries the bearer and
  `ipc_transport:{kind:"unix_socket_peer_identity_v1",socket_path:
  "/absolute/path.sock",peer_code_requirement:"...",peer_identity_profile:
  "adhoc_exact|developer_id_hardened"}`; none may enter argv or child
  environment. The requirement must be nonempty, at most 4096 bytes, and free
  of NUL. The runtime directory must be a
  current-owner `0700` directory, the socket must be `0600`, and its absolute
  path must fit the platform socket-path bound. Both peers must retrieve
  `LOCAL_PEERTOKEN`, resolve the running peer through Security.framework, and
  reject it before frame parsing unless it satisfies the expected designated
  requirement. Both must also use `getpeereid` and reject an EUID other than
  their current EUID. Every route,
  including health, activity, release, and trusted-wake control, must also
  require the launch bearer; peer EUID is defense in depth, not a bearer
  replacement.
- The UDS wire contract permits one four-byte big-endian length and one strict
  versioned JSON request per connection, followed by a required write-half close
  before one framed response. Requests allow only GET,
  POST, DELETE, and PATCH; exact nullable header fields and standard padded
  base64 body fields must reject unknown, malformed, duplicate, oversized, or
  trailing input. Frame/body, hard monotonic deadline, and in-flight connection limits fail closed.
  The shared Swift client must fail locally while its managed transport or
  credential is unavailable. Launch failure, stop, replacement, and observed
  child exit clear the matching generation. Cleanup may remove only the
  validated socket leaf; wrong-type, unsafe, or changed paths must fail without
  recursive deletion.
- `adhoc_exact` may accept only an exact cdhash designated requirement for the
  current build; it is local mechanics evidence, not publisher trust.
  `developer_id_hardened` must require Apple-generic anchored Developer ID
  Application leaf/intermediate certificate extensions, stable app/core
  identifiers, the same nonempty team identifier, and hardened-runtime
  CodeDirectory flags. Unsigned,
  malformed, mixed-profile, missing-audit-token, or wrong-code peers fail
  closed. Packaging must sign the bundled core with the stable
  `com.nobiletechnology.jarvis.core` identifier. Alternate package bundle
  identifiers are rejected because they cannot satisfy the fixed production
  code-identity contract.
- Exact release-smoke mode may emit only a fixed non-secret success marker, and
  only after the app-owned Swift client completes authenticated health,
  dry-run command, task/audit inspection, diagnostics, pause, blocked-command,
  and resume verification over the default UDS. Any failure must suppress the
  marker, and cleanup after a successful pause must make a bounded best-effort
  resume attempt so the test path does not intentionally strand durable pause.
- Signed release provenance must record the exact app executable path and
  SHA-256 plus its code Identifier, ten-character TeamIdentifier, and CDHash.
  Live-device QA must revalidate the installed executable and bind its report
  to that signed-provenance report by path and SHA-256. Final bundling, doctor,
  and Rust evidence-status validation must reject any executable digest or code
  identity mismatch. This is point-in-time candidate evidence only; it does not
  establish installation provenance, continuous runtime integrity, or Apple
  attestation.
- Only exact `JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true` may select the explicitly
  weaker authenticated loopback TCP and owner-only token-file compatibility
  path. `JARVIS_MAC_IPC_AUTH_FILE` may select an absolute override only in that
  mode. The file must be bounded, no-follow, single-link, owner-matched, and
  have no group/other permissions. The supervisor must remove both app-only
  variables, `JARVIS_MAC_RELEASE_SMOKE`, and `JARVIS_IPC_TOKEN_FILE` from the child. Managed TCP clients must
  reject non-loopback destinations before attaching a bearer; authenticated
  TCP serving must reject non-loopback binds. Legacy explicitly unauthenticated
  servers reject any Authorization header so a managed client cannot silently
  downgrade.
- These controls prove bounded local transport, audit-token-bound designated-
  requirement checks, same-EUID checks, bearer possession, and launch lifecycle
  for the evaluated signature profile. Another process running as the user can read an
  explicitly enabled handoff file while it exists. They do not prove peer PID,
  device authentication, XPC, ownership, App Sandbox, host-level egress
  control, notarization, or live-device behavior; ad-hoc evidence specifically
  does not prove Developer ID publisher identity.
- Installed `local_wasm` execution is allowed only for validated low-risk,
  non-proactive compute actions with no memory, model, filesystem, environment,
  process, clock, or network authority and the explicit `wasm_compute` grant.
  The `jarvis_json_v1` module must import nothing and export `memory`,
  `jarvis_alloc`, and `jarvis_run`. Validation and execution fail closed beyond
  4 MiB of module bytes, 256 KiB of request JSON, 1 MiB of output JSON, 16 MiB
  of linear memory, zero table elements, or 10 million fuel units. Exact module bytes participate in
  provenance verification. Emergency pause, cooperative cancellation, timeout,
  and fuel exhaustion dominate completion and suppress output. Audit and Swift
  inspection may expose redacted runtime/enforcement metadata, never module
  bytes, paths, hashes, inputs, outputs, or raw engine errors. Wasmi confinement
  is a guest-language boundary, not an OS sandbox, same-user IPC isolation,
  marketplace/publisher trust, malware analysis, signing/notarization, or
  live-device proof.
  Installed-plugin cancellation uses an explicit unique `cancellation_id` and
  `POST /runtime/cancellations/:id`; Wasmi observes it between fuel slices and
  before output acceptance. Only runs activated immediately before runtime
  entry accept cancellation; bounded registration alone does not. The active
  registry is capped at 128 IDs and consumes each ID on exit. A legacy subprocess may already have caused
  external effects before Jarvis discards its late result.
  Output acceptance atomically finalizes the active cancellation ID; requests
  arriving after that point report that no active execution was found.
  Model planning for these actions is disabled by default, local-model-only,
  reactive-only, and explicitly enabled per command. Advertisement must expose
  no more than 16 actions, 1 KiB per description, 16 KiB per input schema, and
  64 KiB across the installed-tool catalog, never installed paths, module
  bytes, hashes, publisher material, or subprocess configuration. Execution
  must pass the normal sensitivity policy; private, credential-adjacent, and
  restricted commands require confirmation before Wasmi starts. Execution
  must repeat eligibility, enabled-grant, schema, and exact-byte provenance
  validation immediately before guest entry; a catalog snapshot is not
  authority to run changed or disabled code.
  Discovery snapshots no more than 64 enabled `wasm_compute` candidates while
  holding the repository mutex, releases it before provenance hashing, and
  rechecks unchanged database state before advertisement. Source-tree
  provenance rejects more than 8,192 entries, 4,096 files, 64 levels, or 64 MiB.
- Audit logs must explain model route, permission checks, tool calls,
  approvals, denials, files touched, external actions attempted, failures, and
  final state.
- Approval grant/deny decisions must not execute side effects. An explicit
  immediate SQLite transaction must recheck that the approval is pending,
  update the grant or denial, and append its redacted decision audit before
  committing. Free-form actor and reason text remains in the approval record
  but must not enter audit payloads. Any decision-audit persistence failure
  rolls the entire decision back to pending so no unaudited grant chain exists.
  An explicit
  approved replay must validate the approved record, still-waiting
  task, exact action, current risk and scopes, current manifest, input schema,
  and current policy before claiming execution authority. It must also find
  matching `approval_granted` audit evidence for the same approval ID, task,
  action, approved status, risk, sensitivity, scopes, and non-execution state.
  Its timestamp must be at or after `decided_at`. Current redacted decision
  metadata is accepted only with matching actor/reason-presence booleans and no
  raw actor/reason keys. The prior raw-metadata audit shape is compatible only
  without redaction/presence keys and when its actor and reason exactly match
  the approval record. Missing, stale, or unrelated evidence must fail before policy/claim audit
  insertion or plugin entry; the claim path must never fabricate grant evidence.
- The claim must use an immediate SQLite transaction to insert one unique
  schema-v13 `approval_executions` row plus a redacted
  `approval_execution_claimed` audit. A claim permanently consumes that
  approval; concurrent or later attempts fail closed before plugin entry.
- Terminal approval-execution state, task state, and terminal audit evidence
  must commit atomically. Failure, cancellation, and timeout after the claim
  must record that an effect remains possible. A crash, restart, or persistence
  failure that leaves a claimed execution unresolved is likewise ambiguous.
  Jarvis must never automatically retry a claimed approval; the operator must
  review the evidence and create a new approval for any deliberate new attempt.
- Direct installed-plugin execution must pass `PermissionEngine`. Contract dry
  runs remain non-executing and eligible Low/default-sensitivity invocations may
  run directly. Confirm actions or sensitive invocations must create a pending
  approval before runtime entry. Schema v15 binds that approval to canonical
  input and the exact manifest, provenance, and execution grant; input and
  binding digests stay out of public approval, audit, and diagnostics surfaces.
  Approval execution must reject changed input integrity, contract,
  provenance, grant, risk, scope, pause, cancellation, or policy before the
  one-shot claim. After claim, failure is effect-possible and non-retryable.
  CLI and Swift approved-execution requests must generate a fresh
  `cancellation_id`; Rust registers it, binds it to the approved task, and
  activates it at the claim boundary. Authenticated cancellation of that exact
  active handle must discard late output and atomically record cancelled claim
  and task state. It cannot undo an effect already performed.
- Memory writes must have provenance, timestamp, category, sensitivity label,
  and review/delete controls.
- Memory index artifacts are rebuildable projections, never canonical state.
  Status surfaces expose counts only; values, keys, provenance, source IDs,
  digests, and artifact paths stay local and redacted. Corrupt or stale indexes
  fail closed until an explicit rebuild from active SQLite records succeeds.
- Scheduler jobs must remain inspectable and cancellable. Persisted scheduler
  metadata is not permission to execute proactive side effects; trigger
  execution still has to pass policy and visibility rules. Scheduled plugin
  actions must also be manifest-opted-in for proactive execution with
  `proactive_run`; non-opted-in scheduled plugin actions fail closed before side
  effects execute.
- Packaged-app scheduler automation must default off and require a persisted,
  visible user opt-in. The app may then start only the existing bounded audited
  background loop, with an interval of at least one second, a maximum of 64
  jobs per tick, and optional bounded stale-running recovery. Applying a change
  requires a deliberate app-supervised core restart. Attention polling must be
  single-flight and cancellable, stop while the core is unavailable, use only
  the redacted attention projection and bounded durable occurrence outbox, and
  recheck lifecycle acceptance after every asynchronous authorization boundary.
  Due visibility must be committed before execution; failure and stale recovery
  must revision-escalate the same occurrence atomically. Acknowledgement must be
  compare-and-swap guarded and happen only after notification-center submission
  or explicit no-authorization suppression. Delivery is at-least-once, so a
  crash before acknowledgement or a concurrent app consumer may repeat a
  stable occurrence-revision request. A failure after a pause-blocked handoff
  is an explicit revision escalation and may produce a later notification; no
  exactly-once OS-display claim is made.
  The app must never prompt from the
  background, auto-enable trusted wake, or imply LaunchAgent/OS-wake behavior.
- Trusted macOS system-wake rules are disabled by default and use explicit,
  generation-bound enablement. Only public P-256 key material crosses bounded
  supervisor stdin during explicit initial provisioning; normal app/core
  startup does not read the wake Keychain items. Bootstrap preparation must
  succeed before the app-owned healthy core is stopped, and the one-shot bytes
  are discarded after the single restart attempt. The device-only Keychain private key never enters argv,
  environment, logs, diagnostics, or SQLite. Invalid signature, session,
  generation, replay counter, UUID nonce, clock skew, input bounds, emergency
  pause, or proactive policy state fails closed. Generic scheduler restoration
  excludes trusted wake jobs, and ambiguous started events never redispatch.
  Swift counter allocation must advance past both its Keychain counter and the
  Rust rule's durable replay high-water; overflow fails closed. Ambiguous
  events require an explicit generation/state-bound resolve-without-retry action.
  Legacy bootstrap can create or idempotently confirm an enrollment only; it
  cannot rotate a key or command. Normal key rotation requires an old-key,
  active-session, domain-separated P-256 signature. Lost-key recovery requires
  the stronger exact destructive confirmation. The packaged app path also
  requires its per-launch bearer, while an explicitly launched legacy server
  remains unauthenticated; in either mode the phrase prevents accidents and is
  not device authentication, ownership proof, or same-user/process isolation.
  Prepare runs in one immediate transaction, rejects ambiguous dispatch,
  blocks accepted old-generation work, disables and advances the rule, resets
  replay high-water, stores only a bounded one-shot token hash, and quarantines
  enablement until install or cancel. Supervised install consumes the grant
  atomically and installs the staged public key disabled. Swift must retain the
  old active key until fingerprint/generation proof, journal crash recovery,
  reject expired or near-expiry restart attempts, and never auto-enable,
  auto-retry, auto-rollback, or expose private keys, tokens, or proofs. Key
  prepare accepts its secret-bearing JSON only through bounded stdin (or the
  in-process Swift IPC client), never proof/key/confirmation argv. The returned
  one-time token must flow immediately to trusted device-only Keychain journal
  code, which constructs the distinct supervised install document. The raw
  prepare response is not install stdin; neither secret-bearing form may reach
  a terminal, shell history, log, or file.
- Diagnostics exports must redact credentials, command bodies, scheduler
  commands, audit payloads, memory values, arbitrary emergency-pause reason
  text, raw cancellation reasons, and other sensitive payloads. The diagnostic
  health projection may expose only `emergency_paused`,
  `emergency_pause_updated_at`, `emergency_pause_reason_present`, and a null or
  fixed `redacted` compatibility marker in the legacy reason field; explicit
  health and pause-status operator surfaces retain their documented reason contract.
- Memory context is explicit opt-in and local-model-only. Retrieval requires a
  current canonical index, a non-proactive command, and reviewed active memory.
  Automatic context may include only Public, Workspace, or Personal records;
  Private, CredentialAdjacent, Restricted, unreviewed, deleted, stale,
  corrupt, missing, oversized, or over-budget input must fail closed. Context
  is capped at four records and 4 KiB, framed as untrusted data, never persisted
  in route/audit evidence, and rejected by cloud adapters before transport.

## V1 Blocks

These are blocked in v1 unless `DESIGN.md` is revised and tests prove the new
policy:

- Full smart-home control.
- Autonomous external communication.
- Purchases, bookings, invites, or messages without approval.
- Multi-user account sync.
- Third-party plugin marketplace.
- Cloud-first routing.
- Plugin access outside declared scopes.
- Treating arbitrary installed plugin metadata as model-plannable authority.
  The sole model-planned exception is an explicitly opted-in, reactive local
  route selecting a currently eligible `local_wasm` action under the preceding
  confinement rules; `local_subprocess`, cloud, and proactive model planning
  remain blocked. Local installs begin as disabled
  manifest metadata with `execution_grant: metadata_only`. Any installed-plugin
  run request must fail closed, append audit evidence, and report
  `side_effect_executed: false` unless the manifest is a verified
  `local_subprocess` plugin with an explicit action-scoped grant:
  `subprocess_stdio` for non-network actions or `subprocess_stdio_network` for
  network-declaring actions. The network grant must not execute non-network
  actions. Enabled subprocesses must not inherit the app/core process
  environment; only the documented plugin metadata environment allowlist is
  exposed. Subprocess audit evidence must not claim OS sandboxing or
  host-level egress enforcement until those controls are actually enforced;
  current subprocess audit payloads report `os_sandbox_enforced: false` and
  keep OS sandbox/egress proof in the manual plugin-trust QA lane.
- Treating Wasmi language-level confinement as an OS sandbox, host-egress
  policy, marketplace approval, malware scan, publisher identity, same-user IPC
  isolation, or signed/live-device release evidence.

## Regression Tests

Safety regressions should fail release verification:

- High-risk actions bypassing approval.
- Cloud routing receiving restricted data without explicit approval.
- Plugin actions executing outside their manifest.
- Installed plugin run attempts that execute while `execution_enabled` is
  false, omit manifest/action/provenance validation, or skip audit evidence.
- Installed Confirm or sensitive invocations entering Wasmi/subprocess before
  approval, or an installed approval executing after its bound input, manifest,
  provenance, grant, risk, scopes, or policy changed.
- Installed plugin network grants executing non-network actions, or stdio
  grants executing network-declaring actions.
- Installed subprocess plugin enablement or execution while provenance status
  is anything other than `matches_install_snapshot`.
- Installed subprocess plugin execution that inherits unrelated app/core
  environment variables or secrets.
- Installed subprocess plugin audit evidence that reports an OS sandbox as
  enforced when the runner only validated manifest/provenance/grants and
  cleared the inherited environment.
- WASM modules that import any host capability, omit the required
  `jarvis_json_v1` exports, exceed module/request/output/memory/fuel ceilings,
  request non-compute authority, run proactively, execute without
  `wasm_compute`, bypass exact-byte provenance, or expose output after
  pause/cancellation/timeout/fuel exhaustion.
- WASM inspection or audit surfaces that expose module bytes, local paths,
  hashes, request/output bodies, raw engine errors, or claim OS sandboxing.
- Local plugin manifests installing with invalid schema, blocked risk tier,
  missing proactive/memory/model permissions, unsafe source paths, or
  `first_party` source claims.
- Scheduled jobs running while emergency pause is active.
- Audit entries missing route, policy, approval, or action evidence.
- Diagnostics containing raw secrets.
- Diagnostics exposing command bodies, memory values, scheduler command text,
  audit payloads, arbitrary emergency-pause reason text, or cancellation reason
  text.
- Memory retrieval running without explicit opt-in, on proactive or cloud
  routes, against a non-current index, for unreviewed/deleted/high-sensitivity
  records, beyond query/item/corpus/result/context caps, without pause/cancel
  checks, with the query duplicated into retrieval-specific audit fields, or
  with retrieved value/key/provenance/identifier/score/context leakage in
  audit, diagnostics, route evidence, errors, or debug output. Existing
  task/route/model-request surfaces retain their normal user-command visibility.
