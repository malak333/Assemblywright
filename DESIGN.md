# Assemblywright Design

This document is the system-level design. Two documents own the detailed
accepted designs and take precedence within their scope:

- [`docs/feature-conveyor-design.md`](docs/feature-conveyor-design.md) — the
  approved Feature Conveyor target and the implemented repository-kernel slice.
- [`docs/distributed-developer-mode-design.md`](docs/distributed-developer-mode-design.md)
  — the accepted authority, security, routing, recovery, and rollout target for
  distributed Developer Mode.
- [`docs/full-machine-assembly-line-design.md`](docs/full-machine-assembly-line-design.md)
  — the approved replacement target for New Project, New Feature, and the
  protected-control-plane full-machine executor. Its reviewed planning/creation
  source slice is implemented but Windows effects remain fail-closed pending
  capability separation; execution and live evidence remain pending.

Current/target boundary: protocol v5 and master schema v22 are current. Schema v20
added strict full-machine Assembly Line protocol contracts plus Windows planning
persistence and authenticated planning routes. Schema v21 adds durable, effect-free
execution-control capability/session/authority/action/checkpoint/termination ledgers
and restart quarantine; it preserves the legacy
schema-v19 Feature Conveyor grant, queue, activation, and restricted-worker
semantics unchanged. The simplified Mac surface consumes the strict owner projection
and can change auto-run through restart-safe exact signed-helper reconciliation.
Source includes a fixed, tool-free Codex adapter, strict Public disclosure, durable
intent, and exact GitHub reconciliation with real cross-process adapter coverage.
For planning calls on Windows, Codex's inner sandbox is `danger-full-access` only
because Assemblywright already supplies the effective boundary: a restricted-token
AppContainer, exact ACL tree, closed environment, pinned executable, and kill-on-close
Job. Each call also receives a create-only private window station and desktop with a
protected DACL containing only LocalSystem, the provisioning owner, and that call's
exact AppContainer profile; the master restores its original station before creating
the suspended child and never modifies `Winsta0`. The closed Codex environment maps
`LOCALAPPDATA` to a distinct validated
provider-private writable root and maps `TEMP`/`TMP` to the validated provider temp,
never to ambient owner paths. The call remains planning-only and every Codex tool
surface stays disabled; non-Windows planning calls retain the inner `read-only` sandbox.
On Windows the master-private data directory contains only a schema-v4 locator/config;
provider, GitHub, and staged-check assets live under the canonical Common Application
Data `Assemblywright/planning-runtime/<runtime_instance>` tree. The two fixed profile
SIDs receive exact non-inheriting traverse-only ACEs on canonical Common Application
Data itself and on the vendor and namespace shared ancestry. Unrelated
parent ACL entries are preserved, while canonical path, directory identity, and exact
profile rights are revalidated at load and before every external call. Each fixed
AppContainer profile is first matched to its deterministic expected SID and then
registered idempotently in the current Windows service identity's profile namespace;
provisioning under the owner identity is not evidence that LocalSystem can launch it.
When `STARTF_USESTDHANDLES` is set, the provider receives valid inherited stdin,
stdout, and stderr handles, and stderr is drained concurrently into a content-free
bounded discard path. Schema v3,
user-profile placement, links, missing traversal, broader rights, or identity drift
keep planning unavailable.
Windows deliberately refuses to load those adapters until capability-separated
identities and ACLs are provisioned and proved. Therefore deployed provider execution
and GitHub creation/reconciliation remain unavailable, as do Start/Stop/Emergency
effect routes, installed executor/broker services, activation, deployment, and
owner-recorded evidence are unavailable. Source now includes strict signed action,
checkpoint, and termination contracts plus separate fail-closed executor and broker
containment crates; their standalone binaries deliberately refuse authority. The
schema-v21 controller is not an effect route: its persistence API verifies capability-
pinned Ed25519 receipts, while production routing, host termination effects, and
installed identities remain cutover gates. Source now includes a separate inert
Windows named-pipe IPC foundation: the Master signs hop-specific Broker and Executor
frames independently, the Broker forwards the exact signed Executor bytes, and each
service returns its own service-secret-signed, path-free zero-effect acknowledgement.
Local-only pipe mode, canonical service-SID peer checks, server-self-SID DACL ownership
for restricted service tokens, identification-only client SQOS, bounded frames, pinned public keys,
and append-only intent/ack journals fail closed on replay, gaps, stale authority,
binding drift, or ambiguous restart. Broker and Executor ServiceMain can host this
foundation only when an optional protected bootstrap names out-of-band secret files;
the seeds never enter serialized runtime configuration or the Broker-to-Executor
frame. This validates health and dispatch shape only. The production dispatcher
remains `UnavailableAssemblyLineEffectDispatcher`, so it grants no Start, Stop,
Emergency, adapter, process-launch, or resource-reservation effect. This
repository source is not an installed, signed, identity-provisioned runtime. Design
or planning approval does not widen
an existing worker or authorize the UI to claim full-machine readiness.

The owner-confirmed native real-Codex probe is not a deployed provider route. It runs
only while the exact SCM-bound master service is fully stopped, revalidates the current
schema-v4 runtime, and uses the unchanged restricted-token provider AppContainer,
private desktop, exact ACL/environment, inherited handles, deadline, and Job. Exact
login status gates one fixed Public structured request. The receipt is metadata-only
(fixed categories, numeric codes, hashes, and binding identities), and the probe writes
no master planning/approval/effect state. Its result is live diagnostic evidence for
that exact run, never external-effect or release proof.

The Mac app has an owner-facing Developer Mode connection flow. It stores only a
strict owner-private locator for the separately signed helper and an independently
supplied Apple team identifier; every supervision, pairing, status, and certificate-
rotation command revalidates the helper and its running process. Public enrollment
and rotation documents remain in memory for an explicit two-host copy/paste ceremony.
The flow reports helper, installed identity, authenticated Windows connection, and
authoritative projections as separate readiness layers. It does not bundle the helper,
move enrollment authority off Windows, persist ceremony documents, or turn connection
health into execution or production readiness.

Full-machine target phase: planning/creation containment has bounded native Windows LocalSystem/AppContainer service proof; execution admission is implemented and effects remain unavailable.

## Understanding Summary

- Assemblywright moves owner-approved feature specifications through bounded
  implementation, deterministic proof, independent review, and verified
  publication.
- Exactly one owner holds authority. Models and workers may propose, but they
  cannot enqueue, reorder, cancel, abandon, grant authority, or rebind
  providers.
- Implementation runs on restricted local coding agents against credential-free
  repository snapshots. A frontier model may assist with planning and final
  review, and never receives repository-write or implementation authority.
- The Windows master is the sole durable authority for the queue, repository
  policy, audit, and publication. The macOS app is the owner's control and
  observation client.
- Autonomy is bounded by capability scopes, risk tiers, redacted audit
  evidence, explicit cancellation, and an emergency pause control.

## Assumptions

- Single owner, two supported machines: a Windows master and an Apple-silicon
  Mac worker.
- Local inference happens on the Mac through a bounded, no-retention MLX lane.
- Product-grade work includes packaging, diagnostics, migrations, durable local
  state, and release discipline.
- Production-readiness claims are evidence-scoped. A green local gate is not
  finished readiness until Developer ID signing, notarization and stapling,
  clean-profile install and Finder launch, live-device QA, and the final
  evidence bundle are owner-recorded and archived for the claimed surface.
- Architecture docs are release artifacts: keep the current-state map aligned
  with any release-evidence flow change.

## Non-Goals

- No cloud model implementing repository changes.
- No automatic backlog generation, replenishment, or model-controlled ordering.
- No more than one active feature, even across repositories.
- No silent interpretation of ambiguous or contradictory specifications.
- No automatic provider fallback or automatic active-feature rebinding.
- No general capability rebind from the model picker. The only selectable lane
  is the exact designated singleton MLX MacBridge, and only its model ID may
  change.
- No automatic advancement after cancellation, failure, attention, quarantine,
  or abandonment.
- No peer-to-peer worker authority, shared writable worker checkouts, or worker
  Git publication credentials.
- No general-purpose assistant surface. Conversation, personal memory,
  scheduling, voice, and a plugin marketplace were removed with the pivot to
  Developer Mode and are not planned.

## Architecture

The approved next architecture keeps Windows as the sole durable authority but
replaces manual repository onboarding and restricted feature execution only after
its gated cutover. A canonical GitHub URL becomes the owner-facing repository
identity. New Project and New Feature use one explicitly selected, allowlisted,
planning-only orchestrator to produce a frozen specification. Owner approval is
required before GitHub repository creation or queue insertion. Starting a nonempty
Assembly Line creates the execution grant; auto-run defaults on and derives at most
one active feature epoch at a time.

Full-machine effects are performed by signed, unprivileged local executors through
separately signed privileged brokers on Windows and Mac. The sole scope exception is
the protected Assemblywright control plane: master and broker identities, binaries,
service definitions, IPC, configuration, authority databases and backups, audit,
tokens and keys, trust/update roots, enforcement state, and enforcement resource
reservations are never feature targets. The planning orchestrator receives neither
machine-write authority nor restricted data. Until this complete target is activated,
the current restricted local-coding boundary below remains unchanged.

The containment source slice keeps that cutover closed. Schema-v1 Ed25519 action
envelopes bind the session/child epoch, monotonic sequence and nonce, executor and
broker identities/revisions/executable digests, a digest of the complete protected
closure, typed action, exact target and parent identities, operation and working-
directory digests, environment-key names only, deadline, cancellation, effect class,
and reconciliation strategy. Broker policy rejects protected descendants, case aliases,
symlinks/reparse points, hard-linked files, drift, gaps, and replay. The first Windows
`CreateDirectory` adapter now exists only behind a one-shot native proof seam: it holds
the complete target ancestry without delete sharing, creates one absent leaf relative
to the retained parent handle, and returns either path-free applied evidence or an
effect-possible reconciliation outcome. Any uncertain completion after entering the
native create call, including a negative status, is treated as effect-possible and
quarantines the proof policy. The long-running broker runtime still validates
and consumes dispatches without executing them. Its authenticated Windows IPC
foundation durably records intent/ack sequence state and can recover or replay only
one byte-exact inert request, but active-effect termination and adapter-specific
durable reconciliation remain unwired. The
unprivileged executor validates held macOS executable/cwd and signed parent/object
identities, but macOS spawn remains disabled because a hostile descendant can escape a
process group with `setsid`/`setpgid` and Darwin exposes no unprivileged Job-Object-like
reaper. Windows source retains no-reparse path handles, verifies the suspended image,
assigns a kill-on-close Job, rechecks authority, and only then resumes; it remains
uninstalled and unavailable through product routes pending installed-host activation proof.
The Windows spawn path also derives bounded CPU, aggregate Job commit, and active-
process limits from host capacity, rounds the commit cap down to the native page size,
installs them before creating the suspended child,
and queries the exact limits again after assignment and inside the authority-locked
resume operation. This runtime enforcement is not a protected policy-file binding or
proof that installed control-plane reservations exist.
Broker file replacement, removal, service adapters, signed OS installation/identity,
production effect dispatch, active-effect replay restoration, and control-plane resource
reservations remain unavailable. Authenticated master control routes exist behind the
unavailable effect dispatcher; their existence grants no host effect.

The implemented schema-v20 slice is deliberately inert. Project approval records an
effect-free `creation_pending` intent (Public by default, with Private selectable) and
does not call GitHub. A feature can enter the new FIFO only after the repository is
`created` with creation evidence. Auto-run is stored as default-on and may be changed
through a compare-and-set request, but it cannot start work. Every runtime component
in the Assembly Line availability projection is `unavailable`; Start, Stop, and
Emergency Pause routes reject while the production effect dispatcher remains unavailable.

Schema v21 adds the durable execution-control ledger without activating it. Schema v22
adds once-only dispatch claims and signed activation receipts. Start admission, FIFO
child-epoch state, authority revocation, checkpoint/termination evidence, replay safety,
and restart quarantine are represented in master-owned state. Authenticated local and
distributed control routes call an explicit dispatcher boundary, but production installs
the unavailable dispatcher and rejects those routes before decoding or mutation.
The separate inert host IPC cannot launch or terminate a feature process.

The master library also contains a catalog-bound, idempotent planning-effect
coordinator exercised only through private fake-adapter unit tests. It records pre-call intent,
requires exact reconciliation, and remains disconnected from HTTP/runtime startup.
No credential-owning production provider or GitHub adapter is installed, so runtime
availability remains `unavailable` and the corresponding UI actions remain disabled.

### Owner-confirmed local model selection

Developer Mode exposes one bounded Models surface. Mac MLX is selectable;
`assemblywright-local-coding-v1` and the production `openai.codex` /
`gpt-5.6-sol` reviewer remain fixed, while Windows RTX is reported as not
provisioned. One explicit Mac confirmation freezes the exact device, current
device-registry revision, owner-control designation revision, Emergency Pause
revision, and target model. Local `mlx_lm.generate` and model-directory paths
remain only in a mode-0600 owner-private Mac configuration file and never enter
the protocol, Windows database, status, or audit. Selectable model IDs are
bounded printable ASCII without whitespace or control bytes so the picker
cannot create a registration that the relay cannot safely reproduce.

Windows accepts the model-only POST only from the exact current accepted epoch
of the designated non-fixture MLX MacBridge, outside maintenance and Emergency
Pause, with no active attempt. The transaction copies device/name/role,
capability ID/kind/provider/bounds, certificate and key identity unchanged;
advances the device registry, designation registry, and designation revisions
by exactly one; disconnects the accepted epoch; and appends one metadata-only
redacted audit row. Protocol v5 and schema v19 remain compatible because no
certificate, capability shape, database table, or general rebind contract
changes.

The signed helper has only `local-model select --confirm` and the separately
confirmed `local-model reconcile --confirm`. Only strict known pre-mutation
HTTP status/error pairs become a path-free terminal-rejection result bound to
the frozen intent; the app durably clears pending and restarts the unchanged
relay for that result. Unknown bodies/statuses and all 5xx responses remain
ambiguous. Resume first proves the target; if Windows instead proves the
byte-exact old unpaused binding, it may submit the frozen original POST once.
The Mac fsyncs the pending file and its parent directory before POST. Unsafe,
malformed, oversized, or path-invalid stores block startup. A promoted relay
configuration is validated before active state replaces pending on disk. There
is no provider or model fallback.

The pending store is recursively duplicate-free and exact-shape validated.
Its configuration model ID and zero pre-selection registry revision must bind
the canonical frozen request bytes exactly, so a stored request for one model
cannot be promoted with another model's local directory. After a lost committed
POST, acceptance of the target-profile application session proves the old
profile stale. Emergency Pause therefore leaves pending untouched and permits
no local identity install, relay promotion, POST, or old-profile probe. Once
unpaused, exact target registry/device/name/model evidence may reconcile across
monotonically newer designation and Emergency Pause revisions; any lower,
paused, malformed, or otherwise drifted target remains fail-closed. Protocol v5
and schema v19 remain unchanged.

The ordinary Keychain-backed TLS factory remains exact-installed-profile-only.
Reconciliation uses a separate internal transport operation: the factory
reloads and matches the frozen old profile, recomputes and compares the sole
legal target (registry revision exactly plus one, exact requested model, and
otherwise identical identity/name/role/endpoint/certificate and capability
identity/kind/provider/bounds), loads the old TLS material, then rechecks the
old profile before connection setup. There is no caller-controlled transition
flag or arbitrary override, and target identity is not installed before exact
Windows application-handshake and projection proof.

### Windows master

`assemblywright-master` owns durable state and every authority decision. Its schema-v19
SQLite database holds two kernels:

- The distributed device lifecycle: registered devices, connection epochs,
  queued steps, leased attempts, cancellation and expiry outcomes, accepted
  payload digests, and a metadata-only event journal with one server-issued
  stream ID and contiguous sequence.
- The default-inert Feature Conveyor repository kernel: immutable approved
  specification revisions, three independent repository grants, the bounded
  owner-ordered queue, one active lease, exact lifecycle advancement, and
  startup quarantine. Its bounded and redacted lifecycle-observation
  projection is available through the owner-token-authenticated loopback
  `GET /v1/feature-conveyor/status` and, without an owner token, through the
  dedicated `GET /v1/distributed/feature-conveyor/status` only after an
  exporter-bound application session is accepted for an enrolled MacBridge. A
  fixed-enum guidance object bound to queue, Emergency Pause, and optional
  feature lifecycle revisions distinguishes idle, ready,
  dependency-blocked, active, reconciliation-required, and emergency-paused
  states and names one display-only next owner action. It does not establish
  claimability or expose a callable action. Other enrolled-device roles are
  denied, and neither observation route adds mutation, audit, worker,
  repository, provider, publication, or owner-action authority.

Schema v19 adds a separate strict owner-control projection only for the exact
current designated non-fixture MacBridge after an exporter-bound accepted
session. It contains fixed lifecycle, orchestration, readiness, blocker,
revision, ID, and digest-reference fields; paths, content, commands,
credentials, raw evidence, and provider output are forbidden.

Schema v19 keeps orchestration globally inert until one immutable singleton
activation. The six prerequisites are one deterministic repository-gate proof
and five separately admitted live-proof-controller receipt digests: restricted
worker, review provider, GitHub publication, restart recovery, and Mac/Windows
control plus event streaming. Only the owner-token loopback route may admit a
strict category/origin-bound receipt reference, with contiguous revision,
Emergency Pause binding, non-future observed time, and same-transaction
redacted audit. The designated bridge may only authorize activation against the
exact current six Windows-admitted references plus queue, designation, and
Emergency Pause revisions. Activation is global, creates no lease or action,
and exact retry returns the original immutable receipt.

The first proof-controller slice is an owner-run macOS/local repository-gate
controller, separate from the schema-v19 admission route. Its `--run` mode has
no repository or command arguments: it accepts only the controller's own exact
clean `main` checkout when `HEAD` equals `refs/remotes/origin/main`, captures
the branch, HEAD, tree, status, and committed `scripts/release-local.sh`
definition before and after executing only those exact committed blob bytes
through argument-free Bash stdin, and fails closed on any drift or interruption. A
success produces one bounded path-free schema-v1 JSON receipt plus its raw
SHA-256 sidecar under the fixed ignored `target/repository-gate-proof`
directory, using owner-only permissions and temporary-file rename with the
receipt as the final commit marker. The receipt binds the protocol category
`repository_gate_proof`, origin `repository_gate_proof_controller`, HEAD and
tree object IDs, committed gate-definition digest, fixed gate identity,
millisecond observed time, pass status, and an explicit narrow proof boundary.
Failure and handled cancellation remove partial or final output. The
controller never POSTs or admits the digest, writes source, activates, or
claims signing, notarization, live-device, restricted-worker, review-provider,
GitHub-publication, restart-recovery, or Mac/Windows-control proof. Later
owner-token loopback admission remains the Windows-authoritative, atomically
audited approval boundary. This adds no protocol or database-schema change.
The repository `target` and proof-directory components must be ordinary,
owner-matched local directories rather than symlinks; an ambiguous component
is rejected before cleanup or creation. A new `--run` deliberately invalidates
only the exact prior fixed receipt pair after those directory checks, so a
failed current attempt cannot leave a stale file that appears to describe it.
Every Git observation runs at the fixed controller root with an empty inherited
environment, disabled replace objects, and no external config, alternate object
directory, index/work-tree redirection, or hook/fsmonitor authority. The
index must report the normal `H` tag for every tracked entry; assume-unchanged,
skip-worktree, and every other hidden tracked-state tag fail before execution.
The clean/stable checks edge-detect concurrent same-UID mutation of other files
consumed by the committed gate, but do not claim host isolation from that owner.
The committed gate receives only a narrowly recognized internal stdin/root marker;
caller-supplied copies are cleared, the marker never selects another public
controller root, and `release-local.sh` unsets it before its command list.
The controller holds the validated output directory as its current directory,
binds both `target` and output device/inode identities, rejects a group/world-
writable `target`, revalidates the path and held identities before and after
relative-name publication, and never cleans through a replaced path. Gate
execution has a separate process group. Gate failure terminates that group
immediately. Only after successful leader exit, a short bounded natural drain
permits already-exiting members to disappear; a member still present at the
deadline fails proof. HUP, INT, or TERM propagates to the
complete group, escalates boundedly to KILL, reaps the controller-owned leader,
and suppresses the receipt if any child survives or the gate fails.

The second proof-controller slice is the owner-supervised Mac/Windows
restricted-worker controller. Its `--run` mode accepts no repository, remote,
worker, executable, work packet, or alternate harness. It requires the
controller's exact clean `main` at `refs/remotes/origin/main`, executes only the
committed `scripts/mac-windows-bridge-live-e2e.sh` bytes through Bash stdin, and
uses a separate inherited descriptor for the sanitized Windows-local receipts
so live coordination cannot replace the committed script. The fixed harness
uses the independently signed Swift relay, the separately enrolled singleton
`local.coding.v1` `InferenceWorker`, and the real Rust agent against the
schema-v19 Windows master. One protocol-v5 bounded packet is snapshot-bound,
leased, executed in private Mac state, independently artifact-validated on
Windows, integrated into a detached remote-free candidate, observed as one
retained workspace/record pair, then explicitly cancelled, safely abandoned,
and cleaned with the queue, lease, distributed active state, transfer staging,
temporary grants, and disposable checkout empty or terminal. Only one exact,
ordered set of six action markers followed by one bounded success record is
accepted. The complete private transcript is hashed and deleted; late duplicate,
extra, or reordered action markers reject. Before enqueue, the Windows control
writes, flushes, validates, and same-directory atomically renames an owner-private
marker containing the random repository/feature IDs and
exact queue, pause, designation, source, and commit baseline. A repeated Prepare
resumes only missing exact revision-1 grants and reconstructs the identical
approved request. If enqueue committed but its helper receipt was lost, the
marker plus the sole exact queued lifecycle-revision-1 feature recovers the
committed queue revision without a second enqueue. All other queue, authority,
grant, repository, or lifecycle state fails closed. The controller then rechecks
the branch, HEAD, tree, status, origin, and both
committed live-control definitions, and atomically publishes only a path-free
schema-v1 receipt plus raw digest under the fixed ignored
`target/restricted-worker-live-proof` directory. Its receipt binds category
`restricted_worker_live`, origin `restricted_worker_proof_controller`, the
exact commit/tree, both controller-definition digests, the transcript digest,
fixed proof identity, observed time, pass status, and a narrow claim boundary.
Failure, drift, malformed or duplicate success, a surviving descendant, and
handled cancellation leave no receipt. A rerun invalidates the prior fixed
pair only after the ordinary owner-matched output chain is validated. This is
same-owner functional live proof, not a portable host sandbox or OS-wide egress
claim, and it never admits evidence, activates, or proves review provider,
GitHub publication, restart recovery, Mac/Windows control streaming, signing
distribution, notarization, clean-profile installation, or production readiness.
Protocol v5 and schema v19 are unchanged.

The third slice is the selected review-provider live integration and proof
controller. Windows provisioning is fixed to `openai.codex`, `gpt-5.6-sol`,
Codex `0.148.0`, the owner-private `C:\Users\mike\.codex` authentication home,
and committed adapter/output-schema bytes. The schema-v16 production provider
loader verifies the adapter, Codex, and output-schema digests and ordinary-file
identities, requires an owner/SYSTEM-only protected Codex auth DACL,
holds non-delete-share handles across each invocation, clears the adapter
environment, and launches every response-only provider process through the
existing pre-spawn Windows Job Object gate. The adapter then clears the Codex
environment, supplies only `CODEX_HOME`, enables strict configuration parsing,
forces ephemeral read-only execution,
disables shell, agent, skill-install, image, and web surfaces, and strictly
validates every packet/output binding before returning JSON. The owner-run
`scripts/review-provider-proof-controller.sh --run` accepts no provider, model,
executable, repository, schema, or harness selector. From exact clean published
`main`, it executes the committed native harness with Windows coordination on a
separate descriptor and requires one fixed valid candidate to be approved and
one fixed invalid candidate to be rejected. Only a path-free receipt and digest
are atomically published under `target/review-provider-live-proof`; the private
transcript is hashed and removed. This is selected-provider integration and
fixed semantic sanity proof only. It does not admit activation evidence,
mutate the queue, prove general reviewer competence, exercise feature gateway
lifecycle, publish to GitHub, or establish production readiness. Protocol v5
and schema v19 are unchanged.

The fourth slice is the fixed GitHub-publication live integration and proof
controller. The schema-v17 Publication Coordinator remains the only durable
publication authority. Its Windows production adapter is unavailable unless
the owner has provisioned the exact pinned master, `gh.exe`, and `git.exe` identities,
an owner/SYSTEM-private GitHub CLI configuration, and the fixed public
`malak333/Assemblywright` repository policy. Internal check identifiers map
only to the two committed GitHub Actions contexts, workflow IDs and paths, and
trusted workflow-file digests at the checked commit. The adapter pushes the exact
frozen candidate from the master-owned repository, creates or recovers the
single exact pull request, observes strict `main` protection with administrator
enforcement and no bypass, waits for the complete required-check set, rechecks
the reviewed head, performs only the configured normal protected merge,
reconciles exact remote `main`, and observes the fixed post-merge
`release-local` check. Credentials remain in GitHub CLI secure storage and are
never request, argv, environment, file, audit, log, or receipt data. The master
persists an immutable intent before each possible effect, releases its process
lock during external work, monitors current authority and cancellation, and
quarantines every ambiguous or late result without retry.
Provisioning stops the exact service and waits for its process to exit before
building. Because Cargo may hard-link the release output into `deps` and leave
checkout ACLs on it, the control copies those exact bytes into a fresh
single-link file, protects it to the owner and SYSTEM, verifies the unchanged
digest, and only then installs and health-checks it. Rollback applies the same
single-link/private-ACL boundary while restoring the prior bytes. The recovery
copy is link/owner/ACL/digest-verified before the service is stopped.

The owner-run `scripts/github-publication-proof-controller.sh --run` starts
only from exact clean published `main`, executes only the committed native
harness bytes, and receives one sanitized Windows result on a separate
descriptor. The live fixture creates a disposable no-remote working repository
with one bounded metadata-only proof-marker commit, then uses the same
production adapter to push, open, check, normally merge, reconcile, and
post-merge-check that metadata-only marker commit through protected GitHub
`main`. Because the proof intentionally
advances `origin/main`, the controller requires the source checkout itself to
remain unchanged while the fetched remote advances to the exact reported merge
commit. It hashes and deletes the private transcript and atomically publishes
only a path-free owner-private receipt/digest pair under
`target/github-publication-live-proof`. The controller never admits activation
evidence or activates orchestration; its activation origin is the fixed
`github_publication_proof_controller` identifier. This proves the fixed GitHub credential,
push, pull-request, hosted-check, protection, merge, and reconciliation path;
it does not prove a queued feature publication, general repository support,
signing, notarization, clean-profile installation, or production readiness.
Protocol v5 and schema v19 are unchanged.

The fifth slice is the fixed restart-recovery proof controller. The owner-run
`scripts/restart-recovery-proof-controller.sh --run` has no repository,
service, data-directory, executable, test, Windows-control, or harness selector.
It starts only from exact clean `main == refs/remotes/origin/main`, verifies
normal tracked-index state and the committed controller, harness, and Windows-
control blobs, pins the system Git plus owner-host Cargo and rustc paths with
exact ordinary-file identities and digests, validates the fixed Cargo home,
rejects Cargo configuration throughout the checkout ancestry, and clears
caller tool/PATH/build overrides. It then executes only the committed harness
bytes through the absolute system Bash with a system-only PATH. The harness
runs those exact Rust tools and the existing real-process Rust-agent E2E for a
retained workspace/record pair, agent termination and restart, exact
cancellation cleanup, and partial-state restart cleanup. One strict sanitized
Windows receipt arrives on a separate descriptor, never as executable input.

The fixed Windows control requires deliberate `Run` confirmation and serializes
operations. It binds the controller-reported commit to exact clean Windows
source, the literal service/data paths, the SID-bound local `MIKE-PC\mike`
service owner (including SCM's exact `.\mike` display alias), and the source-
checkout release image. It accepts the extended-length path namespace only
when the executable and data-directory arguments use it consistently. It stops
the service to freeze authority,
requires no SQLite sidecar, hashes and integrity-checks the complete database
and bounded migration backups, then rebuilds the exact expected HEAD offline
with the fixed absolute Cargo/Rust/MSVC toolchain into owner/SYSTEM-private
state. The rebuilt ordinary single-link executable must preserve the Cargo
output digest; that exact-source rebuild digest is bound separately because the
Windows link output is not asserted reproducible against the installed image.
It performs a bounded healthy start with PID turnover, stops again, requires
the complete frozen database SHA and logical continuity to be unchanged,
restores the original exact executable, and proves a distinct final healthy PID
with protocol 5, schema 19, Emergency Pause clear, and empty distributed and
Feature Conveyor state. Failure attempts exact healthy service restoration and
emits no receipt. The schema-v2 JSON binds the pinned Cargo, rustc, MSVC,
original/restored service-image, transient exact-source rebuild, and frozen-
database digests without paths.

The installed image is a deployment prerequisite, not controller-created
trust. A direct Cargo release output may share an inode with its `deps` artifact
and inherit broader checkout ACLs; it is ineligible until a stopped-service
deployment preserves the exact bytes in a new single-link file protected to the
owner and SYSTEM, then restores healthy service operation. `Check` observes and
rejects this drift without mutating it.

The Mac controller hashes and deletes its private transcript, rechecks the
source and committed definitions, and atomically publishes one owner-private
`0600` receipt plus raw SHA-256 sidecar under
`target/restart-recovery-live-proof`. The receipt binds category
`restart_recovery_live` to origin `restart_recovery_proof_controller`. Failure,
drift, malformed evidence, unsafe output identity, cancellation, or a surviving
process-group member invalidates prior output and leaves no current pair. This
proves retained-agent functional recovery plus idle authoritative-service
continuity. It does not prove reproducible builds or the original installed
image's source provenance. Active master startup ambiguity remains fail-closed and is covered
by exact native quarantine tests; this slice does not prove recovery of an
active published effect, SCM retry policy, the signed helper, Feature 6
streaming, evidence admission, activation, signing, notarization, live-device
installation, or production readiness. Protocol v5 and schema v19 are unchanged.

The sixth slice is the owner-run Mac/Windows control-streaming proof controller.
`scripts/mac-windows-control-streaming-proof-controller.sh --run` starts only
from exact clean published `main`, normal tracked-index state, and stable
committed controller plus `mac-windows-bridge-live-e2e.sh` definitions. It
clears caller executable, tool, PATH, relay, model, Git, and internal overrides,
then executes only the committed harness bytes through fixed Bash stdin in
`--run-relay` mode. The native boundary is the independently signed Swift
helper, exporter-bound mTLS owner-control projection, exact signed Rust agent,
and durable same-stream cursor that advances after a fresh helper/agent chain.
The controller accepts exactly one ordered terminal relay/bridge marker pair,
hashes and deletes the private transcript, and never retains its endpoint or
stream ID.

The path-free schema-v1 receipt binds category
`mac_windows_control_event_streaming_live` to origin
`mac_windows_control_event_streaming_proof_controller`, exact HEAD/tree,
controller/harness definitions, fixed helper/agent executable and signature
identities, transcript digest, time, pass, and the narrow boundary. It is
published digest-first and receipt-last as owner-private `0600` files in a held
owner-only `target/mac-windows-control-streaming-live-proof` directory. Source,
definition, binary, output, marker, cancellation, or descendant drift fails
closed and leaves no pair. This controller produces evidence only: it never
admits evidence, approves, activates, or changes protocol v5, schema v19, or
runtime authority. Built-binary current-source linkage, Developer ID
distribution, notarization, clean-profile installation, unattended operation,
and production readiness remain separate claims.

The seventh slice is the Windows owner evidence-admission/onboarding tool.
`scripts/windows-owner-evidence-admission.ps1` reads only an owner/SYSTEM-
protected ordinary receipt and its exact raw SHA-256 sidecar. It holds both
non-reparse files without write/delete sharing, binds their canonical paths and
stable identities, and revalidates those paths, identities, and ACLs before and
after handle-based reads and validation. It strictly binds the fixed filename
pair, exact controller field order and one-line bytes, category/origin/schema,
controller identity, pass status, non-future observation time, commit/tree and
digest fields, then sends only category, origin, a fresh evidence ID, contiguous
revision, receipt digest, observation time, and the current Emergency Pause
revision. Its authenticated preflight is the owner-token loopback-only
`GET /v1/feature-conveyor/activation-evidence`; the same path's existing POST
remains the sole mutation. The token is read from protected Windows master
state and never enters argv, environment, output, or a receipt. Exact current-
digest preflight makes a retry idempotent even if pause or activation followed
an ambiguous committed POST; otherwise stale revision, pause, activation,
unsafe files, malformed evidence, or digest drift fail closed. `Admit` requires
explicit `-Confirm`, never retries a POST automatically, and never activates.
There is no enrolled-device route, protocol-v5 change, schema-v19 migration, or
new cancellation authority; interruption before the atomic POST commits
nothing, while an ambiguous response is reconciled by a fresh read-only
preflight.

The designated bridge may explicitly owner-pause or resume only an exact
active, effect-free orchestration checkpoint using lifecycle, orchestration,
queue, designation, and Emergency Pause revisions. Owner pause stops the active
clock and resume starts it from a fresh checkpoint. Existing cancel and
abandon-resolution actions use the same exact remote boundary and retain their
no-advance and safe-reconciliation rules. Global Emergency Pause remains
separate and safety-dominant.

Compatibility note: schema-v18 introduced the internal orchestration ledger;
schema v19 migrates that ledger backup-first and adds its first authoritative
activation writer without changing protocol v5.

Schema v8 adds one nullable, compare-and-set, Windows-authoritative owner-control
bridge designation. Its owner-token-authenticated loopback mutation accepts
only a current, enrolled, non-fixture `MacBridge` and commits a redacted audit
event atomically. Separate owner-token-authenticated loopback repository-grant
routes record one strict digest-only, contiguous, compare-and-set revision and
inspect only the current registration, cloud-disclosure, and autonomous-
publication revisions. The mutation is bound to the Emergency Pause revision;
active grants are blocked while paused, revocation remains available, and audit
failure rolls back the revision. These routes never inspect a repository and
are absent from the enrolled-device router. A separate owner-token-authenticated
loopback preflight accepts one strict scope document whose canonical digest and
revision must match the exact current active registration grant. The master
performs only a filesystem identity check of a canonical non-symlink fixed-
volume repository, standard `.git` and objects directories, symbolic HEAD, and
exact loose ref for a single-component branch. On Windows, non-reparse handles
stabilize that complete path and identity chain; the master reopens and compares
the canonical pathname and identities immediately before the final grant,
Emergency Pause, and audit transaction recheck, then retains the fresh handles
through that transaction. It
executes no Git process, loads no repository config
or attributes, rejects network/device/reparse/worktree/submodule paths, and
does not establish clean working-tree or object-content state. It retains no
path or repository content and returns only a path-free point-in-time digest
receipt after the
grant and Emergency Pause revision are rechecked with a same-transaction
redacted audit. It does not create a snapshot or establish claimability.

Schema v9 adds one owner-token-authenticated loopback-only snapshot-claim
transition. A strict request binds the exact queue head, immutable
specification, repository scope, branch and base commit, all three grant
revisions, provider/model, queue revision, and Emergency Pause revision. The
master performs the database precheck without retaining a transaction across
filesystem work, revalidates the held repository identity immediately around
snapshot creation and again before finalization, then atomically rechecks every
binding, records immutable path-free snapshot evidence, creates the singleton
lease, advances to `implementing`, increments the queue revision, and appends a
redacted audit. Snapshot construction reads raw Git objects through a library,
not a Git process: source config, hooks, attributes, credentials, global config,
PATH lookup, alternates, symlinks/reparse entries, gitlinks, and remotes are not
trusted. It copies only the exact base commit and its current tree/blob graph,
marks that commit shallow, and excludes parent history and deleted historical
objects. Every commit/tree/blob read first validates the ODB header type and
declared size and charges the aggregate budget before decompression/allocation.
The result has independent Git metadata, raw current committed content,
no remote, and a SHA-256 content binding. One fail-fast singleton reservation
serializes claims. Snapshot work is capped at 50,000 objects, 256 MiB total and
32 MiB per blob; the HTTP wait is 30 seconds. A timed-out blocking thread is not
forcibly cancelled: it retains the reservation and RAII snapshot ownership until
it exits. Failure or request cancellation before
finalization removes the unreferenced snapshot; startup removes abandoned
unreferenced UUID directories and quarantines a finalized active lease as before.
The route adds no worker dispatch, provider invocation, review,
GitHub/publication, Mac mutation, remote mTLS mutation, or autonomous
activation. The separate remote
`POST /v1/distributed/feature-conveyor/approved-features` remains unusable until
the caller is the exact current designation on an accepted, revalidated,
exporter-bound session. Its strict request binds one already-approved
specification to queue, designation, and Emergency Pause revisions. It may only
append the immutable specification and queued lifecycle; it adds no claim,
dispatch, worker, repository, provider, review, Git, publication, or activation
authority.

Schema v10 originally added one separate owner-token-authenticated loopback-only coding-
dispatch transition. It accepts only bounded path-free metadata and binds one
work packet digest to the exact active feature, specification and lifecycle
revision, feature lease, snapshot ID and digest, queue and Emergency Pause
revisions, and exact current non-revoked `InferenceWorker` registration with
the singleton `local.coding.v1` capability. The transition atomically inserts
one existing distributed queued step, immutable dispatch evidence, the
distributed event, and redacted Feature Conveyor audit. The enrolled-device
router does not expose this owner action. Remote leasing and result acceptance
recheck the exact device, registration, feature lifecycle, feature lease,
snapshot, queue, and pause binding; cancellation, Emergency Pause, lifecycle
departure, or startup quarantine prevents later acceptance. A separate
default-off distributed route may then serve bounded sequential chunks of the
immutable snapshot bundle only for that exact current attempt, lease,
cancellation identity, snapshot ID, and snapshot digest. Authority is rechecked
both before and after each filesystem read. The Mac bridge strictly validates
every response and forwards it over the authenticated local socket to the
native agent, which reconstructs the exact raw-object graph and safe regular or
executable files in a fresh private per-attempt directory, verifies object,
chunk, bundle, path, and aggregate digests, and charges every manifest entry
against an aggregate materialized-output byte budget before writing it. For the
historical protocol-v4/schema-v12 fixture, it forked exactly one deterministic
child from the already-running agent with no `exec` and no remote input. The
Swift parent launches the agent with an empty environment, and the agent refuses
local-coding startup if that parent environment is nonempty. Before `fork`, the
agent opens the workspace, blocks signals, and captures the descriptor-table
bound and effective UID. The child scans every descriptor slot with `F_GETFD`,
closes every open descriptor except the workspace and process-group gate,
waits for the parent-established process group through that gate, and then uses
the fixed `openat`/`fstat`/`ftruncate`/`lseek`/`write`/`fsync`/`close`/`_exit`
file-mutation path to replace the protocol-fixed relative `README.md` fixture.
Post-fork it does not inspect errno or mutable global state, use environment
APIs, or call `geteuid`, `getdtablesize`, or `setpgid`. The parent
hashes the fixed allowed/changed path set and before/after patch evidence, reports
one changed file with `test_status:not_run`, and removes both materialized and
transfer state before returning the path-free `contained_coding_completed`
result. Its admission digest is protocol-owned and hashes the fixed domain,
protocol version as big-endian `u16`, context digest, five raw UUIDs, then
connection epoch, sequence, lease duration, and deadline as big-endian `u64`;
Rust and Swift recompute the same transcript rather than accepting any nonzero
value. Cancellation, Emergency Pause through distributed cancellation, lease
or deadline expiry, shutdown, restart, malformed or out-of-order chunks,
identity drift, links, duplicate/colliding/unsafe paths, unexpected file drift,
trailing data, and digest or size drift fail closed; the child's own process
group is boundedly TERM-to-KILL reaped before cleanup and no late result is
accepted. During final verification, the Swift cancellation task sends the
local cancellation immediately, then cancels the in-flight Unix request; the
relay marks cancellation in progress before the local call, so an expected
final-chunk rejection that races just ahead of the cancellation acknowledgement
waits for and validates that acknowledgement while every rejection without an
in-progress cancellation still fails closed. The native E2E requires cleanup
before acknowledgement, no result post, and an
acknowledgement strictly inside two seconds. The dispatch approval
authorizes only this fixed fixture, not an arbitrary command, tool, executable,
path, provider, test, or network/credential access. This slice claims no host
sandbox or host-level egress enforcement and adds no retained workspace,
canonical-repository mutation, commit, integration, review, publication, queue
advancement, or autonomous activation.

Historically, schema v12 added result-artifact admission without adding
integration authority.
The agent constructs one protocol-owned canonical `README.md` replacement
artifact in memory and hashes its exact bytes, then removes workspace and
transfer state before returning the strict result/artifact pair. During the
existing cancellation race, Swift strictly validates both documents and uploads
the artifact over the FIFO exporter-bound `InferenceWorker` session before it
posts the metadata-only result. The remote route rechecks the exact current
device registration, connection epoch, task, step, attempt, lease,
cancellation, feature, snapshot, work packet, lifecycle, queue, pause, and
expiry bindings. Artifact bytes live only in the private Windows master state
directory; SQLite stores immutable ID, digest, size, and bindings with no-update
and no-delete triggers plus same-transaction redacted audit. Exact retries are
idempotent, while mismatch, replay, stale identity, cancellation, pause,
lifecycle drift, or missing admission fails closed. Result acceptance now
requires that exact admitted artifact. Startup removes only unreferenced
artifact directories; referenced ambiguous evidence remains, and active-feature
restart quarantine continues to dominate. This adds no apply, repository
mutation, test, provider, review, publication, lifecycle advancement, or
autonomous dispatch authority.

The artifact tree is owner-private and fixed-shape. Unix uses no-follow,
directory-relative handles and rejects wrong-owner, group/world access,
hardlinks, and symlinks. Windows opens reparse points themselves, validates
by-handle identity/link count, and withholds delete sharing while checking
evidence. Coordinated preparation guards recover exact crash-prepared or
concurrent retries without allowing one failed request to delete another's
bytes. Startup validates every referenced file against immutable SQLite
metadata. Terminal result acceptance re-hashes a stable handle immediately
before the transaction and retains file/directory handles through it. Files are
flushed before same-volume rename. Portable Windows directory
`FlushFileBuffers` is not claimed; renamed-tree crash durability remains live
Windows release evidence.

Two further owner-token-authenticated loopback-only schema-v11 resolution
routes expose the kernel's exact `cancel active feature` and `abandon and
advance` transitions without adding a device mutation surface. Schema v11
backup-first migrates retained schema-v10 cancelled or quarantined leases by
backfilling their missing immutable resolution-origin receipt only from one
exact lifecycle-bound append-only audit event; missing, ambiguous, malformed,
or non-active-origin evidence fails closed and restores the verified v10
backup. Their strict
bounded requests compare-and-set the feature identity, lifecycle, queue, and
Emergency Pause revisions inside the same immediate transaction. Cancellation
durably cancels exact coding dispatches, retains the active feature lease,
marks effect possible, and cannot advance. Abandonment is available only from
cancelled or quarantined state, requires a nonzero safe-reconciliation digest
and verified healthy-main evidence after any merge, then releases the lease and
increments the queue revision. Both return fixed path-free receipts and remain
absent from the enrolled-device mTLS router. Neither operation resumes work,
creates a lease, integrates output, approves a result, or grants repository,
review, Git, publication, or autonomous authority.

Schema v13 adds bounded general-worker packet semantics. Protocol v5/schema v13
extends the retained schema-v10 through schema-v12
contracts without weakening them. The immutable packet now contains at most 64
sorted normalized relative paths and exact deterministic `file.write.v1` or
`file.delete.v1` operations, with a 16 KiB complete job frame, 12 KiB canonical
context, and 4 KiB aggregate replacement bytes enforced identically by Rust and
the production Swift decoder. The Mac agent has no shell, `exec`, environment,
credential, network, Git, test, or arbitrary-tool authority. It traverses every
workspace component through held owner-private `O_DIRECTORY|O_NOFOLLOW`
descriptors. Creates use same-parent exclusive atomic installation; replacements
use same-parent atomic swap and verify the displaced inode against the opened
compare-and-set evidence; deletes use held-parent `fstatat` identity verification
and `unlinkat`. No mutation re-resolves a validated parent pathname.
Deletes first atomically swap the leaf into a private same-parent capture, then
verify the displaced inode and content; mismatch rolls the swap back and never
deletes the unverified replacement. Windows independently decodes every admitted
artifact and requires its canonical operations and packet digest to equal the
immutable job packet. Terminal acceptance also requires stored artifact identity,
retention, and expiry to equal the validated result payload; Swift is not trusted
as the sole semantic validator. Startup compatibility is narrower than admission:
a schema-v12 row may reopen only the exact canonical protocol-v4 README artifact
with its stored digest and size intact. That validator is migration-only;
prepare, revalidation, and every new protocol-v5 result continue to reject the
legacy shape.

After exact changed-path and canonical artifact evidence is complete, the agent
renames the workspace into a sealed directory and writes a separate bounded
owner-private recovery record outside the attested tree. That record binds the
exact job/attempt, sealed name, post-edit tree digest, expiry, and its own domain-
separated digest. Restart accepts exactly one matching pair, re-hashes the tree,
reconstructs exact cancellation authority, and blocks new admission while it is
unresolved. Tamper, orphaned or multiple state, bad permissions, binding drift,
or ambiguous cleanup makes runtime open fail closed. Exact expiry or an exact
post-restart cancellation removes both pair members. The record is never logged,
audited, uploaded, or stored in SQLite. This grants no canonical checkout,
integration, test-gate, review, publication, lifecycle advancement, or autonomous
authority, and it does not claim a portable host sandbox or egress enforcement.

The local-coding lane uses a separate `local-coding` Secure Enclave/Keychain
identity namespace. Its enrollment profile accepts only the
`inference_worker` role with the exact singleton `local.coding.v1` capability;
the standard and fixture profiles remain exact `mac_bridge` identities. The
process lifecycle selects that identity only when the local-coding snapshot
opt-in is explicitly enabled, and capability rebind remains standard-profile
only. The Swift supervisor validates that exact worker role/capability before
connecting, requires the production relay, authenticates and checks health,
then leases through the worker path without requesting either the MacBridge-only
event stream or Feature Conveyor projection and emits neither observation.
Snapshot chunks and cancellation polling share a relay-local FIFO permit so the
race remains cancellation-aware without violating the authenticated channel's
single-in-flight request contract.
Partial, mixed, or relayless worker profiles fail before network
use. Standard and fixture MacBridge sessions retain strict health plus Feature
Conveyor observation.

Schema v14 adds artifact integration and candidate freezing without changing
the protocol-v5 worker lane. One strict owner-token-authenticated loopback-only
plan projection exposes only the path-free exact bindings and artifact IDs the
owner needs to construct one strict request. That request binds a non-nil
integration ID, the complete sorted set of terminal
accepted artifact IDs, and the exact active feature, specification, lifecycle,
lease, snapshot, base commit, grants, queue revision, and Emergency Pause
revision. The master reopens and re-hashes every private artifact, decodes its
canonical operations against the immutable dispatch, and derives application
order from the immutable dispatch ordinal and packet ID. It never trusts caller
order. The complete set must equal all terminal accepted artifact-backed
dispatches for that active feature.

Filesystem work occurs without an open SQLite transaction and under one
fail-fast integration reservation that remains owned by detached work after a
client disconnect. It creates a private, independent no-remote
integration repository from the immutable master snapshot, applies only the
validated operation set, and never opens or mutates the registered source
checkout. Duplicate ordinals, case-folded exact or component-wise file/directory
path overlaps, create/replace/delete compare-and-set drift, tree-shape conflicts,
alternate object stores, links, unexpected repository metadata, artifact drift,
or authority drift fail closed. Stable no-follow handles bracket source and
candidate identities, and candidate handles remain live through the SQLite
commit. A conflict records
only bounded path-free reason and binding digests; it leaves no partial
candidate and does not advance the feature. Success creates a deterministic
Git commit and exact tree, flushes and seals the candidate repository, then one
immediate transaction rechecks every authority binding, stores immutable
artifact-to-candidate evidence, advances only `implementing` to `validating`,
and appends a redacted audit. An exact retry revalidates the recorded candidate
before returning the original receipt;
integration-ID or binding drift rejects. Startup removes only unreferenced
staging state, verifies every referenced candidate commit/tree and repository
shape, and lets the existing active-feature restart quarantine dominate any
ambiguous effect. This adds no test execution, evidence gate, review,
publication, registered-source mutation, credential, network, or autonomous
authority.

Schema v15 adds the durable test-and-evidence gate contract without widening
the worker or enrolled-device surfaces. The owner-token-authenticated loopback
`POST /v1/feature-conveyor/test-evidence-gates` accepts one strict request whose
plan must be the exact 13-command list embedded in the immutable approved
manifest: requirements binding, coverage, focused unit tests, native E2E,
documentation, knowledge base, formatting, lint, build, safety, changed paths,
secret scan, and repository validation. The request and plan digests bind the
exact feature/specification/lifecycle/lease, snapshot, integration, artifact
set, candidate commit/tree/base commit, queue and Emergency Pause revisions,
and all three repository-grant revisions. Caller-supplied executable names,
arguments, paths, evidence, or shell input are not accepted.

Validation attempts, per-command result digests and bounded metadata, and the
aggregate evidence-manifest digest are immutable and path-free in SQLite.
Only all 13 present, ordered, nonzero, passing results may atomically advance
`validating` to `reviewing` with transition evidence and redacted audit. A
recorded failure remains in `validating`; an incomplete or malformed result is
rejected without completion. Exact passed retry reopens and revalidates the
frozen candidate before returning the original receipt, exact failed retry
returns the same failure, and validation-ID or binding drift rejects. Startup
continues to quarantine any active `validating` state as effect-possible and
never retries it automatically. The route is absent from enrolled-device mTLS.

The production validation runner is connected only when the Windows master
starts with a complete, valid `<data-dir>/validation-runner/toolchain` and
`dependency-cache-seed`; an unavailable runner rejects before an attempt or
audit row is created. Before recording a start it revalidates the current
toolchain/cache, authoritative candidate, and disposable scratch. It binds the
owner-approved design digest plus approved paths and acceptance counts from the
immutable admitted work packets, creates a clean no-remote disposable copy of
the exact candidate, and revalidates the authoritative candidate and scratch
before and after every command. Master-owned checks bind requirements evidence,
documentation, knowledge-base, safety, exact changed paths, and secret-scan
counts. Fixed contained Cargo commands produce llvm-cov coverage with a
protocol-owned 70% minimum line threshold, focused unit,
native Rust E2E, formatting, lint, build, and repository-validation evidence.
The runner owns executable/argv/environment selection. Its native harness uses
a restricted token and a standard zero-capability AppContainer, with an exact
minimal environment, explicit inherited-handle list, bounded output,
memory/process Job Object limits, timeout tree termination, a temporary
execution-root ACL, and profile cleanup. Windows tests establish granted-root
operation, active cancellation with full descendant reaping, denial of one
outside-root fixture, and nondelivery to loopback TCP/UDP probes. Current proof
does not establish the installed Windows service identity, a real populated
private toolchain/cache, credential-store denial, actual above/below-threshold
llvm-cov behavior, signed Mac E2E, or OS-wide outbound-egress enforcement; keep
live activation unprovisioned until those deployment proofs are complete.

Schema v16 adds the independent-review gateway as a separate owner-token,
loopback-only boundary. Its strict request contains only exact IDs, revisions,
candidate/specification/evidence/provider bindings, never review content. The
master reopens the frozen candidate, reconstructs the canonical approved
specification, exact candidate patch, and ordered digest-only validation packet,
after approval admission has rejected transcript-, memory-, credential-, and
secret-shaped specification content using a strict deny-unknown-fields review-
safe manifest DTO; that admission is repeated at review for migrated
specifications. It derives the complete ordered identifiers from the
required top-level approved-manifest `acceptance` array and requires every finding, coverage item, and
knowledge determination to reference an admitted evidence digest. A configured
adapter is a fixed canonical executable launched once per call (plus its fixed
`--count-tokens` preflight) with a cleared environment, bounded pipes, timeout,
and complete process-tree termination before pipe joins. On Windows the master
holds a verified no-write/no-delete image handle while a trusted gate-blocked
launcher is assigned to the kill-on-close Job Object before provider spawn; missing
configuration rejects before durable mutation. Once a call is durably opened,
cancellation, Emergency Pause, or any
binding drift suppresses acceptance and terminalizes/quarantines an observed
post-response interruption. Outage, malformed output, and incomplete
transport record immutable bounded failure evidence and 1-, 5-, or 15-minute
backoff without consuming repair; at most three calls are allowed per candidate
and twelve per feature. A strict rejection remains active in `reviewing` for a
later bounded repair. Only strict approval atomically records the immutable
decision and advances `reviewing -> publishing`. The route is absent from
enrolled-device mTLS and adds no provider fallback, implementation, GitHub,
credential, or publication-coordinator authority. Startup never retries an
ambiguous in-flight provider call; the existing active-feature quarantine
dominates, while observed in-process interruption records an immutable terminal
outcome and quarantines immediately.

Schema v17 adds the owner-token loopback-only Publication Coordinator. Its
path-free request binds the approved review decision, specification, frozen
candidate, validation evidence, provider/model, three grant revisions, queue,
Emergency Pause, exact remote base, and master-derived branch policy. Before
each possible external effect the master commits one immutable action intent;
adapter evidence is never caller supplied. Each stage evidence document binds
the exact remote base, reviewed head, pull-request identity, complete required-
checks digest and pass state, branch-protection/no-bypass assertion, configured
merge strategy/resulting main, and fixed post-merge gate identity/result.
Adapter execution receives a bounded deadline plus a current-authority and
cancellation probe that must be polled around effectful work. Missing production adapter
configuration rejects before an intent. Once an intent exists, missing or
ambiguous evidence, cancellation, pause, drift, or restart quarantines without
retry. Merge advances only to `verifying_main`; exact remote-main equality plus
the fixed `release-local` post-merge gate atomically succeeds, releases the
lease, and advances the queue. Branch protection bypass is never authorized.
The GitHub adapter remains unprovisioned by default, and the route is absent
from enrolled-device mTLS.
Any durable merge intent makes a merge possible for owner resolution, even if
the feature was quarantined from `publishing`; abandonment then requires
merged/healthy-main reconciliation and cannot use `merged:false` to bypass it.

Schema v18 adds a kernel-only automatic-orchestration coordinator and immutable,
path-free checkpoint ledger. It derives decisions solely from lifecycle and
existing exact candidate, validation, review, provider-retry, publication, and
healthy-main evidence; callers cannot supply commands, paths, provider output,
adapter evidence, credentials, or failure classifications. The activation-
evidence table has no writer or HTTP route in this slice, so the coordinator is
durable but default-inert. It caps replacement candidates at three, reuses the
three-review-attempt/twelve-call limits and fixed backoff, and charges at most
24 active-processing hours while provider, worker, maintenance, and owner
pauses stop the clock. Emergency Pause atomically closes the active interval;
resume requires a fresh immutable checkpoint to restart it. Either immutable
review-call ceiling requires owner attention, never another retry. Only a
complete effect-free paused checkpoint survives restart; ambiguous effects quarantine. Substantive validation or review failure
enters `repairing`, but the current artifact/dispatch contract cannot safely
build a replacement candidate, so the next checkpoint is `attention_required`
without consuming repair budget or fabricating success. Cancellation, failure,
attention, and quarantine never auto-advance; coordination rejects the first
three without mutation. Only existing exact healthy-main success automatically
releases the lease, while an owner may release a reconciled retained lease with
the exact-CAS `abandon-and-advance` action.

The two-device proof controller creates its disposable repository with a
non-local clone so Git does not copy source-maintenance caches into the snapshot
authority boundary. Before a claim, it may remove only the strictly named,
non-reparse commit-graph cache from that marker-bound disposable checkout, then
requires the remaining object store to have exactly the flat directory/file
shape accepted by the Rust snapshot reader. This changes no committed content,
branch, grant, queue, or production checkout state.

Every authoritative transition commits its redacted audit event in the same
transaction. Migrations from supported legacy schemas are backup-first under
the owner lock and fail closed.

The master also owns identity: a DPAPI-protected ECDSA P-256 enrollment CA,
digest-only single-use grants, verified client CSRs, short-lived device
certificates, rotation, and revocation. Remote access is an explicit opt-in
TLS 1.3 mTLS listener bound to a concrete IP, with per-request revocation
recheck and TLS-exporter handshake binding.

Standard-profile certificate rotation is a confirmed, same-device recovery
ceremony rather than enrollment or capability rebind. The stopped Windows
process retains and zeroizes the raw grant, accepts the CSR only on stdin,
revokes prior serials, and durably journals the exact public issued receipt
before committing authority. The Mac reuses its selected non-exportable key and
may replace only the certificate and installed record after validating the
exact device, registration, capability, endpoint, CA, and `rotate` receipt.
Confirmed Windows recovery re-emits that byte-identical grant-bound receipt;
only a separate confirmation after Mac installation and authenticated reconnect
removes the protected journal. Device identity, registry revision, capability,
and owner-control designation remain unchanged throughout.

Capability repair for the installed standard Mac identity is an explicit,
owner-confirmed two-phase rebind, not enrollment or rotation. A ten-minute
digest-only grant snapshots the exact stale fixture registration and exact
singleton MLX target. Issuance records a digest-bound pending certificate that
is absent from the authenticating certificate registry, leaving the active
registration and certificate unchanged. After the Mac validates and stages
that certificate under a separate Secure Enclave generation, a second
owner-confirmed Windows activation atomically advances the registry revision,
inserts the replacement certificate, revokes prior certificates, and makes the
pending evidence terminal. The replacement Secure Enclave key signs a
domain-separated acknowledgement verified against the exact CSR public key
retained in pending evidence; the CA separately signs the activation receipt
verified against the staged pinned CA. Exact activation-output retries reissue
the original `activated_at` receipt, while mismatch and Emergency Pause fail
closed. Every grant, issuance, activation, and abort commits an immutable
metadata-only audit row in the same transaction. Only that authenticated
activation receipt permits local promotion. Local destructive cancellation is
limited to prepare-only state; a staged acknowledgement is preserved for
ambiguous lost-receipt recovery, and post-promotion cancellation cannot remove
selected material. Stale, expired, replayed, mixed,
cross-profile, connected, or actively leased state fails closed; abort
preserves the working identity.

### Mac worker

`assemblywright-agent` executes bounded jobs. It accepts no model, tool, file,
repository, credential, or Git input beyond its exact leased envelope. Its
lanes are default-off and singleton. Cancellation dominates completion and
suppresses late output.

### Mac app and bridge

The SwiftUI app supervises only the exact separately signed bridge helper. The
helper holds the Secure Enclave Keychain identity and the outbound mTLS
session, directly supervises the pinned agent over a mutually
code-identity-pinned local socket, and forwards authenticated metadata pages
into a durable cursor. The enrolled key never leaves the helper, and the helper
is not bundled inside the app. After health succeeds, the helper fetches the
  exact schema-v9 Feature Conveyor projection on the same authenticated session,
strictly validates and bounds it, and includes it only in authenticated app
snapshots. The SwiftUI app renders queue state and guidance as compact read-only
text. Every UUID-bearing nested owner-control value is encoded as canonical
lowercase text before the signed helper emits a snapshot; the app's strict
decoder does not relax this wire rule. A malformed or drifted projection
cancels the session and no stale status is retained. Feature authoring is a
separate explicit owner action: a typed
SwiftUI form accepts only the fixed review-safe manifest fields, digest metadata,
grant revisions, provider/model binding, and dependencies. It constructs the
existing strict request without arbitrary JSON, binds the current authenticated
queue, designation, and Emergency Pause revisions, and requires a second
confirmation. The app then stops observation, revalidates the exact independently
signed helper, and invokes only the one-shot
`feature-conveyor approve-and-enqueue --confirm` command with bounded stdin.
The helper uses the standard Keychain identity, strictly binds the redacted
receipt, and closes the authenticated session on success or failure. Windows
remains the sole canonical manifest-digest and queue authority. The form does
not create grants or proof, access a repository, claim work, dispatch a worker,
invoke a provider, publish, or activate orchestration. The first production
authoring surface fixes the only provisioned review binding to `openai.codex` /
`gpt-5.6-sol`, rejects embedded secret-shaped content before helper launch, and
shows the frozen IDs, digest prefixes, grants, provider/model, dependencies,
title, outcome, device/connection identity, queue/designation/pause revisions,
and exact request SHA-256 at confirmation. If a response is lost and the enqueue may
have committed, the app retains the exact request bytes only in memory and
offers a separately confirmed reconciliation attempt; it never silently
re-encodes or retries. Windows returns the original receipt without mutation
only when the original queue revision plus every stored specification,
dependency, lifecycle, designated device and registry revision, designation,
pause, grant, and enqueue-audit binding remain exact.

### Shared local foundation

`assemblywright-core` provides the hardened peer-identity Unix-socket transport used
between the helper and the agent, its startup validation, and read-only release
readiness and evidence inspection. `assemblywright-protocol` provides the versioned,
bounded wire contracts shared across every component.
The server performs full Security.framework validation before reading the first frame
from a new macOS audit token. It may then reuse that decision only for the exact
eight-word audit token, including PID generation, during the lifetime of that one
bound server; the cache is capped at 64 identities and never caches a rejection.

## Authority Model

Three independent owner grants exist per repository — registration, cloud
content disclosure, and autonomous publication. Each has its own revision,
scope, expiry, and revocation state, and one never implies another. Secret
detection blocks cloud transport and identifies affected paths without exposing
values.

## Safety And Error Handling

Fail closed. Ambiguous repository, provider, external-effect, review, or
publication boundaries quarantine the active feature and block the queue rather
than guessing. Automatic retry is allowed only when evidence proves repetition
cannot duplicate an effect. Emergency Pause blocks new leases and publication,
cancels safe active work, and marks potentially effectful interruptions for
review.

Redaction is structural, not cosmetic: audit and event surfaces carry metadata
and digests, never raw payloads or credentials.

## Testing Strategy

- Focused Rust and Swift unit tests for success, rejection, boundary,
  cancellation, concurrency, and recovery behavior.
- Deterministic cross-process E2E across the real boundaries: the protocol
  contract seam, a real master process with fake workers, enrollment and mTLS
  over loopback, the event cursor, the Windows SCM service, and the Mac
  agent relay. The local-coding lane additionally runs the production Swift
  relay and launcher against the real supervised Rust agent process, including
  bounded multi-file mutation, retained-workspace recovery, and
  cleanup-before-cancellation-acknowledgement contracts.
- Owner-controlled live closeouts for the Mac/Windows bridge, kept explicitly
  separate from repository validation.
- Repository validation stays distinct from signing, notarization, live-device
  QA, and owner-recorded external evidence.

## Packaging And Operations

The macOS app builds into an unsigned distribution layout with a bundled
read-only release CLI, validated bundle and installer metadata, a running-app
guard, and an isolated-HOME launch check. Developer ID signing, notarization,
stapling, and clean-profile installation remain owner-recorded external
evidence, assembled through the release evidence bundle.
