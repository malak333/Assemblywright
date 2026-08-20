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
- Standard-profile capability rebind is a separate confirmed two-phase
  ceremony and may change only an exact stale singleton `fixture.reasoning`
  registration into one exact singleton `mlx.reasoning` / `local_inference` /
  `mlx` registration with fixed protocol bounds. Windows schema v6 must bind
  the short-lived pending record to the grant, old and target registration
  digests, target revision, certificate serial/digest, exact replacement CSR
  P-256 public key, and expiry. A pending
  certificate is absent from normal certificate authentication and lease
  authority. The replacement key must sign a domain-separated acknowledgement
  over the exact grant/device/revision/serial/certificate-digest binding.
  Activation requires Emergency Pause to be clear, a disconnected device with no active attempt,
  and atomically updates registration, activates the replacement, revokes old
  certificates, and terminalizes evidence. Windows must CA-sign a separate
  domain-separated activation transcript, and the Mac must verify it with the
  staged pinned CA. Exact lost-output activation retries may reissue only the
  original terminal timestamp and binding. Grant, pending issuance, activation,
  and abort each require a same-transaction immutable metadata-only identity
  audit row with no payload, certificate, path, or secret. The standard Mac identity remains
  selected while a distinct replacement Secure Enclave key/certificate slot is
  validated; only the exact CA-authenticated Windows activation receipt may
  switch generations. Destructive Mac cancellation is allowed only before a
  replacement certificate/acknowledgement is staged. After acknowledgement it
  must refuse and preserve the key, certificate, and staged receipt for exact
  activation-receipt recovery; cancellation after promotion must never delete
  the selected replacement key or certificate. A prepare-only cancellation
  following Windows issuance requires Windows abort to succeed first. Signed
  acknowledgement digests must use exact lowercase canonical SHA-256 hex.
  Fixture-profile
  rebind, cross-profile mutation, mixed capabilities, general standard-profile
  removal, stale/replayed activation, and automatic retry are forbidden.
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
  cancellation, or acknowledgement. Mac validation of Rust-owned context and
  payload digests must hash sorted-key UTF-8 JSON with forward slashes
  unescaped; serializer-specific slash escaping must not reject an otherwise
  exact leased result.
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
- The Windows `assemblywright-master` schema-v19 database retains the schema-v5
  Durable Feature Conveyor kernel and is
  default-inert. The owner-token-authenticated loopback
  `GET /v1/feature-conveyor/status` and the dedicated enrolled-device
  `GET /v1/distributed/feature-conveyor/status` must return the same
  pure-SELECT, bounded,
  structurally redacted lifecycle-observation projection for current queue and
  retained-lease entries. Its single fixed-enum `owner_guidance` object must be
  derived with this precedence: Emergency Pause; cancelled or quarantined
  retained lease; normal active lease; empty queue; unsatisfied queue-head
  dependency; ready queue head. The object is bound to explicit queue,
  Emergency Pause, and optional feature lifecycle revisions, contains no free
  text or dependency, repository, provider, grant, evidence, or audit data, and
  labels only a display action. Its `queue_revision` and
  `emergency_pause_revision` always bind the advisory snapshot. It must not be
  treated as claimability or a callable authorization. The distributed route
  is permitted only after an exporter-bound application session is accepted
  for an exact `MacBridge`; pre-handshake, revoked, drifted, or non-MacBridge
  sessions fail closed. It accepts no owner token and exposes no mutation,
  worker, Codex, repository, GitHub, publication, audit-event, or automatic
  activation authority. Approved feature manifests
  must be canonical bounded JSON with an
  exact SHA-256 binding; their
  immutable numbered specification rows and owner-approval/design/brainstorming
  proof digests are append-only. The three repository grant revisions remain
  independent and one never implies another. The queue admits at most 100
  queued or active nonterminal features, uses one compare-and-set queue
  revision, never skips a blocked owner-ordered head, and permits only one
  database-backed active lease.
- Schema v8 owner-control designation is Windows-authoritative, nullable by
  default, and compare-and-set revisioned. Only the owner-token-authenticated
  loopback route may designate or replace it, only with an exact current,
  enrolled, non-revoked, non-fixture `MacBridge`, and the transition plus its
  structurally redacted audit must commit in one transaction. Device role or
  registration drift, revocation, a stale designation revision, a missing
  designation, or any fixture identity fails closed. Possessing another valid
  MacBridge certificate never implies owner-control authority.
- Schema v19 activation remains globally inert until six independent
  Windows-admitted references exist: deterministic repository-gate proof plus
  restricted-worker, review-provider, GitHub-publication, restart-recovery, and
  Mac/Windows-control-and-event-streaming live evidence. Receipt admission is
  owner-token loopback-only, strict, contiguous, category/origin bound,
  nonzero-digest-only, non-future-dated, and atomically redacted-audited. It
  accepts no report body, command, path, credential, provider output, or raw
  evidence. The remote designated bridge may bind only exact current admitted
  references and exact queue, designation, and Emergency Pause revisions; it
  cannot create evidence. Activation creates no queue or lease and is safe with
  an empty queue. Exact retry returns the original immutable receipt; mismatch,
  stale state, Emergency Pause, redesignation, or incomplete evidence fails
  closed.
- The owner-run repository-gate proof controller is not an evidence-admission
  or activation client. `--run` accepts no repository, command, or remote
  argument and may execute only the exact committed
  `scripts/release-local.sh` blob through argument-free Bash stdin from its own
  exact clean `main` when `HEAD`
  equals `refs/remotes/origin/main`. It must capture and compare branch, HEAD,
  tree, status, origin ref, and committed gate-definition digest before and
  after the gate. Any dirty state, wrong branch, origin or repository drift,
  gate failure, malformed observation, or handled cancellation removes partial
  output and creates no receipt. Success may write only one bounded path-free
  receipt and raw digest sidecar beneath the fixed ignored
  `target/repository-gate-proof` directory with owner-only permissions and
  atomic receipt publication. The receipt must bind schema, exact activation
  category/origin, HEAD/tree, committed gate-definition SHA-256, fixed gate
  identity, observed time, pass status, and the narrow repository-only proof
  boundary. It must not POST, admit, self-approve, activate, mutate source, or
  claim signing, notarization, live-device, worker, provider, GitHub, restart,
  control-stream, or production-readiness proof. Owner-token loopback admission
  and its same-transaction redacted audit remain a separate deliberate action.
  Handled cancellation and every rejected or failed run leave no receipt.
  The repository `target` and proof output directories must be ordinary,
  owner-matched, canonically stable directories, never symlinks. Reject an
  unsafe component before cleanup. After those checks, a new run removes only
  the exact fixed prior receipt pair, intentionally preventing failed reruns
  from presenting stale local evidence as current output.
  Git observations must be fixed to the controller root, discard all inherited
  Git configuration/redirection/replacement/alternate-object environment, and
  disable replace objects. The internal stdin/root marker used by the committed
  gate is not a public selector: clear caller values, accept it only during
  argument-free stdin execution, and unset it before the release command list.
  `target` must also reject group/world write access. Hold the validated output
  directory, bind target/output device and inode, revalidate both pathname and
  held identities before and after relative-name publication, and clean only
  relative fixed leaves in the held directory. HUP, INT, and TERM must terminate
  the gate's separate process group, boundedly escalate and reap it, and leave
  no receipt. Gate failure must start that drain immediately. Only after a
  successful leader exit, allow a short bounded natural group drain; a member
  still live at that deadline is a failure.
  Every tracked index entry must carry the normal `H` tag; assume-unchanged,
  skip-worktree, or another hidden-state tag rejects before the gate. Pre/post
  clean and stable observations detect concurrent same-UID changes to other
  gate-consumed files at those edges; they are not host-level isolation.
- The owner-supervised restricted-worker proof controller is separate from
  activation-evidence admission and activation. `--run` accepts no repository,
  remote, worker, executable, packet, or alternate harness. It requires exact
  clean `main == refs/remotes/origin/main`, normal tracked-index state, and
  stable branch, HEAD, tree, status, origin, and committed Mac/Windows live-
  controller definitions. Only the exact committed Mac harness bytes may run
  through Bash stdin; sanitized Windows-local receipts must arrive on a
  distinct inherited descriptor and may not become script bytes. Caller
  executable, agent, Tailscale, Git, and internal-marker overrides are cleared.
  Interactive receipt reads must not be truncated by the terminal's canonical
  line limit: temporarily enter no-echo noncanonical mode only for the bounded
  read, cap stored input at 8,192 bytes while draining an oversized line through
  its newline, save and restore the exact prior state on every handled exit path,
  and prove accepted, oversized, and signalled cases through real PTY regressions.
  The live flow must use the separate exact singleton `local.coding.v1`
  `InferenceWorker`, signed production Swift relay, real Rust agent, protocol
  v5, schema-v19 master, and schema-v9 conveyor projection. It must prove one
  strict queued/leased/succeeded event order, exact snapshot and packet
  bindings, Windows-side artifact validation, detached clean remote-free
  candidate integration with exact retry, private Mac retained-pair shape,
  then explicit cancellation, safe abandonment, and complete bounded cleanup.
  Prepare markers bind the generated IDs and exact queue/pause/designation/source
  baseline before grants. Retry may resume only missing exact revision-1 grants,
  or recognize one exact marker-bound queued lifecycle-revision-1 feature after
  a lost enqueue receipt; all drift fails closed. Only six unique strictly
  ordered action markers followed by one exact bounded success record are
  eligible. The complete private transcript is hashed then removed; the
  published schema-v1 receipt contains only the
  category/origin, commit/tree, committed controller-definition digests,
  transcript digest, fixed proof identity, observed time, pass status, and
  narrow proof boundary. It must contain no endpoint, device, feature, task,
  step, attempt, path, artifact, candidate, grant, or raw result. Output uses a
  fixed ignored owner-only directory, two fixed `0600` files, held directory
  identity, and digest-first/receipt-last atomic publication. Validate the
  target/output chain before invalidating a prior pair. Any malformed,
  duplicate, extra, reordered, failed, drifted, interrupted, or descendant-surviving attempt must
  remove temporary/final output and boundedly TERM-to-KILL reap the complete
  live process group. The controller never POSTs admission, activates, or
  claims a host sandbox, OS-wide egress enforcement, provider, GitHub, restart,
  control-streaming, notarization, clean-profile, or production-readiness proof.
- The review-provider live integration is fixed rather than operator-routable:
  `openai.codex`, `gpt-5.6-sol`, Codex `0.148.0`, the committed adapter and
  structured-output schema, and the owner-private Windows Codex auth home are
  the only eligible production selection. Windows provisioning downloads the
  fixed package URL into private checkout-local staging and verifies committed
  package and native-executable digests before extraction output is executed.
  Provisioning requires exact clean
  Windows `main == origin/main`, rejects reparse ambiguity, stages an owner/
  SYSTEM-private provider directory while the master is stopped, and restarts
  into a healthy protocol-v5/schema-v19 service. The master verifies and holds
  adapter, Codex, and schema identities across each call, passes only four fixed
  adapter variables after environment clearing, requires owner/SYSTEM-only
  protected DACLs on the Codex home and auth file, and preserves the pre-spawn
  Windows Job Object gate. The adapter clears again and passes only
  `CODEX_HOME` on non-Windows hosts; on Windows, OS APIs additionally supply
  `SystemRoot` and a system-directory-only `PATH` required by native DNS. It
  uses ephemeral read-only noninteractive execution and disables
  shell, agent, skill-install, image, and web surfaces. Input/output remain
  strict canonical protocol documents under `--strict-config`; credentials,
  stderr, provider prose,
  paths, and raw outputs may not enter the live receipt.
- `review-provider-proof-controller.sh --run` is separate from evidence
  admission and activation. It accepts no repository, provider, model,
  executable, schema, or alternate harness, requires exact clean published
  `main` with normal index state and stable committed definitions, and accepts
  only one fixed approval plus one fixed rejection through the Windows-selected
  provider. Windows coordination uses a separate descriptor from committed
  harness bytes. The bounded private transcript is validated, hashed, and
  removed; success publishes only an owner-private digest-first/receipt-last
  pair. Malformed input, definition/repository drift, provider mismatch,
  unexpected decision, surviving descendants, interruption, or an unsafe
  output chain leaves no current receipt. This proves selected-provider
  integration and two-case semantic sanity only, not general review competence,
  queue/gateway lifecycle, GitHub publication, activation, or release readiness.
- The GitHub-publication production adapter is fixed and default-unavailable.
  Provisioning may accept only the pinned Windows master, `gh.exe`, and `git.exe`
  identities, an owner/SYSTEM-private GitHub CLI configuration for the fixed
  owner account, repository `malak333/Assemblywright`, base `main`, and the
  exact mapping from protocol check identifiers to the two GitHub Actions
  contexts, workflow IDs and paths, and trusted workflow-file digests. A caller
  may never select a repository, executable, remote,
  credential, command, check name, branch-protection policy, or evidence body.
  Authentication tokens must remain in GitHub CLI secure storage and must not
  enter argv, environment, files created by Assemblywright, logs, audit,
  transcripts, or receipts. Missing authentication, identity, private ACL,
  protection, context, or repository state keeps the adapter unavailable before
  durable effect intent. Credential-owning processes must start only from the
  owner-private publication root with a sanitized path; worker candidate paths
  may be derived Git arguments but never process search paths or working directories.
- Every GitHub adapter action must derive from the immutable master plan and
  poll deadline, cancellation, and current authority before, during, and after
  external work. Push only the exact frozen candidate to the derived feature
  branch; create or recover only its exact pull request; require strict
  up-to-date `main` checks, pull-request protection, administrator enforcement,
  no force/delete policy, and no bypass; verify the reviewed head; request only
  the configured normal merge; reconcile the exact resulting `main`; and
  accept the post-merge gate only from the exact successful `release-local`
  GitHub check on that commit. Never use `--admin` or an equivalent bypass.
  Missing, failed, stale, cancelled, deadline-expired, late, or ambiguous
  evidence after intent creation quarantines with effect possible and is never
  automatically retried.
- `github-publication-proof-controller.sh --run` is separate from evidence
  admission and activation. It accepts no repository, remote, executable,
  credential, branch, check, or alternate harness argument; requires exact
  clean published `main` and normal tracked-index state at start; executes only
  the exact committed native harness bytes; and receives sanitized Windows
  evidence on a distinct descriptor. The live fixture may advance protected
  `main` only with one bounded metadata-only proof-marker commit created in
  disposable private state. The source checkout must remain unchanged while fetched `origin/main`
  advances exactly to the reported protected merge. The controller hashes and
  removes its bounded private transcript and publishes only the fixed
  owner-private path-free receipt/digest pair. It never POSTs admission,
  activates, enqueues, or claims general repository support, signing,
  notarization, clean-profile, or production-readiness proof.
- `restart-recovery-proof-controller.sh --run` is separate from evidence
  admission and activation. It accepts no repository, service, data-directory,
  executable, test, receipt, Windows-control, or alternate harness argument;
  requires exact clean published `main` with normal tracked-index state; and
  runs only the committed harness bytes with sanitized Windows evidence on a
  distinct descriptor. The harness must run the exact real Rust-agent retained-
  workspace restart E2E. The fixed Windows control requires explicit `Run`
  confirmation, one owner/SYSTEM-restricted mutex, exact clean source bound to
  the controller-reported HEAD, the literal service/data paths, exact service
  owner/image, and empty protocol-v5/schema-v19 distributed and Feature
  Conveyor state with Emergency Pause clear. The Mac must pin and digest fixed
  Git, Cargo, and rustc identities, reject build-affecting Cargo configuration,
  and use absolute system tools under a system-only PATH. Windows must stop the
  service to freeze authority, validate the complete database SHA/no sidecars
  and migration backups, rebuild the expected HEAD offline with fixed absolute
  tools, require an owner/SYSTEM-only ordinary single-link service executable
  whose digest matches the installed image, prove a bounded healthy PID change,
  stop and require an identical frozen database, restore the original exact
  image, and prove a distinct final healthy PID. Failure must attempt exact
  healthy restoration and emit no JSON. Any ambiguity fails closed. The Mac controller must
  boundedly TERM-to-KILL and reap its complete harness process group, restore
  terminal state, hash and remove the private transcript, invalidate a prior
  fixed pair before a new attempt, and atomically publish only owner-private
  `0600` receipt and raw-digest files. This proves retained-agent functional
  recovery and idle authoritative-service continuity only. Active-effect crash
  recovery, SCM retry policy, signed-helper behavior, Feature 6 streaming,
  evidence admission, activation, signing, notarization, installation, and
  production readiness remain outside the claim. Master startup ambiguity must
  continue to quarantine without retry under the native focused tests.
- A Cargo-produced Windows service image that is still hard-linked to `deps` or
  inherits broader checkout ACLs is not eligible restart-proof input. Deployment
  may prepare the exact bytes only while the service is stopped, must create a
  new single-link service-path file protected to the fixed owner and SYSTEM,
  must preserve the digest, and must restore verified health before proof. The
  read-only `Check` action never performs this preparation implicitly.
- Feature orchestration owner pause/resume is separate from Emergency Pause and
  is available only to the exact current designated non-fixture MacBridge after
  an accepted exporter-bound session. It compare-and-sets feature lifecycle,
  orchestration, queue, designation, and Emergency Pause revisions. Only an
  effect-free checkpoint may owner-pause; pause stops the active clock and
  resume creates a fresh checkpoint. Cancel and abandon use the same remote
  designation boundary, cancellation retains the lease, and abandonment stays
  safe-reconciliation-gated.
- Repository-grant preparation and inspection are Windows-loopback-only and
  owner-token authenticated. A mutation carries only repository identity,
  grant kind, exact scope and approval digests, optional expiry, revocation,
  the contiguous next revision, the expected current revision, and the current
  Emergency Pause revision. Duplicate or unknown fields, nil identity, zero
  digests, skipped/stale revisions, expired active authority, stale pause state,
  and active grant creation while paused fail before mutation. Revocation stays
  available while paused. Each immutable revision and structurally redacted
  audit commit atomically; audit contains no repository identity, scope,
  approval, path, or owner content. The current projection is owner-local,
  bounded to the three fixed grant kinds, and both routes remain absent from the
  enrolled-device mTLS router. Recording a grant performs no repository access
  and grants no enqueue, claim, worker, provider, Git, or publication action.
- Repository preflight is Windows-loopback-only and owner-token authenticated.
  Its strict scope must canonically hash to the exact current active
  registration grant and bind that grant revision, the current Emergency Pause
  revision, one absolute repository path, one exact base branch, and one exact
  HEAD commit. It executes no Git process and loads no repository configuration
  or attributes. The filesystem-only observer must reject UNC, device,
  mapped/non-fixed-volume, and component reparse paths before canonical
  traversal, then require and revalidate only a canonical non-symlink local
  directory, standard `.git` and objects directories, symbolic HEAD, and exact
  loose ref for a single-component branch. On Windows, non-reparse handles for
  the fixed-volume root, every repository component, identity directories,
  symbolic HEAD, and loose ref must remain held while the complete canonical
  pathname and identities are reopened and compared immediately before the
  final grant, Emergency Pause, and audit transaction recheck; the fresh handles
  remain held through that transaction. This is not a cross-filesystem-and-
  SQLite snapshot. Worktree,
  submodule, detached, wrong-branch, wrong-HEAD,
  non-Git, symlinked, stale, expired, revoked, paused, malformed, or ambiguous
  state fails closed with a fixed error. It does not inspect or prove clean
  working-tree, index, object, configuration, attribute, or source-content
  state. A five-second timeout bounds each filesystem-observation await, not
  owner authentication or database lock/audit latency. A blocked local
  filesystem thread cannot be forcibly cancelled, so the observation remains
  limited to fixed local volumes and performs no external process or write.
  Before emitting success, the
  kernel must atomically recheck the grant and pause binding and append a
  structurally redacted audit containing no repository identity, path, branch,
  commit, remote, source, Git configuration, or raw error. Neither the path nor
  repository content is stored or returned. The receipt is point-in-time
  evidence only and grants no snapshot, claim, lease, worker, repository
  mutation, provider, credential, network, Git publication, or activation
  authority.
- Repository snapshot claiming is owner-token-authenticated and loopback-only;
  it must be absent from the enrolled-device router. The strict request binds
  the exact scope digest, branch, base commit, queue head/specification,
  provider/model, all three specification grant revisions, queue revision, and
  Emergency Pause revision. No SQLite transaction may span filesystem work.
  The source identity must be revalidated immediately before and after raw
  object snapshot construction and again before one immediate finalizer
  rechecks the head, dependencies, singleton, pause, current grants and exact
  bindings, then atomically records immutable path-free evidence, inserts the
  lease, advances lifecycle/queue revisions, and appends redacted audit. The
  snapshot must not execute Git or trust source/local/global config, hooks,
  attributes, credentials, PATH, alternates, links/reparse entries, gitlinks,
  remotes, or hardlinks. It must copy only the exact current commit/tree/blob
  graph, mark the base commit shallow, and exclude parent history and deleted
  objects. Every ODB body read must be preceded by a header type/declared-size
  check and aggregate-budget charge before decompression or allocation. A
  singleton reservation must reject concurrent claims immediately.
  Snapshot work is capped at 50,000 objects, 256 MiB total, 32 MiB per blob and
  a 30-second HTTP wait. Timeout must not pretend to cancel a blocking thread:
  that thread retains the reservation and cleanup ownership until it exits.
  Failure or request cancellation before finalization
  must leave no lease and remove the snapshot; startup removes unreferenced
  UUID directories and quarantines a finalized lease. No source path,
  content, config, remote, credential, or raw error may enter SQLite, audit, or
  the receipt. This grants no dispatch, provider, review, publication, Mac, or
  autonomous activation authority.
- The following coding-dispatch and execution description records the
  historical protocol-v4/schema-v12 fixed-fixture boundary. Coding dispatch
  remains a separate owner-token-authenticated loopback-only action and must
  remain absent from the enrolled-device router. In that historical boundary it
  accepted only a
  bounded path-free work-packet digest and ordinal/acceptance-count metadata
  after an exact snapshot claim. It binds the current feature/specification/
  lifecycle revision, feature lease, snapshot ID/digest, queue and Emergency
  Pause revisions, and one exact current non-revoked `InferenceWorker` whose
  singleton capability is `local.coding.v1`. The distributed queued step,
  immutable dispatch row, event, and structurally redacted audit commit
  atomically. Lease and acknowledgement acceptance recheck every binding;
  cancellation, Emergency Pause, lifecycle departure, registration drift or
  revocation, and restart quarantine prevent later acceptance. The metadata
  envelope rejected repository bytes or paths, commands, credentials, mutation
  instructions, and unknown fields. Owner approval of that dispatch authorized
  only the protocol-fixed contained-coding fixture described below; it never
  granted a general command, tool, path, provider, test, or network authority.
  After an exact current lease, a separate
  default-off snapshot route may expose only bounded sequential chunks of the
  immutable master-owned bundle. Every request binds the exact job, attempt,
  lease, cancellation, snapshot ID/digest, and offset; the master must
  reauthorize both before and after reading. The bridge and native agent must
  reject out-of-order, duplicate, trailing, oversized, malformed,
  identity-drifted, or digest-drifted frames. Materialization may create only a
  fresh owner-private per-attempt directory with independent shallow Git
  metadata, no remote or active hooks, and safe regular/executable paths;
  links, traversal, reserved/case-colliding paths, hardlinks, and object
  identity drift fail closed. Every manifest entry must be charged against an
  aggregate materialized-output byte budget before writing. The only permitted
  process was one fixed child in the historical protocol-v4/schema-v12 fixture
  forked from the already-running agent, with no `exec`, argument parsing, or
  remote input. Before `fork`, the parent pre-opens the workspace, blocks
  signals, and captures the descriptor-table bound and effective UID. Swift
  must launch
  the agent with an empty environment and the agent must reject local-coding
  startup if it observes a nonempty parent environment. The child scans every
  descriptor slot with `F_GETFD`, closes every open descriptor except the
  workspace and group gate, and waits for the parent-established process group.
  Its fixed mutation path opens and validates only the protocol-fixed relative
  `README.md`, then truncates, seeks, writes, syncs, closes, and exits. Post-fork
  it must not inspect errno or mutable global state, use environment APIs, or
  call `geteuid`, `getdtablesize`, or `setpgid`. Before and after the
  child, the agent hashes every workspace file and rejects any changed path
  outside that singleton allowlist or any output
  other than the fixed fixture bytes. Its bounded path-free result must bind
  the work packet, admission, snapshot, allowed/changed path-set, and patch
  digests; assert exactly one changed file, mutation performed, workspace
  not retained, ambiguity false, and truthfully record `test_status:not_run`.
  Admission evidence must equal the protocol-owned SHA-256 transcript of the
  fixed domain, protocol version as big-endian `u16`, raw context digest, five
  raw UUIDs, connection epoch, sequence, lease duration, and deadline as
  big-endian `u64`; a merely nonzero or differently serialized value fails
  closed in both Rust and Swift.
  Successful proof must remove materialized and transfer state before returning
  `contained_coding_completed`. Cancellation, Emergency Pause through durable
  cancellation, deadline or lease loss, shutdown, restart, and failure
  boundedly TERM-to-KILL reap the child's own process group and remove partial
  state before acknowledgement; late output is suppressed. Snapshot bundle
  reads must retain no-follow/no-reparse handles for
  every state-directory component and the single-link bundle leaf through the
  read. Durable audit/event surfaces remain metadata-only; result acceptance
  retains the bounded payload digest, not repository content or paths. The
  historical protocol-v4 result must name one exact canonical bounded patch artifact whose
  SHA-256 covers its exact bytes. The agent constructs it in memory and cleans
  workspace/transfer state before returning a strict result/artifact pair.
  Swift uploads it during the existing FIFO cancellation race, validates the
  metadata-only receipt, and only then posts the result. Schema v12 must store
  bytes outside SQLite/audit and atomically retain only immutable artifact ID,
  digest, size, exact authority bindings, and redacted audit. Exact retry is
  idempotent; mismatch, replay, missing admission, stale registration,
  cancellation, pause, expiry, lifecycle drift, or restart quarantine rejects.
  Startup may remove only unreferenced artifact directories and must preserve
  referenced ambiguous evidence. Artifact directories and files must be
  owner-private and fixed-shape; reject symlink/reparse traversal, hardlinks,
  wrong owner or group/world Unix access, unexpected children, and path/handle
  identity drift. Preparation guards make exact crash/concurrent retry
  recoverable without cross-request deletion. Startup verifies every referenced
  digest/size/format, and terminal result acceptance re-hashes a stable handle
  immediately before its transaction while retaining handles through it. File
  flush plus same-volume rename is required. Portable Windows directory flush
  is not repository proof and requires live crash-recovery evidence. This
  fixture grants no arbitrary process or tool execution, retained workspace,
  provider, credential or network access, canonical-repository mutation,
  integration, review, publication, queue advancement, or autonomous
  activation. It does not claim a host sandbox or host-level egress control.
- Protocol v5/schema v13 retains every schema-v10 through schema-v12 dispatch,
  snapshot, cancellation, artifact, audit, and authority rule above. The complete
  job frame is at most 16 KiB, its canonical context at most 12 KiB, and aggregate
  replacement bytes at most 4 KiB in both Rust and the production Swift decoder.
  Only exact deterministic `file.write.v1` and `file.delete.v1` are admitted for
  at most 64 sorted normalized relative paths. Every parent directory is opened
  and retained through owner-private no-follow directory descriptors; create is
  exclusive atomic install, replacement is atomic swap plus displaced-inode
  verification, and delete is held-parent identity verification plus `unlinkat`.
  Delete must atomically displace the leaf into a private same-parent capture,
  verify that displaced inode and content, remove only verified entries, and
  atomically roll back mismatch without deleting a same-UID replacement.
  A same-UID pathname/symlink substitution must never redirect a mutation.
  Successful workspaces are sealed without adding metadata inside the attested
  tree. A separate bounded `0600` recovery record binds the exact job, attempt,
  sealed name, post-edit tree digest, expiry, and domain-separated record digest.
  Restart must find exactly one matching pair, re-hash it, reconstruct exact
  cancellation authority, and refuse new admission until exact resolution or
  expiry. Tamper, orphaned/multiple state, bad permissions, or binding drift makes
  startup fail closed. Recovery records remain local private state and never enter
  logs, audit, remote payloads, or SQLite. This adds no integration, test-gate,
  review, publication, lifecycle advancement, canonical-repository, host-sandbox,
  egress, or autonomous authority.
  Windows must not trust Swift for artifact semantics: protocol-owned validation
  decodes admitted bytes and requires exact canonical operations and packet digest
  correspondence to the immutable job. Terminal result acceptance independently
  matches stored artifact ID/digest/size and retained/expiry fields to the validated
  result payload and exact attempt authority. Exact replay is idempotent; any drift
  fails closed.
  Startup may preserve an already-referenced schema-v12 artifact only when its
  bytes are the exact canonical protocol-v4 README artifact and its stored digest
  and size match. That compatibility validator is migration-only and is forbidden
  from new prepare, handle revalidation, result admission, and terminal acceptance.
- Schema v14 artifact integration is a distinct owner-token-authenticated
  loopback-only action and must remain absent from enrolled-device mTLS. A
  companion owner-loopback plan projection may return only the path-free exact
  artifact IDs and authority bindings needed to construct the action; it adds
  no mutation or inference authority and is likewise absent from mTLS. Its
  strict request binds a non-nil integration ID, the complete sorted set of
  terminal accepted artifact IDs, the exact active feature/specification/
  lifecycle/lease/snapshot/base commit, all three current grant revisions, the
  queue revision, and the Emergency Pause revision. The master must reopen,
  re-hash, and independently decode every private artifact against its immutable
  dispatch, require that the supplied set equals the complete terminal accepted
  artifact-backed dispatch set, and derive application order only from immutable
  dispatch ordinal and packet ID. Caller order is never authority. Filesystem
  work must not retain an open SQLite transaction and must use one fail-fast
  reservation that remains owned by detached work after client cancellation,
  plus private staging ownership. The integration repository is an
  independent no-remote copy of the immutable claimed snapshot; the registered
  source checkout must never be opened or mutated by this action. Duplicate
  ordinals, case-folded exact or component-wise file/directory path overlap,
  compare-and-set mismatch, tree-shape conflict, alternate object store, link
  or metadata drift, missing evidence, authority drift, cancellation, pause, or
  restart ambiguity fails closed. Conflict evidence is bounded, path-free, and
  redacted; no partial candidate or lifecycle advance is permitted. Success
  freezes one deterministic exact Git commit/tree, flushes and seals the private
  repository, then atomically rechecks every binding, stores immutable artifact-
  to-candidate evidence, advances only `implementing` to `validating`, and
  appends redacted audit. Stable no-follow handles must bracket source and
  candidate identity verification, and candidate handles remain retained through
  the SQLite commit. Exact retries revalidate the recorded candidate before
  returning the original receipt; integration-ID or request drift rejects.
  Startup removes only unreferenced staging state,
  verifies every referenced candidate repository and commit/tree binding, and
  quarantines ambiguous active state. This grants no test execution, evidence
  gate, review, publication, registered-source mutation, credential, network, or
  autonomous authority.
- Schema v15 test-and-evidence gating is a distinct owner-token-authenticated
  loopback-only action and must remain absent from enrolled-device mTLS. The
  immutable approved manifest, not the caller, owns the exact ordered 13-item
  validation plan: requirements binding, coverage, focused unit tests, native
  E2E, documentation, knowledge base, formatting, lint, build, safety, changed
  paths, secret scan, and repository validation. The strict request must bind
  its canonical plan and request digests to the exact feature/specification/
  `validating` lifecycle revision/lease, snapshot, integration, artifact set,
  candidate commit/tree/base commit, queue and Emergency Pause revisions, and
  all three current repository-grant revisions. Executable, argument, shell,
  path, output, result, and caller-asserted evidence fields are forbidden.
  Immutable SQLite evidence may contain only command identifier, pass bit,
  nonzero result digest, duration, truncation bit, aggregate manifest digest,
  and exact authority bindings; it must contain no raw output, path, match,
  source, or secret. Only all 13 ordered, passing, non-truncated results may
  atomically append redacted audit and exact transition evidence and advance
  `validating -> reviewing`. A recorded failure remains `validating`; missing
  or malformed evidence does not complete. Exact passed retry must revalidate
  the frozen candidate before returning the original receipt, exact failed
  retry returns the same failure, and any validation-ID or binding drift
  rejects. Startup must quarantine active `validating` state as effect-possible
  and must never retry it automatically.
- The schema-v15 production validation runner may activate only when the fixed
  private Windows toolchain and credential-free dependency-cache inputs validate
  at startup. Otherwise `validation_runner_unavailable` must reject before an
  attempt/audit mutation. It may classify only closed protocol command IDs,
  derive path and acceptance scope only from immutable admitted work packets,
  bind that scope to the approved design digest, verify toolchain/cache plus a
  clean no-remote disposable candidate before recording a start, and own fixed
  executable, argv, environment, timeout, output selection, and the 70% minimum
  line-coverage threshold. Its native fixture
  harness uses a restricted token and standard zero-capability AppContainer,
  exact minimal environment and handle list, bounded output, Job Object limits/
  tree termination, temporary root ACL, and profile cleanup. Outside-root and
  loopback TCP/UDP hostile fixtures do not prove service-identity execution,
  real production toolchain/cache staging, credential-store denial, production
  command containment, actual above/below-threshold llvm-cov behavior, signed
  Mac E2E, or OS-wide outbound-egress denial. No live validation activation,
  review, publication, or autonomous authority may be inferred without those
  deployment proofs.
- Schema v16 independent review is a separate owner-token-authenticated,
  loopback-only action and must be absent from enrolled-device mTLS. The caller
  supplies no specification, diff, evidence, prompt, transcript, memory, or
  provider output. The master must reconstruct the approved specification,
  exact frozen-candidate patch, and bounded ordered evidence digests, revalidate
  the candidate before and after the provider call, and bind all content to the
  exact feature/specification/lifecycle/lease, validation/integration,
  candidate/base, provider/model, grant, queue, and Emergency Pause revisions.
  Approved specifications with transcript-, memory-, credential-, or secret-
  shaped content or any field outside the fixed review-safe manifest DTO must
  reject before provider transport, including an explicit review-time recheck
  for supported legacy rows. The exact patch must
  retain add/delete/context polarity. The master must derive the complete
  ordered identifiers from the required top-level approved-manifest
  `acceptance` array; every finding, coverage item, and
  knowledge determination must reference an admitted evidence digest. A
  configured provider must be one fixed canonical executable launched as a new
  cleared-environment process per call, plus the fixed `--count-tokens`
  preflight, with bounded I/O, timeout, and forced complete process-tree
  termination before pipe joins. Windows must hold a verified no-write/no-delete
  executable handle while a trusted gate-blocked launcher enters the kill-on-
  close Job Object before it may spawn the provider; missing or incompatible configuration is unavailable before
  durable mutation. No automatic provider fallback is permitted. Outage, malformed
  output, and incomplete transport must never create a decision or consume a
  repair cycle; they record only bounded immutable failure evidence and the
  fixed 1-, 5-, or 15-minute backoff. Limit calls to three per candidate and
  twelve per feature. Cancellation, pause, lifecycle or authority drift must
  suppress late acceptance; observed interruption, including post-response
  authority drift, must durably terminalize the
  call and quarantine without automatic retry. A structured approval with any blocking finding or
  uncovered requirement fails closed. Rejection must remain `reviewing` with
  the active lease; only exact approval may atomically record the immutable
  decision and advance `reviewing -> publishing`. No review call grants worker,
  repository-write, GitHub, credential, publication, or queue-advance authority.
- Schema v17 publication is owner-token loopback-only and absent from enrolled-
  device mTLS. Its path-free request binds the exact approved decision,
  candidate, specification, evidence, provider/model, grants, queue, Emergency
  Pause, remote base, and branch policy; callers supply no command, path,
  credential, or adapter output. Persist an immutable intent before every
  possible external action. Stage evidence must exactly bind remote base,
  reviewed candidate/head, PR identity, complete required checks, enforced
  branch protection with no bypass, merge strategy/resulting main, and the
  fixed post-merge gate. The adapter must poll a bounded deadline plus current-
  authority/cancellation control before, during, and after effectful work.
  Missing or ambiguous evidence, cancellation,
  pause, drift, or restart after intent creation must quarantine without retry.
  Never bypass branch protection. Only exact merge, remote-main reconciliation,
  and the fixed post-merge gate may atomically release the lease and advance the
  queue. Default-unavailable transport is not live GitHub proof.
- A durable merge intent is effect-possible evidence for abandonment even when
  quarantine originated in `publishing`. Owner resolution must require exact
  healthy-main reconciliation; `merged:false` cannot release that lease.
- Schema v18 orchestration is an internal, default-inert kernel that was
  introduced with no route or activation writer; schema v19 adds the separately
  bounded owner-control authority above. The coordinator derives actions and failure classes only from immutable
  master state and exact admitted evidence; callers cannot provide commands,
  paths, provider/adapter output, credentials, or retry classifications. Its
  path-free checkpoints are immutable, and every mutation commits a redacted
  audit in the same transaction. Provider, worker, maintenance, and owner pauses
  stop the 24-hour active clock. Emergency Pause charges only through the pause
  instant and resume needs a fresh checkpoint to restart the clock. The initial
  candidate is free and at most three replacements may be recorded. Three
  completed calls for one candidate or 12 for the feature require owner
  attention, never another retry. Restart may resume only a complete paused,
  effect-free checkpoint. Any in-flight repository, provider, publication, or
  other effect ambiguity quarantines without retry. Without a protocol-safe
  replacement-candidate contract, substantive failure must move through
  `repairing` to `attention_required` without charging repair or claiming
  completion. Cancellation, failure, attention, quarantine, and abandonment
  never auto-advance; coordination rejects cancellation, attention, and failure
  without mutation. Only exact healthy-main success auto-releases the lease.
  A deliberate owner `abandon-and-advance` may release a cancelled,
  quarantined, attention-required, or failed lease only after exact durable
  resolution-origin and merge reconciliation checks.
- Owner resolution is available only through the owner-token-authenticated
  loopback `cancel-active-feature` and `abandon-and-advance` actions. Both must
  compare-and-set the exact feature, lifecycle, queue, and Emergency Pause
  revisions inside the state transition; a route-level precheck is not enough.
  Cancellation must cancel exact coding dispatches, retain the feature lease,
  set effect-possible state, and never advance. Abandonment must accept only a
  cancelled or quarantined retained lease, require a nonzero safe-
  reconciliation digest and verified healthy-main evidence after any merge,
  and derive merge necessity from its immutable active-origin transition
  receipt, then release the lease and increment the queue revision. Schema v11
  backup-first migrates a retained v10 resolution by backfilling a missing
  receipt only from one exact lifecycle-bound append-only audit event. Missing,
  ambiguous, malformed, or non-active-origin evidence fails closed and restores
  the verified v10 backup. Both may resolve an
  exact paused epoch but must never resume work. They return only fixed path-
  free receipts, remain absent from enrolled-device mTLS, and grant no worker,
  repository, review, Git, publication, or activation authority.
- Local-coding snapshot traffic must use a separate Mac `local-coding`
  Secure Enclave/Keychain namespace enrolled as `InferenceWorker` with exactly
  one `local.coding.v1` capability. It must never reuse the standard owner
  `MacBridge` or fixture identity. Standard and fixture enrollment remain
  `MacBridge`-only, local-coding capability rebind is forbidden, and selecting
  the local-coding relay profile requires the explicit default-off opt-in.
  Before connecting, the Swift supervisor must validate the exact
  `InferenceWorker` role, singleton capability, and production relay. That path
  may authenticate and health-check, then relay, but must not request or emit
  the MacBridge-only Feature Conveyor projection. Partial, mixed, drifted, or
  relayless profiles fail before network use; standard and fixture MacBridge
  observation remains unchanged.
- Remote approved-feature enqueue is permitted only on
  `POST /v1/distributed/feature-conveyor/approved-features`, after a fresh
  exporter-bound application handshake is accepted and revalidated for the
  exact designated device and registry revision. The strict bounded request
  must bind the owner-control schema, current queue revision, current
  designation revision, current Emergency Pause revision, canonical manifest
  digest, immutable approved specification, three existing independent grant
  revisions, and dependencies. Emergency Pause, stale state, absent/currently
  invalid grants, capacity, dependency, digest, duplicate, unknown, malformed,
  or oversized input rejects without mutation using a fixed redacted error.
  Success may only append the immutable specification and queued lifecycle in
  the same transaction as redacted audit. It never claims or leases the queue
  and grants no brainstorming, grant creation, worker, repository, provider,
  review, Git, publication, or activation authority.
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
- By default the long-running helper may request only authenticated MacBridge health, the
  exact bounded Feature Conveyor observation route, and the metadata event
  route. Feature Conveyor status is strictly decoded as schema v9, may appear
  only in authenticated supervisor/app snapshots, and is never forwarded to
  the agent. Unknown, duplicate, extra, oversized, inconsistent, or drifted
  status cancels the session and clears the observation.
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
  Separately, the standard-profile one-shot signed-helper command may send one
  exact already-approved Feature Conveyor request only when invoked with
  `feature-conveyor approve-and-enqueue --confirm` and bounded stdin. It must
  recursively reject duplicate keys and self-dependencies, validate exact
  nonzero digest shapes, accept only an exact redacted revision-bound receipt,
  and close the session after success or failure. Windows alone recomputes and
  validates the canonical manifest digest so Foundation and Rust number
  serialization cannot create a second approval authority. The fixture profile
  and the supervised app
  monitor/relay modes must not invoke this action; the SwiftUI app remains
  read-only.
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
