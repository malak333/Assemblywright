# Durable Feature Conveyor Design

Status: APPROVED by owner and structured multi-agent design review

Date: 2026-07-24

Scope: Approved target design plus the bounded repository-kernel implementation
status below. This document does not claim that activation prerequisites have
been satisfied, live-device proof, or production readiness.

Implementation status: the default-inert protocol-v5/schema-v19 Windows
`assemblywright-master` kernel is implemented through the owner-control and
activation surface. The detailed implemented boundary is summarized under
`Implementation Boundary` below. The default-inert protocol-v5/schema-v18 Windows
predecessor introduced the internal orchestration ledger; schema v19 migrates it
backup-first and adds the bounded activation and owner-control authority.
It retains immutable approved specification
revisions, the bounded owner-ordered queue, dependency blocking, compare-and-
set revisions, one active lease, exact lifecycle advancement, cancellation
without advancement, explicit safe abandonment, startup quarantine, and same-
transaction redacted audits. It exposes the exact
same read-only projection through the owner-token-authenticated loopback
`GET /v1/feature-conveyor/status` and the accepted-session, MacBridge-only
remote route described below. The pure-SELECT projection returns the
schema and queue revisions, startup-quarantine count, zero-inclusive aggregate
lifecycle counts, and at most 100 current queue or retained-lease entries
containing only feature ID, specification and lifecycle revisions, queue
position, status, lease presence, and effect-possible state. It reports the
current visible total and whether entries were truncated. Historical terminal
rows are excluded. One fixed-enum `owner_guidance` object adds a state bound to
the queue, Emergency Pause, and optional current feature lifecycle revisions;
a reason code; a display-only next owner action; and optional current
feature/specification/lifecycle identity. Its exact precedence is
Emergency Pause; cancelled or quarantined retained lease; normal active lease;
empty queue; unsatisfied queue-head dependency; ready queue head. It exposes no
dependency identifiers, repository/provider/grant data, free text, evidence,
or audit metadata. `queue_revision` and `emergency_pause_revision` always bind
the advisory snapshot; feature guidance also binds `lifecycle_revision`.
This projection remains advisory observation only: it does not establish
claimability or authorize its labeled action.
A dedicated `GET /v1/distributed/feature-conveyor/status` returns the exact same
projection only after the TLS-exporter-bound application handshake is accepted
for an enrolled `MacBridge`; pre-handshake and non-MacBridge requests are
denied, and no owner token crosses the device boundary. The Swift helper
strictly decodes the bounded schema-v9 allowlist, cancels on drift, and includes
the projection only in authenticated snapshots. The app renders a compact
read-only queue/guidance section and never presents `next_owner_action` as a
button. Neither observation route grants enqueue, execution, review,
repository, Git, publication, audit-event, or activation authority. The
separate schema-v16 independent-review gateway is implemented and defaults
unavailable until one fixed adapter is provisioned. Schema v17 implements the
durable owner-loopback Publication Coordinator and controlled bare-Git proof.
Feature 4 provisions its fixed credential-owning Windows GitHub adapter only
after exact tool, private configuration, repository, protection, and check
policy validation; the adapter remains unavailable by default. Schema
v18 added the internal deterministic orchestration checkpoint kernel described
below. Schema v19 adds owner-token activation-evidence admission, designated
exporter-bound Mac activation and owner-control routes, and confirmed signed-
helper SwiftUI controls. No automatic dispatcher or registered-source mutation
is implemented, and activation remains blocked until all six genuine live proof
receipts are admitted. The remainder of this document beyond the explicit
implementation boundary is still target design.

Master schema v8 adds the first bounded owner-control transport without
activating the conveyor. An owner-token-authenticated Windows loopback action
designates one exact current, enrolled, non-fixture `MacBridge` using a durable
compare-and-set designation revision and same-transaction redacted audit. Two
additional owner-token loopback-only routes record and inspect the three
independent repository grants without accessing a repository. Each strict
digest-only mutation must be the contiguous next revision, match the current
grant and Emergency Pause revisions, and commit with a redacted audit. Active
grants fail while paused; a revocation revision remains available. The current
projection exposes only digests, revision, expiry, revocation, and computed
active state, and both routes are absent from enrolled-device mTLS. A separate
owner-token loopback-only repository preflight accepts one bounded scope whose
canonical digest, registration-grant revision, exact single-component base branch, exact HEAD,
and Emergency Pause revision must all still match. It performs a point-in-time
filesystem identity inspection of only a standard local `.git` directory,
objects directory, symbolic HEAD, and exact loose branch ref. Windows holds
non-reparse handles for the complete fixed-volume identity chain, then reopens
and compares the canonical pathname and identities immediately before the final
atomic grant, pause, and audit recheck and retains the fresh handles through it.
It runs no Git
process, loads no repository configuration or attributes, rejects network,
device, reparse, worktree, and submodule paths, and does not prove clean-tree,
index, object, or content state. The canonical path is neither stored nor returned;
success emits only a path-free digest receipt after an atomic redacted audit.
The five-second timeout applies to each filesystem-observation await only; owner
authentication and database lock/audit latency remain separately fail-closed
but are not covered by that response-time claim.
An existing `.git/worktrees` directory is a deliberate rejection, not an
operator cleanup instruction. Positive admission proof uses a dedicated
standalone checkout with independent Git metadata; a disposable proof grant is
revoked by its next contiguous revision before that checkout is removed.
The live controller uses a non-local clone and verifies the snapshot reader's
flat object-store shape. A copied Git commit-graph cache is expendable metadata,
not repository content: recovery removes it only after exact marker, path,
entry-name, regular-file, and no-reparse checks, then revalidates the complete
object-store shape before requesting the claim.
This is admission evidence for the observed instant only: it creates no durable
snapshot, claim, lease, mutation, worker, review, publication, or activation
authority.

Master schema v9 adds the separate owner-token loopback-only
`POST /v1/feature-conveyor/repository-snapshot-claims`. Its strict request binds
the exact preflight scope, queue head and specification revision, provider/model,
all three grant revisions, queue revision, Emergency Pause revision, branch,
and base commit. Planning and filesystem work occur without an open SQLite
transaction. The source identity is freshly revalidated immediately around raw
object snapshot construction and once more before an immediate finalizer
atomically rechecks every binding, records immutable path-free snapshot
ID/digest/base-commit evidence, inserts the singleton lease, advances the
lifecycle, increments the queue revision, and appends redacted audit. Snapshot
construction executes no Git process and reads no source config, hooks,
attributes, credentials, global config, or PATH; alternates, symlink/reparse
entries, gitlinks, non-fixed/ambiguous paths, and invalid object graphs fail
closed. It copies only the exact base commit and its current tree/blob graph,
writes that commit to shallow metadata, and never copies parent history or
deleted historical objects. The result contains raw current committed content,
independent Git metadata, an empty hook lane, no remote, no hardlinks, and no
source path. A process-wide fail-fast singleton reservation permits only one
claim. The snapshot task is capped at 50,000 objects, 256 MiB total, and 32 MiB
per blob; its HTTP wait is 30 seconds. Because blocking threads cannot be safely
force-cancelled, a timed-out task retains the reservation and RAII cleanup
ownership until it actually exits. Request
cancellation or any failure before finalization removes the snapshot, and
startup removes unreferenced UUID directories. A finalized lease is
quarantined on restart without retry. This slice grants no worker dispatch,
provider call, review, repository mutation, publication, Mac mutation, remote
mTLS action, queue advancement, or autonomous activation. Only the
exact designated device may use
`POST /v1/distributed/feature-conveyor/approved-features` after a revalidated,
exporter-bound application handshake. Its fixed-schema request carries one
already-approved specification and binds the exact queue, designation, and
Emergency Pause revisions. Existing grant, manifest-digest, dependency,
capacity, immutable-specification, and atomic-audit checks remain authoritative.
The one-shot signed Mac helper requires `--confirm`, reads the bounded request
from stdin, strictly validates the redacted receipt, and closes the session.
It does not create grants or brainstorming proof, claim or dispatch the queue,
invoke a worker or provider, access a repository, or grant Git/publication or
autonomous activation. That schema-v8 enqueue slice left the SwiftUI app
observation-only; schema v19 adds the separate confirmed owner-action surface
described under `Implementation Boundary`.

The following paragraph records the historical protocol-v4/schema-v12 worker.
Master schema v10 added the first separate metadata-only coding-dispatch
admission kernel. The owner-token loopback-only
`POST /v1/feature-conveyor/coding-dispatches` accepts one strict path-free work-
packet digest and bounded ordinal/acceptance-count metadata only after schema
v9 has claimed the exact queue head and snapshot. It binds the exact active
feature and specification/lifecycle revisions, feature lease, snapshot ID and
digest, queue and Emergency Pause revisions, and one current non-revoked
`InferenceWorker` registration whose only capability is `local.coding.v1`.
Dispatch evidence, one existing distributed queued step and event, and a
redacted Feature Conveyor audit commit atomically. The owner action is absent
from the enrolled-device router. Remote lease and result acceptance fail closed
when any bound authority changes; cancellation, Emergency Pause, lifecycle
departure, or restart quarantine prevents later acknowledgement. The
default-off snapshot-transfer lane is separate from dispatch: after the exact
lease, the bridge requests sequential bounded chunks whose request and response
bind the job, attempt, lease, cancellation, snapshot ID/digest, offset, total,
chunk digest, and completion state. The master reauthorizes around every
filesystem read. The native agent reconstructs and verifies the independent
shallow Git snapshot in a fresh private per-attempt directory, rejects unsafe
paths, links, duplicates, trailing bytes, and object/digest drift, and charges
every manifest entry against an aggregate materialized-output byte budget. It
then forks one fixed child from the already-running agent with no `exec`,
argument parsing, or remote input. Before `fork`, the parent pre-opens the
workspace, blocks signals, and captures the descriptor-table bound and
effective UID. The
Swift parent launches the agent with an empty environment and the agent refuses
local-coding startup if it observes a nonempty parent environment. The child
scans every descriptor slot with `F_GETFD`, closes every open descriptor except
the workspace and group gate, and waits for the parent-established process
group. Its fixed path validates, truncates, seeks, writes, syncs, closes, and
exits after opening only `README.md` relative to the workspace. It does not
inspect errno or mutable global state, use environment APIs, or call `geteuid`,
`getdtablesize`, or `setpgid` post-fork. The parent rejects
any other file drift and returns bounded path-free
work-packet/admission/snapshot/allowed-path/changed-path/patch digests,
one changed file, `test_status:not_run`, mutation true, workspace-retained false,
and ambiguity false, and removes all attempt state before the result is
available. The protocol-owned admission digest is the SHA-256 of one fixed
domain, protocol version as big-endian `u16`, the raw context digest, five raw
UUIDs, then connection epoch, sequence, lease duration, and deadline as
big-endian `u64`; Swift mirrors that exact transcript. Cancellation,
Emergency Pause through durable cancellation,
lease/deadline loss, shutdown, restart, and failure dominate completion; the
child's own process group is boundedly TERM-to-KILL reaped before cleanup. The
Swift cancellation task sends local cancellation before cancelling the blocked
Unix request. It marks cancellation in progress before the local call, so the
expected final-chunk rejection may race ahead of the acknowledgement without
overriding a subsequently validated cancellation; a rejection without that
in-progress cancellation still fails closed. Native proof requires
cleanup-before-acknowledgement, no result post, and acknowledgement strictly
inside two seconds. This
proves one deterministic contained-coding fixture, not arbitrary
commands, tools, paths, providers, tests, credential/network access, a host
sandbox or host-egress enforcement, retained worker state, canonical-repository
mutation, integration, review, publication, queue advancement, or autonomous
activation.

Protocol v5/schema v13 adds the bounded general-worker packet while retaining
all schema-v10 through schema-v12 contracts above. Rust and production Swift
enforce one 16 KiB complete job frame, 12 KiB canonical context, 4 KiB aggregate
replacement data, 64 sorted normalized relative paths, and only exact
`file.write.v1`/`file.delete.v1` schemas. The agent traverses all parents through
held owner-private no-follow directory descriptors. Create uses exclusive atomic
installation, replacement uses atomic swap with displaced-inode compare-and-set
verification, and delete verifies the held leaf identity before `unlinkat`.
Delete atomically swaps the leaf into a private same-parent capture before
verification; mismatch restores it atomically and no unverified replacement is
removed. Windows independently decodes canonical artifact bytes against the
immutable job packet, and terminal acceptance requires stored artifact and
retention/expiry metadata to match the validated result. This authority does not
depend on Swift validation, while exact replay remains idempotent. For backup-
first schema-v12 migration only, startup also recognizes the exact canonical
protocol-v4 README artifact when its immutable row digest and size match. New
preparation, handle revalidation, result admission, and terminal acceptance
remain strictly protocol-v5 and reject that historical shape.

Completion seals the workspace unchanged after evidence collection. A separate
bounded owner-private recovery record outside that tree binds the exact job,
attempt, sealed name, post-edit tree digest, expiry, and record digest. Startup
re-hashes exactly one paired workspace, reconstructs exact cancellation authority,
blocks new admission while unresolved, removes an exact expired pair, and rejects
tamper, orphaned/multiple state, permissions drift, or ambiguity. The private
record is not audit, SQLite, or remote content. This adds no arbitrary tool,
canonical checkout, integration, test-gate, review, publication, lifecycle
advancement, sandbox, egress enforcement, or autonomous authority.

Schema v14 implements the separate owner-token loopback-only
`POST /v1/feature-conveyor/artifact-integrations` action. It does not change the
protocol-v5 worker lane. The companion owner-token loopback-only
`GET /v1/feature-conveyor/features/:feature_id/integration-plan` returns the
current path-free artifact IDs and exact authority bindings needed to construct
that request; neither route exists on enrolled-device mTLS. The strict request
supplies one non-nil integration ID
and the complete sorted set of terminal accepted artifact IDs, bound to the
exact active feature, specification, lifecycle, lease, snapshot, base commit,
grants, queue revision, and Emergency Pause revision. Windows reopens and
re-hashes every artifact, independently validates its operations against the
immutable packet, requires the set to equal every terminal accepted artifact-
backed dispatch for the feature, and orders application by immutable dispatch
ordinal and packet ID rather than caller order.

Under one fail-fast reservation that survives client cancellation, and without an open SQLite transaction, the
master creates a private independent no-remote integration repository from the
immutable claimed snapshot. It never opens or mutates the registered source
checkout. Duplicate ordinals, case-folded exact or component-wise file/directory
overlaps, create/replace/delete compare-and-set drift, tree-shape conflicts,
alternate object stores, links, metadata drift, missing
evidence, or authority drift record only bounded path-free conflict evidence;
they leave no partial candidate and do not advance the lifecycle. Success
freezes one deterministic exact Git commit and tree, flushes and seals the
private repository, then atomically rechecks every binding, stores immutable
artifact-to-candidate evidence, advances only `implementing -> validating`, and
appends redacted audit. Stable no-follow handles bracket source and candidate
identity verification, and candidate handles remain live through the SQLite
commit. Exact retries revalidate the recorded candidate before returning the
original receipt, and every
binding drift rejects. Startup removes only unreferenced staging state, verifies
every referenced candidate repository and commit/tree, and quarantines
ambiguous active state. Test execution, review, publication, registered-source
mutation, credential/network access, and autonomous activation remain absent.

Schema v15 implements the persistence, strict protocol, and owner-local
coordination seam for the next `Evidence Gate` stage. The sole HTTP mutation is
the owner-token-authenticated loopback-only
`POST /v1/feature-conveyor/test-evidence-gates`; it is absent from enrolled-
device mTLS. Its request cannot name a command, executable, argument, path, or
result. Instead it must reproduce the immutable approved manifest's exact
protocol-owned ordered plan of all 13 required command identifiers and its
canonical plan digest. The request binding also covers the exact feature,
specification and `validating` lifecycle revision, retained feature lease,
snapshot ID/digest, integration ID/artifact-set digest, frozen candidate
commit/tree/base commit, queue and Emergency Pause revisions, and all three
repository-grant revisions. Any drift, missing command, reordering, unknown
field, zero digest, pause, stale grant, or candidate mismatch fails closed.

The schema stores an immutable attempt, immutable per-command pass bit, result
digest, duration and truncation bit, and an immutable aggregate evidence-
manifest digest. It stores no command text, output, path, match, source, secret,
or caller evidence. All 13 ordered evidence results must be present, nonzero,
passing, and non-truncated before one immediate transaction advances only
`validating -> reviewing`, records the accepted evidence digest in transition
evidence, and appends redacted audit. A completed failure stays `validating`;
incomplete or malformed evidence does not complete the attempt. An exact retry
of a passed attempt reopens and revalidates the frozen candidate before
returning the original receipt; exact retry of a recorded failure returns the
same fixed failure; validation-ID or request-binding drift rejects. Startup
quarantines an active `validating` feature as effect-possible and never retries
it automatically.

Validation execution is connected only on Windows when both fixed, private
runner inputs exist and validate at startup. Missing inputs return
`validation_runner_unavailable` before durable attempt/audit mutation. Before a
start is recorded, the runner revalidates toolchain/cache, authoritative
candidate, and scratch resources. It binds the approved design digest to exact
changed paths and acceptance counts from admitted work packets, creates and
verifies a disposable no-remote candidate copy, runs six
deterministic master checks and seven fixed contained Cargo commands, and
revalidates authoritative and disposable candidate identity throughout.
Coverage uses staged `cargo-llvm-cov` with the protocol-owned 70% minimum line
threshold; native E2E uses the fixed Rust test-target command. The process
boundary uses a restricted token, standard zero-capability
AppContainer, exact minimal environment and inherited-handle list, bounded
output, Job Object process/memory limits and tree termination, temporary ACLs,
and checked cleanup. Native Windows tests prove granted-root read/write, active
cancellation and descendant reaping, outside-root denial, and loopback TCP/UDP
nondelivery. They do not yet prove installed-service-identity execution, a real
populated production toolchain/cache, credential-store isolation, actual
above/below-threshold llvm-cov behavior, signed Mac E2E, or OS-wide outbound
egress denial. Live provisioning remains blocked on those deployment proofs.

Schema v11 also exposes two separate owner-token loopback-only resolution
actions: `POST /v1/feature-conveyor/cancel-active-feature` and
`POST /v1/feature-conveyor/abandon-and-advance`. Both accept only strict,
bounded, path-free input and compare-and-set the exact feature, lifecycle,
queue, and Emergency Pause revisions inside the authoritative transaction.
Cancellation cancels any exact bound coding dispatch, retains the feature
lease, records effect-possible state, and never advances. Abandonment accepts
only a cancelled or quarantined retained lease, requires a nonzero safe-
reconciliation digest and verified healthy-main evidence after any merge, then
releases the lease and advances the queue revision. Same-transaction redacted
audits remain authoritative. The fixed receipts contain no repository path,
content, evidence text, credential, provider payload, or raw error. Schema v11
requires the cancelled or quarantined retained lease to carry an
immutable transition receipt naming its active execution origin. The
backup-first v10 migration backfills a missing receipt only when one exact
lifecycle-bound append-only cancellation or startup-quarantine audit event has
the fixed redacted shape; missing, duplicate, malformed, or non-active-origin
evidence fails closed and restores the verified v10 backup. These actions are absent
from enrolled-device mTLS and do not resume work, approve a
result, integrate a patch, or grant review, Git, publication, or autonomous
activation authority.

The Mac provisions this worker through a separate `local-coding` Keychain
identity profile. Enrollment is accepted only for role `inference_worker` and
the exact singleton `local.coding.v1` descriptor; standard and fixture profiles
remain `mac_bridge`-only. The production helper selects the isolated profile
only for the explicit local-coding snapshot opt-in and never reuses owner-control
bridge authority. That exact `inference_worker` plus singleton-capability
profile must have a production relay, authenticates and health-checks without
requesting either the MacBridge-only event stream or Feature Conveyor
projection, emits neither observation, and fails before connection when
partial, mixed, or relayless. Standard and fixture
MacBridge profiles continue to require strict health plus Feature Conveyor
observation.
Snapshot transfer and cancellation polling remain concurrent for cancellation
dominance, but every network request crosses one relay-local FIFO permit so the
production authenticated session still has exactly one request in flight.

Schema v18 adds immutable path-free orchestration state and checkpoints under a
backup-first migration. This paragraph describes the historical v18 boundary;
schema v19 adds the owner controls above. The internal coordinator derives every stage/action and
failure classification from master state and existing exact evidence. It never
accepts caller commands, paths, provider output, adapter evidence, credentials,
or retry classifications. The activation-evidence table intentionally has no
writer in that historical slice because capability 7 owns owner activation and
functional live evidence. Schema v19 supplies the bounded writer and routes
without changing how the coordinator derives actions.

The ledger supports `repairing`, `paused`, `attention_required`, and `failed`.
Only `succeeded` automatically releases the lease. Coordination rejects
`cancelled`, `attention_required`, and `failed` without mutation; after exact
reconciliation, the owner may explicitly use `abandon-and-advance` to release
one of those retained leases. Provider transport backoff is an effect-
free pause and consumes neither active time nor repair budget. The initial
candidate is free, replacements are capped at three, review calls retain the
three-per-candidate and twelve-per-feature ceilings, and the 24-hour active
clock is restart-safe. Emergency Pause atomically charges only through the
pause instant and suspends the clock; a fresh immutable checkpoint restarts it
after resume. Completed immutable calls reaching either review ceiling require
owner attention instead of another retry. Startup resumes only a complete paused checkpoint with
`effect_possible:false`; unsafe legacy stages, in-flight provider calls, and
unresolved publication intents quarantine. Current artifact/dispatch contracts
cannot express a replacement candidate safely, so substantive validation or
review failure records `repairing` and then `attention_required` without
charging a repair or fabricating completion.

## Understanding Summary

- Assemblywright will add an owner-managed autonomous development queue for personal,
  registered repositories. The queue has a hard capacity of 100 approved
  nonterminal features and never generates or replenishes features
  automatically.
- Every feature is manually scoped through a Assemblywright-hosted brainstorming
  session. It enters the executable queue only after the owner confirms the
  Understanding Lock, accepts the final design, reviews the exact specification
  digest, and selects `Approve and Enqueue`.
- Exactly one feature may be active. Up to three restricted local coding agents
  may collaborate within that feature while a fourth job slot remains reserved
  for orchestration or interactive control.
- Codex may assist with planning and final review but never implements. One
  global setting selects Codex-account or an owner-configured local model for
  planning and review. There is no silent provider fallback.
- A fresh reviewer must approve the exact specification, candidate commit and
  diff, deterministic evidence, documentation, testing, and knowledge-base
  outcome. Rejection keeps the feature active and enters a bounded repair loop.
- Approval publishes through feature branch, pull request, required checks,
  merge to `main`, exact merge-commit verification, and a post-merge gate. Only
  verified success may advance automatically to the next feature.
- The owner may explicitly abandon an unapproved feature only through a
  separate authenticated `abandon and advance` resolution after safe
  reconciliation. After any merge, `main` must first be restored to verified
  health through a corrective or revert pull request.

## Assumptions And Non-Functional Requirements

| Area | Requirement or assumption |
| --- | --- |
| Owner | One authenticated owner. Models and workers may propose but cannot enqueue, reorder, cancel, abandon, grant authority, or rebind providers. |
| Authority | The Windows `assemblywright-master` is the sole durable Feature Conveyor, repository, policy, audit, and publication authority. The Mac app is the primary owner UI. |
| Queue scale | At most 100 queued plus active nonterminal features. Archived terminal records do not consume capacity. |
| Feature concurrency | Exactly one active feature. Four total job slots: up to three implementation jobs and one reserved orchestration or interactive slot. Actual use may be lower when resources are unavailable. |
| Worker placement | Implementation uses restricted local coding agents only. Codex never receives repository-write or implementation authority. |
| Provider selection | One global planning/review provider applies to future features. An active feature keeps its snapshotted provider until an authenticated owner rebinds it. |
| Provider compatibility | Owner selection determines local-review quality eligibility, but every selectable adapter must mechanically support the fixed review envelope and strict structured output. |
| Repair budget | The initial candidate is free. At most three replacement candidate commits and 24 hours of active processing are allowed per feature. |
| Provider budget | At most three transport attempts per candidate and twelve review calls per feature, with 1-, 5-, and 15-minute backoff. Provider failures do not consume repair cycles. |
| Responsiveness | Queue and status operations acknowledge within one second p95, dispatch decisions within two seconds p95, and state reaches the Mac UI within two seconds under the supported two-machine topology. |
| Performance workload | Measure after warm-up with 100 queued features, one active feature, three implementation workers, continuous event streaming, and at least 1,000 queue or status operations. |
| Availability | Provider, worker, maintenance, and owner-requested pauses do not consume active-processing time. There is no automatic provider substitution or Windows failover. |
| Recovery | Resume only work proven safe. Ambiguous repository, provider, external-effect, review, or publication boundaries quarantine the active feature and block the queue. |
| Privacy | Credentials and detected secrets never reach Codex. Secret admission runs before planning context or review transport and again before publication. |
| Cloud review context | The review packet contains only owner-approved, non-secret repository artifacts: the approved final specification, exact candidate diff, and implementation evidence. Raw brainstorming transcripts and canonical Assemblywright memory are not reused in final review. |
| Planning conversation | When Codex is the selected planner, each owner-authored hosted brainstorming turn is an explicit cloud interaction. The resulting raw transcript is not later attached to the review packet. |
| Maintenance | Feature Conveyor schema changes use backup-first, versioned, transactional, fail-closed migrations. Rollback restores the verified pre-migration backup. |

The fixed first-release review envelope is a release contract:

- At most 256 KiB of serialized UTF-8 review input.
- At most 64,000 provider input tokens after adapter-specific tokenization.
- At most 64 KiB of structured reviewer output.
- The selected adapter must advertise enough context for the bounded input and
  output together.

If the final specification, exact diff, and required evidence cannot fit, the
feature is rejected before enqueue and must be split through brainstorming.
Changing the global provider does not change these ceilings.

## Explicit Non-Goals

- Codex or any cloud model implementing repository changes.
- Automatic backlog generation, replenishment, or model-controlled ordering.
- More than one active feature, even across different repositories.
- Silent interpretation of ambiguous or contradictory specifications.
- Automatic provider fallback or automatic active-feature rebinding.
- Automatic advancement after cancellation, failure, attention, quarantine, or
  abandonment.
- Peer-to-peer worker authority, shared writable worker checkouts, or worker Git
  publication credentials.
- Treating repository validation as proof of live Codex-account reliability,
  GitHub authority, two-device behavior, signing, notarization, or production
  readiness.

## Architecture

### Windows Master

The Windows `assemblywright-master` owns six Feature Conveyor components.

#### Feature Registry

The registry stores immutable owner-approved specification revisions, queue
position, dependencies, repository identity, grant revisions, brainstorming
proof, review-envelope admission, and lifecycle state. Draft brainstorming
sessions remain outside the executable queue.

The capacity of 100 counts queued and active nonterminal features. Insertion is
atomic and fails when capacity is exhausted. Waiting features may be reordered
by the owner with compare-and-set queue revisions. The active feature is never
reordered. Only the first queued feature may activate; an unmet dependency
blocks the conveyor rather than allowing a later feature to skip ahead.

#### Feature Orchestrator

The orchestrator owns a singleton active-feature lease. It alone may activate a
feature, snapshot its provider and grants, reserve disk, create isolated
execution state, issue worker jobs, freeze a candidate, request validation or
review, begin publication, or authorize advancement.

#### Local Worker Dispatcher

The dispatcher creates bounded jobs for restricted local coding agents. Each
agent runs under a dedicated identity against a self-contained, credential-free
repository snapshot with its own Git metadata and no remote. It cannot read the
canonical repository, master database, canonical memory, credentials, or
unrelated files. General network access is disabled; only a narrowly controlled
local-model connection is allowed.

The implemented protocol-v5/schema-v19 kernel retains the schema-v14 worker and
integration boundary and accepts one immutable
snapshot-bound general coding packet for one exact registered worker. After the
exact lease, a separate default-off route streams a bounded authenticated
snapshot bundle to the Mac bridge and native agent. The agent reconstructs an
independent no-remote Git repository, verifies its object graph, paths, modes,
sizes, and aggregate digest, and enforces an aggregate materialized-output byte
budget. It then applies only exact deterministic `file.write.v1` and
`file.delete.v1` operations within the packet's sorted normalized allowlist,
without a child, shell, `exec`, environment, provider, test, network, credential,
or arbitrary-tool authority. Raw manifest and packet paths are rejected before
filesystem normalization.

The agent verifies the exact changed-path set and canonical multi-file artifact,
then seals the successful workspace with a separate owner-private bounded
recovery record until exact resolution or expiry. The Swift relay admits the
artifact bytes over the existing authenticated FIFO cancellation race before
posting a metadata-only result. Windows independently decodes those bytes
against the immutable packet and records only immutable artifact and retention
bindings with redacted audit. Exact retry is idempotent; drift fails closed;
startup preserves referenced ambiguity under active-feature quarantine. A
separate owner-local integration action validates the complete accepted
artifact set again, applies it only to an isolated no-remote master repository,
and freezes one exact candidate commit before entering `validating`. Schema
v15 adds the strict digest-only evidence-gate persistence and a provisioned
Windows execution path over a disposable candidate. Schema v16 adds the
independent-review gateway described below. Publication and registered-source-
checkout mutation remain unimplemented. This boundary does
not establish installed-service execution, signed Mac E2E, or OS-wide egress
control.

Schema v19 closes owner activation without turning the Mac into an evidence
authority. Windows admits six fixed, digest-only proof-controller receipt
categories over an owner-token loopback route, then projects their current
references to the exact designated exporter-bound MacBridge. Remote activation
can select only those already-admitted references and exact queue, designation,
and Emergency Pause revisions. The global singleton is immutable and creates
no lease; orchestration stays default-inert until it exists. Owner pause/resume
is a distinct active-feature action bound to exact lifecycle and orchestration
revisions. It requires an effect-free checkpoint, preserves active-time
accounting, and never substitutes for global Emergency Pause. Cancel retains
the lease; abandon still requires safe reconciliation and healthy-main proof
where publication might have merged.

Feature 1 supplies only the deterministic repository-gate proof controller.
The owner explicitly invokes
`./scripts/repository-gate-proof-controller.sh --run` on the Mac. The command
cannot select a repository or gate; it requires the controller's exact clean
`main` checkout to equal `refs/remotes/origin/main`, records stable pre/post
branch, HEAD, tree, status and origin state, and executes only the exact
committed `scripts/release-local.sh` bytes through argument-free Bash stdin. Its fixed ignored owner-private output
contains a bounded, path-free schema-v1 receipt and raw SHA-256 sidecar. The
receipt binds `repository_gate_proof` to
`repository_gate_proof_controller`, the exact commit/tree, the committed local-
gate definition digest, `assemblywright.release-local.v1`, observed time,
pass status, and the repository-only boundary. Drift, failure, and handled
cancellation leave no receipt; temporary files are never evidence. This
controller performs no Windows request or audit, evidence admission, source
mutation, approval, activation, or live proof. The owner separately decides
whether to admit its digest through the existing owner-token loopback route;
that Windows transaction remains authoritative and audited. Protocol v5 and
schema v19 are unchanged.
The fixed `target` chain must remain ordinary, owner-matched and non-symlinked;
reparse-style ambiguity is rejected before output cleanup or creation. Once
that chain validates, beginning a new run removes only the exact previous
receipt and sidecar. This deliberate invalidation ensures a failed rerun does
not expose stale local evidence as though it came from the current attempt.
Repository observations use environment-empty, fixed-root Git with replace
objects and inherited config, index/work-tree, object-directory and alternate
redirections disabled. A narrow internal stdin/root marker lets the committed
gate recover the already-fixed root; caller values are cleared and the gate
unsets the marker before running commands. `target` is non-group/world-writable,
and the controller holds the output directory while binding and revalidating
target/output device and inode before and after relative-name publication.
Gate failure drains the separate process group immediately and suppresses the
receipt. Only after the successful gate leader exits does the controller permit
a short bounded natural group drain. A member still present at that deadline,
or HUP, INT, or TERM, drains the group forcibly and suppresses the receipt.
Every tracked index entry must have Git's normal `H` tag; hidden
assume-unchanged, skip-worktree, or other index state is ineligible. Pre/post
stability catches same-UID mutation of other gate-consumed files at the
controller edges, not through a claimed host sandbox.

Feature 2 supplies the separate owner-supervised restricted-worker live proof
controller at `scripts/restricted-worker-proof-controller.sh`. Its public
`--run` accepts no repository, remote, worker, executable, packet, or alternate
harness. The controller requires exact clean `main` at
`refs/remotes/origin/main`, then executes only the committed Mac live-harness
blob through Bash stdin. Sanitized Windows-local control receipts use a
separate inherited descriptor, preventing coordination input from becoming
executable script content. While reading each receipt from an interactive Mac
terminal, the controller temporarily uses no-echo noncanonical input so the
bounded JSON can exceed the PTY's 1,024-byte `MAX_CANON`; it restores the exact
saved terminal state on every success, failure, and handled signal. A real-PTY
self-test binds the full 2,182-byte fixture digest, proves storage remains capped
while a 9,000-byte line is drained and rejected, and verifies TERM restoration. The
fixed harness requires the signed Swift relay,
the separate singleton `local.coding.v1` identity, and the real Rust agent. It
binds one protocol-v5 packet to the schema-v19 Windows master, observes exact
queued/leased/succeeded events, validates the admitted artifact and detached
remote-free candidate, checks retained Mac state, and ends only after explicit
cancel, safe abandon, grant revocation, disposable-checkout removal, and empty
active/transfer state. The controller accepts one exact bounded terminal record,
after exactly six unique ordered action markers. It hashes and removes the
complete private transcript, rechecks stable repository/controller state, and writes
only a fixed owner-private path-free receipt plus digest under
`target/restricted-worker-live-proof`. The receipt binds
`restricted_worker_live` to `restricted_worker_proof_controller`, exact
HEAD/tree, both committed control-definition digests, the private transcript
digest, fixed identity, observed time, and its narrow boundary. It neither
admits that digest nor activates. Same-owner process and live-device proof does
not establish a portable host sandbox, OS-wide egress denial, review-provider,
GitHub-publication, restart-recovery, control-streaming, signing distribution,
notarization, clean-profile, or production-readiness evidence. Protocol v5 and
schema v19 do not change.

Feature 3 supplies a fixed production Codex review adapter plus a separate live
proof controller. `scripts/windows-review-provider-live-control.ps1` pins the
selection to `openai.codex` / `gpt-5.6-sol` and Codex `0.148.0`, verifies exact
clean Windows source, downloads the fixed Windows package only from its
committed registry URL, verifies committed package and native-executable
digests before first execution, builds both master binaries, and stages the adapter,
native Codex executable, committed strict output schema, and digest-bound
schema-v2 `provider.json` in an owner/SYSTEM-private directory while the service
is stopped. The master accepts no runtime provider path or model override. It
verifies the adapter/Codex/schema digests and ordinary canonical support assets,
requires owner/SYSTEM-only protected auth DACLs, retains locked handles during the
call, clears the provider environment, and uses the existing Windows Job Object
gate before process execution. The adapter clears again and passes the
owner-private `CODEX_HOME`; on Windows it additionally derives `SystemRoot` and
the system-directory-only `PATH` directly from OS APIs so native DNS can operate
without ambient user environment. Codex uses strict configuration parsing, is
ephemeral and read-only, and is configured with
shell, snapshot, subagent, skill-install, image, and web surfaces disabled.

After exact published Mac/Windows parity, the owner runs
`./scripts/review-provider-proof-controller.sh --run` and supplies the one
sanitized output from the committed Windows control. The native harness rejects
duplicate/extra keys and binds source HEAD, protocol 5, master schema 19,
provider/model, four packet/output digests, and live time. The production path
must approve a fixed specification-matching candidate and reject a fixed
mismatch. The controller revalidates the repository and five committed
definition blobs, hashes and removes its private transcript, then atomically
publishes a bounded path-free schema-v1 receipt and raw digest under
`target/review-provider-live-proof`. It never posts that digest to Windows.
This proves selected-provider integration and two-case semantic sanity, not
general review quality, an actual queued feature review, activation admission,
GitHub publication, or production readiness. Protocol v5 and schema v19 remain
unchanged.

#### GitHub Publication Live Proof

Feature 4 connects the existing schema-v17 coordinator to one fixed Windows
GitHub transport without adding a caller-selected repository, executable,
credential, command, or evidence seam. Provisioning pins `gh.exe` and
`git.exe`, the running master executable, the public `malak333/Assemblywright`
repository, base branch `main`,
the internal `release-local` and `protocol-windows` check identifiers and their
exact GitHub Actions context names, workflow IDs and paths, trusted workflow
file digests, and an owner/SYSTEM-private GitHub CLI
configuration. Authentication must already be valid for the fixed owner
account; raw tokens are never copied into the repository, master data, argv,
environment, logs, audit, controller transcript, or proof receipt.

The production adapter derives all operations from the master plan. It pushes
only the exact frozen candidate commit from the reviewed live base, upserts only the derived feature branch
pull request, requires current branch protection with strict required checks
and administrator enforcement, never requests an admin merge, waits with
deadline/cancellation/current-authority polling, verifies the PR head and
normal configured merge result, reconciles exact remote `main`, and treats only
the exact successful `release-local` check on that merged commit as the fixed
post-merge gate. The owner-loopback route serializes publication, releases the
master process lock around external work, and rechecks durable authority before
accepting every adapter result. Any missing, stale, cancelled, deadline-expired,
or ambiguous result is quarantined with `effect_possible:true`; remote mTLS
still has no publication route.

Provisioning and proof execution are mutually exclusive, transactionally
rollback service/configuration changes, and fail for owner reconciliation on
stale recovery state. The live proof uses the same provisioned adapter against a disposable bounded
metadata-only proof-marker commit based on exact published `main`. The
protected pull request intentionally advances `main` with that durable public
proof marker and no product-code change. The Mac controller binds the starting
source commit, exact resulting protected merge commit, both hosted check
contexts, protection/no-bypass observation, pinned tool and service digests,
and a private transcript digest into one path-free owner-only receipt. It does
not enqueue or mutate Feature Conveyor state, admit its own receipt, or claim
general publication, signing, notarization, clean-profile, or production
readiness.

Prepare is resumable without adding master authority. The Windows disposable
checkout marker is byte-validated and published by same-directory atomic rename
before grants, durably binding the generated repository/feature IDs and exact
queue, Emergency Pause, owner-designation, source, and commit baseline before
grant creation. A retry reconstructs the identical approved request and creates
only missing exact revision-1 grants; exact existing grants must match every
digest and state field. If the helper enqueue committed but its receipt was
lost, retry accepts only one exact queued lifecycle-revision-1 feature matching
the marker and baseline-plus-one queue revision, then continues without issuing
a duplicate enqueue. Any other partial or drifted state fails closed and remains
available to the existing explicit Cleanup action. The narrow pre-marker crash
window is recoverable only by deleting an exact clean, normal-index clone whose
exact remote-name set is `{origin}` and whose sole origin URL still equals the
fixed source while the queue remains empty.

The concrete authority endpoints are loopback-only
`POST /v1/feature-conveyor/activation-evidence` and exporter-bound designated-
bridge `GET /v1/distributed/feature-conveyor/owner-control`, plus remote POSTs
for `activation`, `orchestration/pause`, `orchestration/resume`,
`cancel-active-feature`, and `abandon-and-advance`. The signed helper exposes
each mutation only as an explicit bounded `--confirm` command. The SwiftUI app
stops and reaps monitoring, revalidates that same helper, owns one command child,
strictly validates its receipt, and restores observation.

#### Evidence Gate

The schema-v15 gate accepts only the approved manifest's exact ordered plan of
13 protocol-owned identifiers: requirements binding, coverage, focused unit
tests, native E2E, documentation, knowledge base, formatting, lint, build,
safety, changed paths, secret scan, and repository validation. Missing,
reordered, truncated, zero-digest, or failed required evidence is a rejection
rather than a warning. Only all 13 passes advance the exact frozen candidate
from `validating` to `reviewing`; a recorded failure remains `validating`.
SQLite retains immutable result digests and bounded metadata, not raw output.
The production command runner is connected only on Windows when the fixed
private toolchain and credential-free dependency-cache inputs validate. It
executes the master-owned checks and fixed offline Cargo commands against a
verified disposable copy; otherwise it returns
`validation_runner_unavailable` before durable attempt or audit mutation. The
fixture proves the containment and command contract, but not a populated real
toolchain/cache, installed-service identity, or actual above/below-threshold
llvm-cov behavior.

#### Review Gateway

The gateway opens a fresh response-only session using the active feature's
bound provider. It sends the approved final specification, exact candidate
commit and diff, and bounded evidence manifest. It sends no implementation
transcript, raw brainstorming transcript, or canonical Assemblywright memory.
Approval admission rejects sensitive transcript-, memory-, credential-, and
secret-shaped manifest content and any unknown field through a fixed review-safe
manifest DTO before it can become reviewable; the gateway repeats that check for
migrated pre-v16 specification rows.

The review-safe DTO permits only `acceptance`, `outcome`, `title`, `scope`,
`allowed_paths`, exact `validation_gate`, assumptions, risks, non-goals,
decisions, required capabilities, unit/E2E/documentation/knowledge obligations,
prohibited-data declarations, publication checks, base branch, security
classification, merge strategy, and post-merge gate. Each is bounded and typed;
unknown or structurally free-form fields reject.

The implemented schema-v16 owner-token loopback action accepts only strict
path-free identifiers, revisions, digests, provider/model, grant, lifecycle,
queue, and Emergency Pause bindings. The master reconstructs the packet from
immutable SQLite rows and the verified frozen candidate. A missing production
adapter is unavailable by default. A configured adapter is one canonical fixed
executable launched once per call with a cleared environment, bounded pipes,
timeout, and complete process-tree termination before pipe joins. On Windows,
the master holds a verified no-write/no-delete executable handle while a trusted
gate-blocked launcher enters the kill-on-close Job Object before provider spawn;
the provider's only argument is the separate fixed
`--count-tokens` preflight. Mechanical capacity, adapter token-count, and strict-
output checks complete before the call intent is stored. Calls and outcomes are
append-only. Provider outage, malformed output,
or incomplete transport records a fixed outcome and 1-, 5-, or 15-minute
backoff without consuming repair. Three candidate attempts and twelve feature
calls are hard ceilings. The structured result must bind the exact packet,
provider/model, ordered evidence digests, blocking and non-blocking findings,
the complete ordered identifiers from the required top-level approved-manifest
`acceptance` array, and
knowledge-base determination. Every finding and determination must reference
an admitted packet evidence digest. Approval cannot carry
a blocker or uncovered requirement. Rejection remains `reviewing` with the
active lease; repair orchestration is not part of this slice. Approval alone
atomically records immutable decision/transition evidence and advances
`reviewing -> publishing`. The route is absent from enrolled-device mTLS and
grants no publication action.
An interrupted in-flight call, including post-response authority drift, records a terminal immutable outcome and
quarantines the feature without automatic retry.

#### Publication Coordinator

The coordinator is the sole boundary that may later own GitHub credentials and
external publication. Schema v17 creates immutable exact-action intents before
branch push, pull-request create/update, required-check observation, reviewed-
head verification, merge, remote-main reconciliation, and the fixed post-merge
gate. Its request carries only path-free authority bindings; adapter evidence is
internal and never caller supplied. Every stage binds exact remote base,
reviewed head, PR identity, complete named-check digest/pass state, branch-
protection/no-bypass state, merge strategy/resulting main, and fixed post-merge
gate identity/result. Adapter execution has a bounded deadline and must poll
current authority/cancellation around external work. The production adapter is unavailable by
default. Ambiguity, cancellation, pause, drift, or restart after intent
creation quarantines without automatic retry. Workers and reviewers cannot
publish, and branch-protection bypass is never authorized.
A durable merge intent means merge is possible for later abandonment, including
quarantine directly from `publishing`; verified healthy-main reconciliation is
then mandatory and `merged:false` is rejected.

### Mac App

The Mac Developer Mode surface is an authenticated control and observation
client. It hosts the manual brainstorming session, previews the final
specification and digest, and exposes the owner-only `Approve and Enqueue`
action. It does not become queue or repository authority.

The queue UI shows capacity, order, dependencies, the active feature,
specification revision, global and active bound providers, branch, stage,
workers, job slots, repair candidates, active-processing time, validation,
review, pull-request, merge, and post-merge evidence.

Every blocked state uses a plain-language reason and one exact next owner
action. The UI distinguishes:

- Cancel: stop work without authorizing queue advancement.
- Abandon and advance: record explicit non-approval and release the queue only
  after safe reconciliation and, after any merge, verified healthy `main`.
- Global provider change: affects future features.
- Active provider rebind: creates a new explicit attempt for the active
  feature.

Repository registration, cloud-content disclosure, and autonomous publication
grants appear together with independent status and revision.

## Authority Model

Three independent owner grants exist per repository:

1. **Registration grant:** authorizes Assemblywright to recognize and inspect the
   repository under declared path and workflow policy.
2. **Cloud-content disclosure grant:** authorizes eligible non-secret repository
   artifacts to reach Codex for the hosted planning session or final review.
3. **Autonomous publication grant:** authorizes exact branch push,
   pull-request, and merge actions under declared branch protection and
   validation policy.

Each grant has its own revision, scope, expiry, and revocation state. One grant
never implies or broadens another.

Secret detection blocks cloud transport and identifies affected paths without
exposing values. Appropriate untracked credential patterns must be added to
`.gitignore`, but `.gitignore` is remediation hygiene rather than a security
boundary. Tracked or historical exposure also requires rotation or another
explicit resolved-security record.

The hosted brainstorming session may use Codex only when the disclosure grant
is current. Owner-authored conversational turns are explicit session traffic.
The final review does not automatically replay that transcript. Conversation
details omitted from the approved final specification cannot be recovered or
evaluated by the final reviewer; the owner explicitly accepted this limitation.

## Approved Feature Specification

A queue-eligible feature contains:

- Confirmed Understanding Lock, assumptions, risks, non-goals, accepted design,
  and Decision Log.
- Intended outcome, bounded scope, and observable acceptance criteria.
- Repository, base branch, allowed paths, and dependencies.
- Required local-worker capabilities and bounded work decomposition.
- Exact repository validation contracts and focused unit-test obligations.
- Required E2E scenarios and their proof boundaries.
- Documentation and knowledge-base review obligations.
- Security classification, prohibited data, and the three grant revisions.
- Publication requirements, required checks, merge strategy, and post-merge
  gate.
- Planning provider/model provenance and the final document digest.

`Approve and Enqueue` binds the owner decision to the exact structured
manifest, design digest, repository identity, grant revisions, and queue
position.

Specifications are immutable. Corrections create a new numbered revision with
a new owner approval. An active feature requiring amendment moves to
`attention-required`, closes worker leases, preserves inspectable state, and
waits for another completed brainstorming workflow. Accepting a revision
invalidates affected evidence and requires revalidation.

Workers may report `specification_ambiguous` but cannot modify, reinterpret, or
broaden the specification. Material ambiguity blocks the conveyor until the
owner approves an amendment.

## Feature Lifecycle

The normal lifecycle is:

```text
queued
  -> implementing
  -> validating
  -> reviewing
  -> publishing
  -> verifying_main
  -> succeeded
```

Supporting states are:

- `repairing`: a replacement candidate is being built after substantive
  integration, validation, review, hosted-check, or post-merge failure.
- `paused`: safely resumable provider, worker, maintenance, provider-budget, or
  owner interruption.
- `attention-required`: ambiguity, exhausted repair budget, unresolved secret
  remediation, incompatible repository policy, or another exact owner decision.
- `quarantined`: restart or external-effect reconciliation cannot prove safe
  resumption.
- `cancelled`, `failed`, `abandoned`, and `succeeded`: recorded outcomes.

Only `succeeded` automatically releases the active-feature lease and dispatches
the next dependency-ready feature. Cancellation or failure remains blocking
until the owner invokes `abandon and advance` after reconciliation. If any
feature commit reached `main`, abandonment remains unavailable until a
corrective or revert pull request restores and verifies healthy `main`.

The initial integrated candidate does not consume a repair cycle. Every new
candidate commit produced after a substantive failure consumes one of three
cycles, regardless of how many bounded worker jobs produce it. A fourth
replacement requirement moves the feature to `attention-required`.

Active-processing time includes implementation, integration, validation,
review, repair, publication, and post-merge verification. Recorded
infrastructure outages, unavailable workers, provider backoff, maintenance,
and owner pauses suspend the clock. Checkpointing prevents restart from
resetting or double-charging time.

Every transition uses compare-and-set revisions and records actor, reason,
specification digest, repository snapshot, grant revisions, provider/model
identity where relevant, accepted evidence digest, timing change, and
effect-possible status.

## Local Worker Collaboration

The orchestrator converts the approved specification into bounded work packets
with inputs, outputs, allowed paths, dependencies, validation commands, and
completion criteria. Decomposition may refine execution order but cannot add
requirements.

Up to three local coding agents may run concurrently. Each receives a
self-contained repository snapshot rather than a linked Git worktree. The
snapshot has independent Git metadata, no remote, no credentials, and no path
to the canonical repository.

Workers return bounded results containing:

- Commit or patch digest.
- Files touched.
- Tests run and normalized outcomes.
- Specification coverage claims.
- Assumptions, failures, or ambiguity.

The master verifies task, step, attempt, lease, specification revision, base
snapshot, allowed paths, result size, and digest. It serially imports accepted
commits or patches into the master-owned feature integration worktree.

Write packets run concurrently only when their declared path ownership does not
overlap. Semantic or textual conflicts produce a new bounded local repair
packet; Assemblywright never silently chooses one result. Workers exchange no hidden
peer-to-peer conversation or authority.

Before final validation, all worker leases close, the integration worktree is
frozen to one exact candidate commit, and every accepted artifact remains
traceable to its worker attempt and specification revision.

## Validation And Review

The Evidence Gate requires:

- Coverage of every acceptance criterion.
- Focused Rust and Swift unit tests for success, rejection, boundary,
  cancellation, concurrency, and recovery behavior.
- Relevant E2E coverage across Assemblywright's real boundaries: cross-process CLI,
  distributed runtime, packaged app, Git fixture, or live device as applicable.
  Playwright is required only for an actual browser surface.
- Required documentation changes and documentation-contract validation.
- A knowledge-base change when the approved final specification and
  implementation evidence support one, or a reviewer-accepted
  `no_new_knowledge` determination.
- Formatting, linting, build, safety, secret, changed-path, and canonical
  repository gates.

The final reviewer decides the knowledge-base outcome using only the approved
specification, exact diff, and implementation evidence. The owner rejected a
mandatory pre-enqueue knowledge-candidate artifact. Consequently, conversation
details omitted from the approved specification cannot be recovered during
review.

The fresh reviewer returns a strict `approved` or `rejected` decision with
blocking findings, non-blocking observations, requirement coverage, evidence
digests, and the knowledge-base determination. It cannot waive deterministic
gate failure.

Malformed output, provider outage, or incomplete transport pauses review
without consuming a repair cycle. Each candidate permits at most three
transport attempts with 1-, 5-, and 15-minute backoff. Twelve total review calls
are allowed per feature. Budget exhaustion pauses the feature until owner
resume or provider rebinding.

Approval becomes durable only when the specification, candidate commit,
provider binding, grant revisions, review packet, and repository state still
match.

## Publication And Advancement

Publication requires a durable approval for the exact candidate. The
coordinator rechecks grants, branch policy, paths, secrets, remote base, and
publication authority before creating an external intent.

The coordinator:

1. Pushes the feature branch.
2. Creates or updates its pull request.
3. Waits for every required hosted check.
4. Confirms the pull-request head still equals the reviewed commit.
5. Merges using the repository-approved strategy.
6. Resolves and verifies the exact resulting `main` commit.
7. Runs the declared post-merge gate from that commit.
8. Marks the feature `succeeded`, releases the lease, and dispatches the next
   dependency-ready queue head.

Any candidate, specification, evidence, grant, provider, pull-request head, or
base change invalidates approval and requires fresh validation and review.

Code-caused hosted or post-merge failure returns the feature to `repairing` and
requires a replacement candidate. Infrastructure failure pauses without
charging a repair cycle. Post-merge correction uses another branch and pull
request; it never rewrites published history.

Ambiguous push, pull-request, or merge outcomes are reconciled against GitHub
before any retry. Branch protection remains authoritative and is never weakened
or bypassed.

## Recovery And Persistence

Startup reconciles the active lease, specification, worker attempts, snapshots,
integration worktree, candidate commits, provider calls, time budget, review
evidence, Git intents, pull request, merge state, and remote `main` before
accepting queue mutation or dispatch.

Automatic retry is allowed only when evidence proves that repetition cannot
duplicate an effect. Abandoned stateless inference may receive a new attempt
only after the prior lease is durably closed. Ambiguous repository writes,
provider output, publication, or post-merge state quarantines the feature.

Feature records, grants, leases, budgets, and intents use versioned schema
migrations. Upgrade behavior is:

1. Stop dispatch and publication.
2. Create and verify an encrypted pre-migration backup.
3. Apply the migration transactionally.
4. Reconcile all Feature Conveyor records.
5. Resume only after schema and state validation succeed.

Migration failure blocks startup. Rollback restores the pre-migration backup
instead of attempting reverse writes. Existing RPO, RTO, backup-custody, and
restore-drill proof boundaries remain authoritative.

Schema v7 adds the durable `emergency_pause_revision` under this backup-first
path so schema-v6 binaries reject the forward database instead of changing
pause state without advancing the advisory snapshot revision.

Schema v8 adds the nullable owner-control MacBridge designation and its
compare-and-set revision under the same backup-first path. Older binaries reject
the forward database instead of accepting an approved-feature mutation without
the exact designated-device boundary.

Emergency Pause blocks new leases and publication, cancels safe active work,
and marks potentially effectful interruptions for review.

## Testing Strategy

### Unit And Concurrency Tests

Focused Rust and Swift suites cover:

- Queue capacity, owner ordering, dependencies, and stale revisions.
- Singleton active-lease races and invalid transitions.
- Repair-candidate and active-time accounting across pauses and restarts.
- Provider snapshot, backoff, call budgets, outage, and explicit rebinding.
- Worker cancellation, late results, snapshot confinement, path ownership, and
  integration conflicts.
- Grant separation, revocation, and exact authority checks.
- Secret admission and redacted audit output.
- Evidence binding and approval invalidation.
- Publication-intent reconciliation and atomic queue advancement.
- Backup-first migration success, failure, and restore.
- Swift owner-control gating, stale UI mutations, and exact blocker guidance.

### Deterministic E2E

A real master, multiple fake local coding agents, fake review providers,
self-contained repository snapshots, an integration worktree, and a controlled
Git remote prove:

1. Owner-approved specification through parallel work, validation, review,
   publication simulation, verified `main`, and next-feature dispatch.
2. Reviewer rejection, replacement candidate, and fresh approval.
3. Budget exhaustion, ambiguity, dependency failure, and secret detection block
   the queue.
4. Worker and provider outages pause without fallback or incorrect budget use.
5. Restart at worker, review, migration, publication, and post-merge boundaries
   either resumes safely or quarantines.
6. Raw brainstorming transcripts and canonical Assemblywright memory cannot enter the
   final review packet.
7. Changed commits, grants, provider bindings, or evidence invalidate approval.
8. Cancellation does not advance; explicit abandonment does, but never while
   merged `main` is unhealthy.

### Performance And Live Evidence

The p95 control targets are measured under the representative full-load
workload defined above.

Autonomous activation additionally requires separately recorded functional
live evidence for:

- Real restricted local coding agents.
- The selected review provider.
- GitHub branch, pull-request, required-check, merge, and reconciliation
  authority.
- Restart recovery.
- Live Mac/Windows queue control and event streaming.

Developer ID signing, notarization, clean-profile installation, unattended
long-duration reliability, and broader product production readiness remain
separate claims.

## Bootstrap And Activation

The Feature Conveyor must not publish its own initial implementation.

Implementation and merge use the existing human-supervised branch and
pull-request workflow. Autonomous dispatch and publication remain default-off.
Deterministic repository gates and the required functional live evidence must
pass before the owner receives an explicit activation preview and control.

Partial mechanics must not be presented as an autonomous development system.

## Accepted Risks

- Owner selection alone determines local reviewer quality. Mechanical context
  and structured-output compatibility do not prove review competence.
- A repository cloud-disclosure grant permits broad non-secret source and
  documentation exposure to Codex. Secret scanning can miss novel or obfuscated
  credentials.
- One blocked active feature stops all autonomous feature throughput.
- Fresh-context review is not provider diversity.
- Parallel patches may conflict semantically even when paths do not overlap.
- Codex and GitHub outages can pause completion indefinitely.
- Post-merge repair cannot erase published history.
- Windows remains a single point of availability.
- Active-time accounting permits indefinite calendar delay while paused.
- Review cannot recover useful conversation details omitted from the approved
  final specification.

## Decision Log

| Decision | Alternatives considered | Objection or trade-off | Resolution and rationale |
| --- | --- | --- | --- |
| Dedicated durable Feature Conveyor | Generic task graph; GitHub-backed queue | A feature carries approval and publication invariants that generic tasks obscure. | Accepted as the clearest auditable boundary. |
| Capacity of 100 without automatic refill | Maintain 100 ready features; automatic planning | Automatic generation conflicts with manual brainstorming and owner scope control. | Only owner-approved features enter; full capacity fails atomically. |
| One active feature with up to three local coding agents | One worker total; multiple active features | Head-of-line blocking reduces throughput. | Accepted to preserve exact final-review and publication sequencing. |
| Assemblywright-hosted manual brainstorming | Artifact import; automatic planning | Import could become error-prone; automatic planning violates owner control. | Hosted owner conversation produces the final preview and explicit approval. |
| Owner-approved specification before enqueue | Draft queue entries; automatic enqueue | Workers must not need clarification. | Immutable approved revisions are the execution boundary. |
| One global planning/review provider | Per-feature provider; automatic routing | Outage blocks the queue and a weak owner-selected local model may review. | Predictability and owner choice were preferred; no silent fallback. |
| Local implementation only | Codex full-agent implementation; same provider for all roles | Existing stateless workers cannot safely edit repositories. | Add a restricted local coding-agent capability; Codex never writes. |
| Self-contained worker repository snapshots | Linked worktrees; non-Git scratch directories | Linked worktrees escape the claimed path boundary through shared metadata. | Independent Git metadata and no remote make the worker boundary explicit. |
| Three grants | Combined repository grant; per-feature grants | Registration, disclosure, and external effects must not imply one another. | Independent revisioned grants preserve least authority. |
| Repository content may reach Codex except credentials and secrets | Preserve per-feature sensitivity gates; include everything | Broad disclosure is meaningful and scanners are imperfect. | Owner accepted repository-level disclosure; secret findings block transport and require remediation. |
| Final review excludes raw transcript and canonical memory | Send transcript; pre-enqueue knowledge artifact | Reviewer may miss conversation details absent from the specification. | Owner explicitly accepted review from specification, diff, and evidence only. |
| Any owner-selected local model may review | Qualification benchmark; per-feature approval | Review quality is not guaranteed. | Mechanical envelope and output compatibility are required; quality remains owner risk. |
| Provider-independent exact-review envelope | Review-time failure; hierarchical partial review | Oversized exact input cannot be reviewed truthfully. | Reject at enqueue and split the feature during brainstorming. |
| Fresh response-only final review | Reuse implementation context; provider-diverse review | Same provider is not independent in the diversity sense. | Fresh context plus deterministic gates provides context separation only. |
| Replacement-candidate repair accounting | Failure-event; worker-attempt counting | Internal attempts could consume the budget unpredictably. | Initial candidate is free; each later candidate consumes one of three cycles. |
| 24 active-processing hours | Wall-clock deadline; unlimited work | Pauses may block the queue indefinitely. | Infrastructure and owner pauses do not consume work budget; indefinite calendar delay is accepted. |
| Bounded provider retries | Unlimited backoff; feature-specific budgets | Free retries can cause call storms. | Three attempts per candidate, twelve total calls, fixed backoff and envelopes. |
| Reviewer approval may merge autonomously | Owner approval for each merge; direct push | Model approval carries quality risk. | Deterministic gates, exact evidence, separate publication grant, and branch protection remain mandatory. |
| PR then merge and verify `main` | Direct push; repository-specific publication | Post-merge failure cannot be undone safely. | Correct through another PR and keep the same feature active. |
| Cancellation requires explicit abandonment to advance | Cancellation advances; cancellation blocks forever | Automatic advancement bypasses reviewer approval. | Separate owner-authenticated abandonment records non-approval and requires reconciliation. |
| Merged code must be healthy before abandonment | Risk acceptance; work on another repository | Reconciliation alone does not prove repository health. | Correct or revert and verify `main` before releasing the queue. |
| Safe restart reconciliation | Restart whole feature; fail and advance | Effect ambiguity can duplicate writes or publication. | Resume only proven-safe work; otherwise quarantine and block. |
| Backup-first versioned migrations | Export/rebuild; forward-only repair | Durable queue recovery depends on compatible schema state. | Transactional migration and verified backup restore fail closed. |
| Owner-controlled order and dependencies | FIFO; model reprioritization | A blocked head stops unrelated work. | Owner intent and strict sequencing take priority. |
| Owner-only administration | Delegated admins; model-managed queue | Models must not manufacture authority. | All authority mutations require the authenticated owner. |
| Native Assemblywright E2E | Browser-first Playwright; unit-only proof | Playwright does not cover Rust/Swift distributed boundaries. | Use cross-process, distributed, packaged-app, Git, and live-device lanes; use Playwright only for a browser surface. |
| Manual supervised bootstrap | Immediate dogfood; repository-only release | The conveyor cannot prove itself with its own unproven authority. | Existing human-controlled PR workflow, default-off mechanics, live proof, then explicit activation. |
| Functional live activation evidence | Repository gates only; full Apple release evidence | Repo gates cannot prove provider, GitHub, or two-device behavior. | Require functional live proof; keep signing/notarization and production readiness separate. |

## Structured Review Resolutions

### Skeptic / Challenger

The Skeptic returned `REVISE`. All blocking objections were accepted and
resolved:

- Worker repository authority became an explicit restricted local coding-agent
  boundary.
- Repository registration, cloud disclosure, and publication became separate
  grants.
- Cancellation no longer advances automatically; owner abandonment is a
  distinct resolution.
- Final cloud review context was made explicit.
- Exact-review feasibility gained hard admission ceilings.
- Repair accounting and bootstrap authority were specified.

### Constraint Guardian

The Constraint Guardian returned `REVISE`. All blocking objections were
accepted and resolved:

- Linked worktrees were replaced with self-contained worker snapshots.
- Abandonment after merge now requires healthy verified `main`.
- Durable migrations became backup-first, transactional, and fail-closed.
- Provider retries gained fixed call, token, output, and backoff bounds.
- Performance, privacy-negative, and functional live evidence became explicit
  verification criteria.

### User Advocate

The User Advocate found two blocking usability gaps:

- The manual brainstorming-to-enqueue flow was accepted and resolved through a
  Assemblywright-hosted owner session and digest-bound preview.
- The proposed mandatory pre-enqueue knowledge artifact was explicitly rejected
  by the owner. The final reviewer decides the knowledge-base outcome from the
  approved specification, exact diff, and implementation evidence. The
  resulting inability to recover omitted conversational knowledge is recorded
  as an accepted limitation.

Non-blocking UI objections were accepted: each blocked state has one plain
next action; cancellation and abandonment are visually distinct; the three
grants are shown together; global and active providers are distinguished; and
evidence failures identify exact missing obligations.

### Integrator / Arbiter

The Arbiter first returned `REVISE` because sending the raw transcript
contradicted the accepted cloud-disclosure boundary. The owner corrected the
contract to exclude raw transcripts and canonical memory from final review and
rejected the mandatory pre-enqueue knowledge artifact.

The Arbiter then returned `APPROVED`:

- Understanding Lock confirmed.
- Dedicated Feature Conveyor approach accepted.
- Assumptions and risks acknowledged.
- Skeptic, Constraint Guardian, and User Advocate invoked sequentially.
- All objections resolved or explicitly rejected.
- Decision Log complete.
- No unresolved blocking issue remains.

## Historical Schema-v12 Disposition

The schema-v12 slice was approved. The Windows-master repository kernel, owner resolution, snapshot
dispatch, and fixed contained-coding live lane are implemented. The 2026-08-11
two-device proof used Windows snapshot source `80fed217`, the production
Apple-development-signed helper built from `01afff03`, and the real supervised
Rust agent. The recovered terminal suffix was lease sequence 3824 followed by
success sequence 3825 on one connection; both Mac snapshot directories and
Windows transfer staging were empty. Owner cancellation retained the lease,
abandonment released it with queue-empty reconciliation, and cleanup revoked
all three repository grants before removing the marker-bound checkout. General
coding was outside that historical proof; review, publication, autonomous
activation, and external Apple release evidence remain outside the current
protocol-v5/schema-v13 proof.
