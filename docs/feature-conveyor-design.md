# Durable Feature Conveyor Design

Status: APPROVED by owner and structured multi-agent design review

Date: 2026-07-24

Scope: Approved target design plus the bounded repository-kernel implementation
status below. This document does not claim autonomous activation, live-device
proof, or production readiness.

Implementation status: the first default-inert Windows `assemblywright-master`
repository kernel is implemented as master schema v5. It covers immutable
approved specification revisions, the bounded owner-ordered queue, dependency
blocking, compare-and-set revisions, one active lease, exact lifecycle
advancement, cancellation without advancement, explicit safe abandonment,
startup quarantine, and same-transaction redacted audits. It exposes the exact
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
strictly decodes the bounded schema-v8 allowlist, cancels on drift, and includes
the projection only in authenticated snapshots. The app renders a compact
read-only queue/guidance section and never presents `next_owner_action` as a
button. Neither observation route grants enqueue, execution, review,
repository, Git, publication, audit-event, or activation authority. No worker
dispatcher, repository execution, review provider, publication coordinator,
Mac control UI, or autonomous activation is implemented. The remainder of this
document is still target design.

Master schema v8 adds the first bounded owner-control transport without
activating the conveyor. An owner-token-authenticated Windows loopback action
designates one exact current, enrolled, non-fixture `MacBridge` using a durable
compare-and-set designation revision and same-transaction redacted audit. Only
Two additional owner-token loopback-only routes record and inspect the three
independent repository grants without accessing a repository. Each strict
digest-only mutation must be the contiguous next revision, match the current
grant and Emergency Pause revisions, and commit with a redacted audit. Active
grants fail while paused; a revocation revision remains available. The current
projection exposes only digests, revision, expiry, revocation, and computed
active state, and both routes are absent from enrolled-device mTLS. Only the
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
autonomous activation. The SwiftUI app remains observation only.

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

#### Evidence Gate

The gate assembles digest-bound evidence for requirements, unit tests, E2E
tests, documentation, knowledge-base outcome, formatting, linting, builds,
safety rules, changed paths, secret scanning, and repository validation.
Missing required evidence is a rejection rather than a warning.

#### Review Gateway

The gateway opens a fresh response-only session using the active feature's
bound provider. It sends the approved final specification, exact candidate
commit and diff, and bounded evidence manifest. It sends no implementation
transcript, raw brainstorming transcript, or canonical Assemblywright memory.

#### Publication Coordinator

The coordinator alone owns GitHub credentials and external publication. It
creates durable exact-action intents for branch push, pull-request mutation,
merge, and remote reconciliation. Workers and reviewers cannot publish.

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

## Final Disposition

APPROVED. The Windows-master repository-kernel slice above is implemented; all
execution, review, publication, UI, live-evidence, and activation layers remain
design.
