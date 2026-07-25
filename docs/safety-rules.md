# Safety Rules

Assemblywright is designed for high autonomy with explicit boundaries. These rules are
release requirements, not optional UX guidance.

## Policy Defaults

- One authenticated owner holds all authority. Models and workers may propose,
  but they cannot enqueue, reorder, cancel, abandon, grant authority, or rebind
  providers.
- Implementation runs on restricted local coding agents. No cloud model
  receives repository-write or implementation authority.
- Planning and acting are separate. Generating a plan does not grant permission
  to execute side effects.
- Ambiguity fails closed. A blocked state names one exact next owner action.

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
- Developer Mode enrollment must remain an explicit, bounded two-host ceremony.
  The confirmed Windows pairing process may persist only the grant digest and
  must never print, log, place in argv/environment, or write the raw 256-bit
  grant secret to a file. It emits a strict secret-free invitation, keeps the
  raw grant only in bounded process memory, accepts one strict CSR reply on
  stdin, and issues only when the grant, device, expiry, role, registry
  revision, endpoint, and CSR match. Interruption leaves no transferable
  secret and the digest-only grant expires without automatic retry.
- The Mac Developer Mode identity must use a distinct device-only Keychain
  namespace. Its P-256 private key is generated as a permanent non-exported
  Keychain key and must never enter SQLite, files, argv, environment, logs,
  diagnostics, jobs, or enrollment output. Certificate installation must fail
  before promotion unless the signed leaf matches the staged public key and
  invitation device, role, registry revision, validity, endpoint, and pinned CA
  fingerprint. Pending enrollment material must not authorize a connection.
- The live fixture device must use a second device-only Keychain namespace.
  Absence of an exact `--identity-profile fixture` argument must keep using the
  existing standard accounts, key tag, certificate label, and installed
  profile. The fixture namespace must reject staging, installation, status, or
  TLS use unless its capability list is exactly
  `fixture.reasoning` / `assemblywright-fixture` / `assemblywright-fixture-v1` with the fixed
  8 KiB bounds. MLX, mixed, corrupt, or cross-profile material fails closed.
  Local fixture removal requires a confirmed fixture-only command and never
  deletes the standard identity; Windows revocation remains authoritative.
- Every Mac-to-Windows Developer Mode session must be outbound, use the exact
  private-overlay IP from enrollment, pin the enrolled CA, present the enrolled
  client identity, require TLS 1.3, and bind the strict application handshake
  to SHA-256 of 32 bytes exported with
  `EXPORTER-Assemblywright-Developer-Mode-v1` on that same persistent connection.
  Missing channel binding, ordinary system trust without the pinned CA,
  certificate/key mismatch, expiry, revocation, role or capability drift,
  registry-revision mismatch, replay, or non-accepted handshake fails closed
  and cancels the channel. Tailscale reachability is never Assemblywright authority.
- `Assemblywright.app` may supervise the Mac bridge only through an explicit exact
  helper path plus an independently supplied Apple team. It must validate an
  Apple-anchored, pinned-team requirement, the
  fixed bridge identifier and exact executable, plus the bridge-only Keychain
  access group before launch, then revalidate the running child and prevalidated
  CDHash before accepting output. It must launch only the monitor or exact
  relay command with a cleared environment; fixture relay additionally requires
  the explicit `--identity-profile fixture` child argument rather than inherited
  environment.
  own and boundedly TERM-to-KILL reap at most one child; and bound both the
  phase-specific redacted snapshot and its queue. Replacement, duplicate or
  extra fields, malformed, oversized, or overproduced output, EOF, and launch
  or signature failure must fail closed. The app must not load or share the bridge
  private-key identity, infer command authority from health, or claim bundled,
  unattended background operation before packaging and live evidence exist.
- A live Developer Mode outage claim must be observed through the production
  Swift lifecycle, not inferred from Windows service status alone. The proof
  must begin on an authenticated positive epoch, record fail-closed Master
  Offline with no retained connection epoch while the service is stopped, and
  accept recovery only after a fresh authenticated session returns a strictly
  higher epoch. Coordination artifacts may contain only epochs and fixed
  redacted error codes. A service stop/start does not prove Tailscale or network-
  interface outage recovery, bundled background operation, or command safety.
- Distributed task events are Windows-authoritative metadata, not a second task
  database. Enqueue, lease, terminal result, cancellation, disconnect, expiry,
  and startup reconciliation must append a contiguous server-issued event in
  the same SQLite transaction as the corresponding state transition. Event
  batches contain only typed IDs, event kind, timestamp, connection epoch, and
  cursor; prompt text, retrieved context, source snippets, results, policy
  payloads, credentials, paths, and raw errors are forbidden. A cursor from a
  different stream, a future cursor, a gap, replay, oversized batch, invalid
  identity shape, or non-MacBridge remote session fails closed.
- Fixture-job execution is a separate default-off diagnostic, never an implied
  consequence of enabling the metadata cursor. It requires the exact registered
  `fixture.reasoning` / `assemblywright-fixture` / `assemblywright-fixture-v1` descriptor and
  accepts only Public `synthetic_echo` context with at most 4 KiB of UTF-8 input,
  an 8 KiB context/result ceiling, a 5-second synthetic delay ceiling, and
  `ephemeral_no_retention`. The agent may hold one active fixture in memory but
  must not persist its context, output, job, result, cancellation, or
  acknowledgement. The adapter has no model, tool, file, repository, credential,
  network, Codex, or Git authority.
- MLX-job execution is a separate default-off standard-profile capability,
  never an implied consequence of enabling the metadata cursor or fixture
  adapter. Registration must contain the exact singleton `mlx.reasoning` /
  `local_inference` / `mlx` descriptor and its configured model and bounds;
  mixed, fixture, or unknown capability sets fail closed. The job must be
  Public and `ephemeral_no_retention`, use exactly `generate_text`, contain a
  nonempty prompt of at most 32 KiB, request 1 to 512 tokens, and use a
  temperature from 0 to 2000 milli-units. The agent may hold only one active
  attempt in memory and must persist no prompt, output, job, result,
  cancellation, or acknowledgement.
- The MLX executable and model directory must be absolute, canonical, local
  startup-stdin configuration. The executable must be a regular executable
  file rather than a symlink. The agent must clear inherited environment,
  force offline/telemetry-disabled execution, pass the prompt only through
  stdin, discard stderr, bound stdout, and run the backend in a dedicated
  process group. Cancellation, emergency pause, timeout, lease loss, or
  disconnect must TERM-to-KILL and reap that process group before completion;
  a simultaneous or late result is suppressed. This lane grants no tool, file,
  repository, credential, network, Codex, Git, publication, or unattended
  authority.
- The Windows `assemblywright-master` schema-v5 Durable Feature Conveyor kernel is
  default-inert. Its only HTTP/API surface is the owner-token-authenticated,
  loopback-only `GET /v1/feature-conveyor/status`: a pure-SELECT, bounded,
  structurally redacted lifecycle-observation projection for current queue and
  retained-lease entries. It is insufficient to determine claimability,
  dependency blockers, or owner action, must not drive owner action, and is
  absent from the enrolled-device remote mTLS router. It exposes no mutation,
  worker, Codex, repository, GitHub, publication, or automatic activation
  authority. Approved feature manifests must be canonical bounded JSON with an
  exact SHA-256 binding; their
  immutable numbered specification rows and owner-approval/design/brainstorming
  proof digests are append-only. The three repository grant revisions remain
  independent and one never implies another. The queue admits at most 100
  queued or active nonterminal features, uses one compare-and-set queue
  revision, never skips a blocked owner-ordered head, and permits only one
  database-backed active lease.
- Feature Conveyor state mutation and its redacted audit evidence must commit
  in the same immediate SQLite transaction. Manifest content, repository
  identity/path, brainstorming content, and owner reason must not enter audit
  payloads. Audit failure rolls back enqueue, reorder, claim, lifecycle,
  cancellation, abandonment, and quarantine. Emergency Pause blocks new feature
  claims. Cancellation retains the active lease and never advances the queue.
  Only exact healthy-main success, or an explicit owner abandonment after
  proven-safe reconciliation and healthy-main verification when a commit
  reached `main`, may release it.
- A file-backed master schema upgrade that introduces or changes Feature
  Conveyor state must run under the master owner lock, create and verify a
  pre-migration backup before mutation, and restore that backup if migration
  open fails. On startup, an active Feature Conveyor stage that might have
  crossed a worker, repository, review, publication, or other effect boundary
  becomes `quarantined` with `effect_possible:true`; it is never retried or
  released automatically. Partial kernel mechanics must not be described as an
  autonomous development system.
- Remote raw-step enqueue is forbidden. Fixture work is queued only through the
  Windows-local authenticated development seam, then leased only to the
  authenticated device that registered the exact fixture capability. Result and
  cancellation acceptance must bind device, connection epoch, task, step,
  attempt, lease, cancellation ID, sequence, and digests. Emergency pause,
  maintenance, expiry, disconnect, cancellation, or acknowledgement timeout
  dominates result acceptance; late or duplicate output is suppressed and
  rejected.
- Fixture emergency-pause mutation is Windows-local and owner-authenticated.
  The loopback activate/resume actions accept only `{}`, expose no planning or
  enqueue authority, and must not be registered on the enrolled-device mTLS
  router. Activating pause must atomically move active fixture attempts into
  durable cancellation; deliberate resume may reopen admission but must never
  revive an old lease or permit its late output. While paused, only the exact
  authenticated `503 {"error":"emergency_pause_blocks_work"}` lease response
  is treated as bounded no-work so cancellation events and paused health can be
  observed; other 503/error shapes still fail the session.
- A fixture no-work `204` is accepted only as a strictly bodyless HTTP/1.1
  response with absent or zero `Content-Length`; malformed field names or
  lengths, transfer encoding, a nonzero length, or same-read trailing bytes
  must fail closed. The connection must then close before another request so
  later chunks cannot become a false next response. Cancellation polling uses
  an exact length-delimited `{"status":"no_cancellation"}` response instead of
  `204`, with duplicate and escaped-equivalent top-level keys rejected,
  preserving the active lease epoch without reusing ambiguous framing.
- The owner-run `--run-fixture` harness may coordinate only fixed redacted
  markers and owner-only `0600` sanitized receipts with those Windows-local
  controls. The bearer-authenticated loopback event query may return only
  task/step identity, event kind, cursor, device/epoch metadata, and timing from
  the existing batch; it must never expose context, payload, result, prompt, or
  raw error and must remain absent from the remote router. Require exact ordered
  event binding, agent consumption through each terminal sequence, a bounded
  seven-second no-late-event cancellation window followed by a fully paged
  durable-head observation completed after the deadline. Reject every
  same-task event that is not the next expected kind, every stream/cursor
  regression, and any tail that exceeds the bounded drain. Require cursor
  restart and a fresh
  standard-profile authentication. The final receipt must omit fixture
  identity, input/result, tokens, certificates, paths, and raw errors and remain
  synthetic live-device evidence rather than model, repository, Codex, Git,
  unattended, or release proof.
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
  `com.nobiletechnology.assemblywright.core` identifier. Alternate package bundle
  identifiers are rejected because they cannot satisfy the fixed production
  code-identity contract.
- The optional Developer Mode event relay keeps the enrolled private key and
  TLS session inside the separately signed Swift helper. `Assemblywright.app` may pass
  only an absolute agent executable path and absolute agent data-directory path
  through one strict, bounded, secret-free helper startup document; a partial,
  relative, extra-field, oversized, or malformed opt-in disables the launch.
  The helper must validate the exact agent static signature, path, identifier,
  and CDHash before launch and revalidate the running PID before sending its
  startup document. It must be the agent's declared direct parent.
- The helper alone creates the agent's current-owner `0700` runtime directory,
  bounded `0600` socket leaf, and fresh 32-byte bearer. Those values and the
  helper's exact designated requirement travel only through bounded agent
  stdin, never argv, environment, status, logs, or app-to-helper configuration.
  Both UDS peers must validate audit token, current EUID, exact executable path,
  and exact CDHash before framing; the agent must reject any requirement text
  that is not canonical for its declared profile before opening durable state.
  The agent cursor may store only stream ID, sequence, and update time.
- By default the helper may request only the authenticated MacBridge metadata
  event route and may forward only its exact bounded response body to the agent.
  The separate exact fixture opt-in may additionally use only lease, result,
  cancellation-poll, and cancellation-acknowledgement routes for the registered
  Public synthetic fixture contract. The agent
  remains the final strict protocol validator and rejects gaps, replay, stream
  replacement, unknown fields, or identity-shape mismatches before cursor
  commit, and rejects malformed, oversized, non-Public, non-fixture, mismatched,
  concurrent, cancelled, or late fixture work before output forwarding. Any
  master, helper, UDS, identity, cursor, fixture, cancellation, or deadline
  failure cancels the current mTLS session and enters fixed redacted backoff; it
  never authorizes a model, tool, file, repository, Codex, or Git action.
- Signed release provenance must record the exact app executable path and
  SHA-256 plus its code Identifier, ten-character TeamIdentifier, and CDHash.
  Live-device QA must revalidate the installed executable and bind its report
  to that signed-provenance report by path and SHA-256. Final bundling, doctor,
  and Rust evidence-status validation must reject any executable digest or code
  identity mismatch. This is point-in-time candidate evidence only; it does not
  establish installation provenance, continuous runtime integrity, or Apple
  attestation.
- Only exact `ASSEMBLYWRIGHT_MAC_ENABLE_IPC_CLI_HANDOFF=true` may select the explicitly
  weaker authenticated loopback TCP and owner-only token-file compatibility
  path. `ASSEMBLYWRIGHT_MAC_IPC_AUTH_FILE` may select an absolute override only in that
  mode. The file must be bounded, no-follow, single-link, owner-matched, and
  have no group/other permissions. The supervisor must remove both app-only
  variables, `ASSEMBLYWRIGHT_MAC_RELEASE_SMOKE`, and `ASSEMBLYWRIGHT_IPC_TOKEN_FILE` from the child. Managed TCP clients must
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
- The candidate update token is an aggregate integrity binding, not a raw
  manifest/source-tree/command/module provenance hash, publisher signature,
  artifact trust verdict, or execution grant. Clients must treat it as opaque
  compare-and-set data and must not infer trust or authority from possession.
- Audit logs must explain model route, permission checks, tool calls,
  approvals, denials, files touched, external actions attempted, failures, and
  final state.
- Terminal approval-execution state, task state, and terminal audit evidence
  must commit atomically. Failure, cancellation, and timeout after the claim
  must record that an effect remains possible. A crash, restart, or persistence
  failure that leaves a claimed execution unresolved is likewise ambiguous.
  Assemblywright must never automatically retry a claimed approval; the operator must
  review the evidence and create a new approval for any deliberate new attempt.
- A file-backed repository must acquire its secure sibling `.owner.lock`
  before migration backup, version inspection, database open, or migration and
  retain the lease for its lifetime. The lock must be opened no-follow and
  close-on-exec, be a current-owner regular single-link file with mode `0600`,
  and accept one nonblocking exclusive Unix lock. Symlink, hard-link,
  permissive-mode, wrong-owner, unsupported-platform, and competing-owner
  states fail closed before SQLite mutation.
  Treat this as coordination among cooperating Assemblywright repositories only; never
  claim the advisory lease OS-blocks raw SQLite or noncooperating writers.

## Current Blocks

These are blocked unless `DESIGN.md` is revised and tests prove the new policy:

- Any cloud model implementing repository changes.
- Autonomous dispatch, repository mutation, or publication before the required
  live evidence exists and the owner explicitly activates it.
- Automatic backlog generation, replenishment, or model-controlled ordering.
- More than one active feature.
- Automatic provider fallback or automatic active-feature rebinding.
- Automatic advancement after cancellation, failure, attention, quarantine, or
  abandonment.
- Peer-to-peer worker authority, shared writable worker checkouts, or worker
  Git publication credentials.
- Reintroducing a general-purpose assistant surface: conversation runtime,
  model routing, plugins, personal memory, scheduling, voice, or trusted wake.

## Regression Tests

Safety regressions should fail release verification:

- A worker or model gaining repository-write, credential, network, or
  publication authority it was not granted.
- An authoritative state transition committing without its redacted audit
  event in the same transaction.
- A result accepted for anything other than its exact leased attempt.
- Cancellation, timeout, disconnect, or emergency pause failing to dominate a
  simultaneous completion, or late output reaching a consumer after it.
- A lease released after cancellation or failure without explicit owner
  abandonment.
- Ambiguous restart, publication, or external-effect state resuming
  automatically instead of quarantining.
- Event batches or audit surfaces containing prompt text, retrieved context,
  results, credentials, paths, or raw errors.
- Diagnostics containing raw secrets.
