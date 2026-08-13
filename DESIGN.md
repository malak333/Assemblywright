# Assemblywright Design

This document is the system-level design. Two documents own the detailed
accepted designs and take precedence within their scope:

- [`docs/feature-conveyor-design.md`](docs/feature-conveyor-design.md) — the
  approved Feature Conveyor target and the implemented repository-kernel slice.
- [`docs/distributed-developer-mode-design.md`](docs/distributed-developer-mode-design.md)
  — the accepted authority, security, routing, recovery, and rollout target for
  distributed Developer Mode.

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
- No automatic advancement after cancellation, failure, attention, quarantine,
  or abandonment.
- No peer-to-peer worker authority, shared writable worker checkouts, or worker
  Git publication credentials.
- No general-purpose assistant surface. Conversation, personal memory,
  scheduling, voice, and a plugin marketplace were removed with the pivot to
  Developer Mode and are not planned.

## Architecture

### Windows master

`assemblywright-master` owns durable state and every authority decision. Its schema-v16
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
  exact schema-v8 Feature Conveyor projection on the same authenticated session,
strictly validates and bounds it, and includes it only in authenticated app
snapshots. The SwiftUI app renders queue state and guidance as compact read-only
text; a malformed or drifted projection cancels the session and no stale status
is retained. The app remains read-only. A separate explicit one-shot signed
helper command accepts one bounded already-approved document on stdin only with
`--confirm`, uses the standard Keychain identity to invoke the designated-owner
route, strictly binds the redacted receipt, and closes the authenticated
session on success or failure.

### Shared local foundation

`assemblywright-core` provides the hardened peer-identity Unix-socket transport used
between the helper and the agent, its startup validation, and read-only release
readiness and evidence inspection. `assemblywright-protocol` provides the versioned,
bounded wire contracts shared across every component.

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
