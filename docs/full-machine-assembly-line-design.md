# Full-Machine Assembly Line Design Proposal

Status: blocked draft; not accepted architecture or implementation authority

This document preserves owner-requested product intent for future design work.
It does not amend `DESIGN.md`, `docs/safety-rules.md`, protocol contracts, or the
current bounded executor. Its structured review records an unresolved blocker:
literal machine-wide authority on the Windows control host can bypass the master,
audit, credentials, and pause enforcement. No UI or runtime may claim or grant the
authority described below unless that conflict is resolved in an accepted design,
implemented with fail-closed boundaries, and proven through the required native and
live evidence.

## Understanding Summary

- The primary UX has three actions: New Project, New Feature, and Assembly Line.
- Repositories are entered and displayed as canonical GitHub URLs. Windows keeps an
  immutable internal repository identity mapped to the URL.
- New Project and New Feature run a built-in structured brainstorming workflow with
  an explicitly selected, configured orchestrator profile. The orchestrator plans
  only and receives no machine-write authority.
- An owner-approved project creates a GitHub repository at the entered URL. Public is
  the default visibility; the UI may select Private before approval.
- An owner-approved feature is appended to the Windows-authoritative FIFO queue.
- Starting the assembly line grants a signed local executor autonomous full-machine
  authority across the registered Windows and Mac hosts. There are no per-action
  approval prompts while that execution epoch remains active.
- Only one feature is active. Auto-run defaults on and advances only after complete
  implementation, validation, independent review, publication, and final-main
  verification. Stop and Emergency Pause preserve resumable durable state.

## Explicit Authority

Full-machine authority includes file reads and writes, process execution, network,
credentials, system configuration, destructive operations, and external effects on
both registered machines. It belongs only to the signed local executor. It never
belongs to the planning orchestrator.

The owner grants this authority by starting a nonempty assembly line. The grant is
bound to one Windows-issued execution epoch, the designated Windows and Mac
executors, the queue revision, and the active feature. An executor must stop when the
epoch, designation, connection, or feature binding becomes invalid.

## User Experience

### New Project

1. Enter a canonical `https://github.com/<owner-or-org>/<repository>` URL.
2. Select Public (default) or Private.
3. Select one explicitly configured orchestrator provider/model profile.
4. Enter a short project idea and start brainstorming.
5. Review the bounded project specification.
6. Approve and Create.

Only the final approval permits Windows to use its existing authenticated GitHub
credentials to create the repository, initialize `main`, record the internal
repository identity and canonical URL, and retain the approved brainstorming digest.
An ambiguous GitHub response is reconciled against the exact owner, repository,
visibility, and request digest before any retry.

### New Feature

1. Select a repository by Git URL.
2. Select an orchestrator profile and enter the feature idea.
3. Run the same structured brainstorming workflow.
4. Review the generated feature specification.
5. Approve and Add to Assembly Line.

The normal UI does not expose UUIDs, grant revisions, raw evidence digests, allowed
paths, or validation-obligation fields. Full-machine scope is explicit. Testing,
documentation, knowledge-base, safety, publication, and final-verification
obligations are derived from the approved specification. Internal identities and
evidence remain available under diagnostics.

### Assembly Line

- Start is disabled while the queue is empty.
- Auto-run defaults on and can be disabled before or during execution.
- With auto-run on, a fully successful feature advances to the next queue item.
- With auto-run off, success leaves the next item queued until Start.
- Stop reaches the next declared durable checkpoint, terminates remaining executor
  processes, and preserves the feature for resume.
- Emergency Pause immediately terminates active executor processes and preserves the
  last durable checkpoint for later resume. It does not impose a separate network
  shutdown or automatic quarantine rule.

## Components And Data Flow

### Windows Master

Windows remains the sole durable authority. It owns repository URL registration,
GitHub creation intent and reconciliation, brainstorming requests and approved
artifacts, the FIFO queue, the global auto-run setting, assembly-line execution
epochs, checkpoints, lifecycle transitions, and redacted audit.

The orchestrator catalog is an explicit provisioned allowlist. The default is
`openai.codex` / `gpt-5.6-sol`. Selection is owner-visible and immutable for a
brainstorming attempt. There is no automatic fallback, general conversation runtime,
plugin marketplace, or model-controlled queue ordering.

### Brainstorming Gateway

The gateway is a built-in schema-bound workflow, not a dynamically loaded skill or
plugin. It accepts a bounded typed project or feature brief and returns a strict
project or feature specification. Provider output is never authority. Only owner
approval converts the frozen specification into a repository-creation request or an
enqueue request.

### Full-Machine Executor

The executor is a distinct signed runtime rather than a widening of the planning
orchestrator. It runs locally on both registered hosts and accepts work only for the
current Windows-issued execution epoch. All child processes belong to a boundedly
terminable process tree. Checkpoints commit before and after externally effectful
steps so restart can distinguish completed, not-started, and effect-possible work.

## Failure And Recovery

- Brainstorming failure creates neither repository nor feature.
- Repository creation persists intent before GitHub access and reconciles ambiguous
  results without blindly creating a second repository.
- Stop prevents new steps, reaches the next checkpoint, and terminates descendants.
- Emergency Pause terminates descendants immediately and retains the latest durable
  checkpoint.
- A disconnect pauses the feature. Resume requires both hosts to bind the same current
  execution epoch and checkpoint.
- Completed checkpointed actions are not repeated. Effect-possible actions are
  reconciled against external state before continuing.
- Auto-run never advances after failure, incomplete validation, rejected review,
  ambiguous publication, or failed final-main verification.

## Audit And Privacy

Audit records action identity, command identity, affected resource identity,
external-action type, timestamp, result, checkpoint, and digests. It structurally
excludes raw credentials, secret values, provider prose, and file contents. Planning
and execution records remain separate.

## Non-Functional Requirements

- Scale: one owner, two registered machines, one active feature, at most 100 queued or
  active features.
- Reliability: durable intent before possible external effect; exact reconciliation;
  no blind retry; checkpointed resume.
- Responsiveness: local UI state changes are immediate; process termination uses a
  bounded graceful-to-forced shutdown and reports incomplete termination.
- Security: planning has no execution authority; execution authority is explicit,
  epoch-bound, revocable, and auditable even though its granted scope is machine-wide.
- Maintainability: strict versioned protocol types, database migrations, native Rust
  and Swift tests, and no dynamic provider or skill loading.

## Native Verification Strategy

- Swift unit tests cover the simplified forms, URL/visibility validation, default
  orchestrator, queue-empty Start gating, auto-run default/toggle, Stop, and Emergency
  Pause presentation.
- Rust unit and kernel tests cover URL identity mapping, public/private creation
  intent, brainstorming freeze/approval, execution epochs, single-active FIFO,
  auto-run advancement, checkpoint resume, stale-epoch rejection, and redacted audit.
- Native process E2E uses disposable Mac and Windows fixtures to prove process-tree
  termination, checkpoint recovery, cross-host epoch binding, GitHub adapter
  reconciliation, and one-at-a-time advancement. It must not use real destructive
  machine-wide operations as repository-gate evidence.
- Live Windows service, GitHub creation, signed executors, and Mac/Windows full-machine
  operation remain separately owner-recorded evidence.

## Decision Log

| Decision | Alternatives considered | Resolution and rationale |
| --- | --- | --- |
| Git URL is the user-facing repository identity | Expose UUID; local path | URL matches owner intent; Windows retains an immutable internal UUID and URL binding. |
| Brainstorm before project creation or feature enqueue | Manual form; automatic generation | Structured brainstorming reduces UI complexity; owner approval remains the authority boundary. |
| Public repository is the default | Private default | Owner explicitly selected Public default with a visible Private option. |
| Orchestrator is planning-only | Give cloud model execution authority | Planning/execution separation keeps credentials and machine authority local. |
| Full-machine executor is a distinct runtime | Widen orchestrator; replace system with general assistant | A distinct signed runtime preserves Windows authority and avoids restoring a conversation/plugin runtime. |
| Start is the single execution authorization | Per-action confirmation | Owner explicitly selected autonomous operation with no prompts after Start. |
| One active FIFO with auto-run on by default | Parallel features; manual-only advancement | Matches the assembly-line mental model and preserves serial repository authority. |
| Stop checkpoints; Emergency Pause terminates processes | Always quarantine; network kill switch | Owner selected resumable termination without a separate network shutdown or quarantine policy. |
| Structurally redacted audit | Raw transcripts and file contents | Records accountability without retaining secrets or content. |

## Known Architecture Changes

This design deliberately replaces the current restricted-worker execution policy and
requires accepted revisions to `DESIGN.md`, `docs/safety-rules.md`, the protocol,
Windows schema and routes, the Mac app/helper, executor containment and recovery,
GitHub provisioning, release evidence, and native E2E. Until those changes and their
evidence are complete, the existing bounded implementation remains authoritative and
the new UI must not claim full-machine execution readiness.

## Structured Review Log

### Skeptic / Challenger

Disposition: changes required.

| Objection | Resolution status | Rationale |
| --- | --- | --- |
| A full-access Windows executor can rewrite the master, database, audit, credentials, and pause enforcement, collapsing Windows authority. | Unresolved blocker | Literal access to the enforcement substrate cannot coexist with an independently authoritative, fail-closed master on that same substrate. |
| Arbitrary privileged work can escape a terminable process tree through services, scheduled tasks, remote delegation, or supervisor replacement. | Accepted | The design cannot claim revocation or Emergency Pause coverage without an enforceable delegation boundary. |
| Arbitrary irreversible effects do not always have an exact reconciliation predicate. | Accepted | The generic resume promise is too broad; effect-possible steps without an exact adapter-owned reconciliation contract must stop for owner resolution. |
| Auto-run changes queue and feature bindings, invalidating a single feature-bound epoch. | Accepted | Start must authorize a bounded assembly-line session, while each feature receives a narrower child epoch derived from that owner-started session. |
| The executor has no strict executable action/provenance contract. | Accepted | A versioned action envelope, provenance rules, interception boundary, and adapter-specific reconciliation contract are required before execution. |

### Constraint Guardian

Disposition: changes required.

| Objection | Resolution status | Rationale |
| --- | --- | --- |
| Machine-wide authority makes authority and redacted audit self-bypassable on the Windows control host. | Unresolved blocker | Confirms the skeptic's P0 finding. |
| Stop and Emergency Pause cannot cover services, scheduled tasks, remote delegation, or a replaced supervisor. | Accepted | Termination claims must be limited to actions created through an enforceable executor boundary. |
| Recovery and provenance remain undefined for arbitrary actions. | Accepted | Execution must be rejected unless a versioned action type defines launch, audit, cancellation, and reconciliation semantics. |
| Performance, resource, provider-cost, retention, checkpoint-growth, and migration-rollback limits are not measurable. | Accepted | Numeric budgets and rollback criteria are required in the final design and tests. |

### User Advocate

Disposition: changes required.

| Objection | Resolution status | Rationale |
| --- | --- | --- |
| Auto-run off leaves the visible on/off and authorization state ambiguous. | Accepted | The UI must enter `waiting_for_owner_start` after success, show `Start next feature`, and create a new feature execution epoch under the still-configured but inactive line. |
| Recovery states and next owner actions are not visible. | Accepted | The Assembly Line must render closed states for stopping, checkpoint-paused, emergency-paused, host-reconnect wait, reconciliation-required, and incomplete termination. |
| Brainstorming and creation failure recovery is undefined. | Accepted | Each pre-effect failure must show a bounded reason plus Edit and Retry; URL conflict and effect-possible GitHub creation must use exact reconciliation or owner resolution rather than a blind retry. |
