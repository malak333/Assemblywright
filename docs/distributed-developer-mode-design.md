# Distributed Developer Mode Design

Status: APPROVED by owner and structured multi-agent design review; implementation planning permitted

Date: 2026-07-16
Scope: Design only; this document does not claim implementation or production readiness

## Understanding Summary

- Assemblywright will expose explicit Personal and Developer modes over one Rust-owned
  safety, policy, routing, audit, and unified-memory foundation.
- A Windows machine is the single stateful master. It owns repositories,
  worktrees, memory, indexes, orchestration, policy, audit, scheduling,
  builds/tests, Git publication, and Codex-account execution.
- The Windows RTX 3080 also provides logically separate stateless lightweight
  inference for autocomplete, small functions, and other low-context work.
- The existing Swift macOS app remains the primary interface, voice surface,
  and Apple-integration client. An app-supervised Rust agent preserves the
  local Mac IPC boundary, bridges to Windows, and provides stronger M1 local
  inference.
- Codex uses the owner's authenticated ChatGPT account through Codex CLI, not
  the OpenAI Platform API. Assemblywright supports both response-only and full coding
  agent Codex roles.
- Developer Mode may autonomously edit, validate, review, commit, push, open
  pull requests, and merge when an explicitly registered repository's policy,
  branch protection, required reviews, and validation gates authorize it.
- Workers receive bounded ephemeral task context and never own repositories,
  canonical memory, task state, credentials, or publication authority.

## Goals

- Preserve the existing Personal Mode feature surface while intentionally
  changing its availability contract: after Windows-master cutover, Personal
  Mode is unavailable whenever the master is offline.
- Make routing deterministic, explainable, policy-checked, and auditable.
- Use the Windows machine's CPU, RAM, repository tooling, and RTX GPU while
  using the M1 Mac for stronger local-model inference.
- Use Codex-account capacity for difficult coding, architecture, and review
  rather than introducing OpenAI Platform API billing or credentials.
- Support one user, two machines, four concurrent jobs, and future additional
  capability workers without prematurely building a fleet platform.
- Preserve fail-closed behavior, planning/action separation, cancellation,
  emergency pause, redaction, sensitivity controls, and evidence-scoped claims.

## Non-Goals

- OpenAI Platform API integration.
- Employer-managed computers, employer integrations, or work-domain data.
- Multi-master consensus, state replication, offline Mac operation, or master
  failover.
- Repository checkouts, durable memory, or publication credentials on workers.
- Dynamic or learned routing in the first release.
- Automatic worker installation or update, generalized fleet administration,
  or distributed storage.
- Silent fallback from a Codex-assigned task to a local model.
- New Personal Mode features during the Developer Mode buildout.

## Assumptions And Non-Functional Requirements

| Area | Requirement or assumption |
| --- | --- |
| Users | One owner and personal repositories only. |
| Initial topology | One Windows master, one co-located RTX worker, one Mac bridge/M1 worker. |
| Future scale | Additional capability workers may be enrolled later; Windows remains the only master in this design. |
| Concurrency | Up to four concurrent jobs; one full-agent Codex session by default; one publication or merge lease per repository. |
| Responsiveness | Warm-path p95 Mac UI acknowledgement under one second and routing decision under two seconds, measured over at least 100 requests with payloads up to 32 KiB and overlay round-trip latency at or below 100 ms. Cold model loading is excluded and reported separately. |
| Long work | Jobs may run for hours and must use durable master checkpoints and bounded attempts. |
| Availability | Windows is mandatory. When it is unavailable, the Mac app accepts no commands, inference, memory changes, or actions. |
| Connectivity | A private overlay supplies reachability; Assemblywright independently authenticates devices and roles. |
| Deployment | Matching versioned Mac and Windows builds are installed manually. Protocol mismatch blocks execution. |
| Privacy | Workers retain model weights and runtime configuration but no prompts, retrieved passages, source snippets, results, or repository state. |
| Cloud data | Registered repository context may reach Codex only after exclusions, sensitivity policy, and secret scanning. Personal or Private context requires exact task approval; Restricted context remains local. |
| Maintenance | Windows is administered through a local operator CLI; the first release adds no Windows GUI. |
| Recovery | After cutover, target RPO is 15 minutes and target RTO is four hours; release evidence requires a restore drill from independently stored encrypted backup material. |

The worker-retention requirement means no deliberate application persistence.
It does not claim that plaintext can never reach encrypted OS swap, crash
dumps, provider-internal caches, or hardware-managed memory. A runtime is
ineligible when its prompt logging or telemetry cannot be disabled and
verified. Worker hosts require full-disk encryption and crash-dump policy as
separate deployment evidence.

## Architecture

```text
Mac                                      Windows
┌─────────────────────┐                 ┌──────────────────────────┐
│ Assemblywright.app          │                 │ assemblywright-master            │
│ Voice / UX / Apple  │                 │ Tasks / policy / audit   │
└─────────┬───────────┘                 │ Memory / RAG / scheduler │
          │ Local UDS                    │ Repos / worktrees / Git  │
┌─────────▼───────────┐  Authenticated  │ Codex orchestration      │
│ assemblywright-agent        │◄───────────────►└────────────┬─────────────┘
│ Control bridge      │    overlay                    │ Loopback
│ M1 inference worker │                      ┌────────▼─────────────┐
└─────────────────────┘                      │ RTX inference worker │
                                             └──────────────────────┘
```

### Windows Master

`assemblywright-master` is the sole authoritative service. It owns:

- SQLite tasks, memory, policy, audit, scheduler, repository registry, leases,
  and migration state.
- Rebuildable retrieval indexes and repository snapshots.
- Deterministic task classification, routing, decomposition, and checkpoints.
- Worktree creation, validation, Git credentials, pull-request operations, and
  merge authority.
- Codex CLI authentication and both Codex execution adapters.
- Device enrollment, capability registration, revocation, health, and
  protocol compatibility.
- Emergency pause and recovery reconciliation.

### Mac App And Agent

The Swift app remains the primary human interface and owns native Apple UX,
voice, notification, and permission presentation. It communicates only with an
app-supervised Rust `assemblywright-agent` over the existing local UDS, bearer,
same-EUID, and Apple code-identity boundary.

The signed Mac bridge helper maintains the outbound authenticated connection to
Windows because it alone owns the non-exportable enrolled Keychain identity.
The directly supervised Rust agent owns the durable event cursor and will later
host eligible Apple capabilities and the M1 inference adapter. Neither holds
authoritative Assemblywright state.

The current default-inert implementation now extends the Mac trust/transport
foundation with a bounded Rust relay seam. Windows schema version 4 stores a
metadata-only event journal with one server-issued stream ID and contiguous
cursor. State transitions and their events commit atomically. The
`assemblywright-agent` executable stores only its last accepted cursor and serves
authenticated health and event acceptance over the shared owner-only,
same-EUID, Apple code-identity-checked UDS transport. Startup policy and its
fresh bearer arrive through bounded stdin, and a direct-parent mismatch fails
before storage opens.

The completed fixture slice adds a deliberately non-production execution proof.
An exact, separately enabled `fixture.reasoning` adapter accepts only a Public,
no-retention `synthetic_echo`, holds at most one active attempt in memory, and
returns the same bounded input in a typed synthetic result. The Windows master
is the only enqueue authority; raw remote enqueue is absent. Lease, result,
cancellation, acknowledgement, device, connection epoch, attempt, and digest
identity are bound end to end. A two-second cancellation acknowledgement
deadline, disconnect, expiry, maintenance, or emergency pause makes the attempt
non-accepting and suppresses late output. This adapter is transport/lifecycle
evidence only and grants no MLX, model, tool, file, repository, Codex, or Git
authority.

The completed bounded MLX slice adds the first real M1 inference worker without
expanding planning authority. A separately enabled standard-profile
`mlx.reasoning` lane accepts only Public, `ephemeral_no_retention`
`generate_text` requests with a nonempty 32 KiB prompt ceiling, 1 to 512
tokens, and 0 to 2000 milli-temperature. The selected model must match the
single registered `local_inference` / `mlx` capability. The Mac agent invokes
only its configured absolute `mlx_lm.generate` and local model-directory paths,
clears inherited environment, forces offline and telemetry-disabled operation,
passes the prompt on stdin, bounds stdout, and discards stderr. One process
group and one in-memory attempt are permitted. Timeout, cancellation, pause,
lease loss, or disconnect owns TERM-to-KILL reaping and suppresses simultaneous
or late output. Cross-language context and payload digests bind sorted-key
UTF-8 JSON with forward slashes left unescaped, matching the Rust
`serde_json` representation even for slash-bearing model identifiers and
bounded text. No prompt or output enters the durable cursor or event journal.
This is local LLM execution with frontier cloud review remaining a selective
separate layer; it adds no remote enqueue, repository, tool, credential,
network, Codex, Git, publication, or unattended authority.

The first Durable Feature Conveyor implementation is a separate default-inert
schema-v5 repository kernel retained by the schema-v8 Windows
`assemblywright-master`. It persists
immutable digest-bound approved specifications, independent repository grant
revisions, a 100-nonterminal-item owner-ordered queue, dependency blocking,
compare-and-set revisions, one active lease, exact evidence-bound lifecycle
advancement, cancellation without queue advancement, explicit safe
abandonment, and restart quarantine with atomic redacted audits. Its exact
bounded projection is reachable through the owner-authenticated local route and
through `GET /v1/distributed/feature-conveyor/status` only after an accepted
exporter-bound MacBridge session. The latter accepts no owner token, denies
other roles, and is decoded into authenticated read-only Mac app state. It
  grants no worker, repository, review, Codex, GitHub, publication, control UI,
  unattended, or autonomous authority. Schema v8 separately persists one
  owner-token-designated, compare-and-set owner-control MacBridge. Only its
  exact current exporter-bound application session may POST one bounded,
  already-approved specification bound to current queue, designation, and
  Emergency Pause revisions. The action appends an immutable queued feature
  with atomic redacted audit and does not claim, dispatch, execute, review,
  publish, or activate it. The signed Mac helper exposes this only through a
  one-shot standard-profile `approve-and-enqueue --confirm` stdin command; the
  app remains read-only. Separately, an owner-token loopback-only repository
  preflight binds one canonical path/branch/HEAD scope to the exact current
  active registration grant and Emergency Pause revision. It runs only bounded
  filesystem-only identity observation of a standard local `.git` directory,
  symbolic HEAD, and exact loose ref for a single-component branch. Windows
  holds non-reparse handles for the complete fixed-volume identity chain, then
  reopens and compares the canonical pathname and identities immediately before
  the final atomic grant, pause, and audit recheck and retains the fresh handles
  through it. It executes no Git process, loads
  no repository configuration or attributes, rejects network/reparse/worktree/
  submodule paths, does not prove clean-tree or content state, retains no path
  or content, and returns a path-free point-in-time digest receipt after atomic
  redacted audit. The receipt is not a durable snapshot or claimability.

Emergency pause and deliberate resume are reachable only through the
Windows-local bearer-authenticated loopback actions
`POST /v1/development/emergency-pause/activate` and
`POST /v1/development/emergency-pause/resume`. Both require an exact empty JSON
object, accept no planning or enqueue fields, and are absent from the remote
mTLS router. Pause activation atomically transitions active fixture or MLX
attempts into durable cancellation; resume reopens admission but never revives
an old lease or permits its late result.

The earlier bridge foundation remains a Swift Keychain enrollment coordinator, Security.framework
identity store, Network.framework TLS 1.3 client, and focused
`assemblywright-mac-bridge` operator CLI can complete an exporter-bound authenticated
health session. Its explicit monitor mode reuses the accepted connection for
exact bounded health samples, cancels on any failure, and reconnects with capped
backoff while emitting only allowlisted status. Its bounded reconnect diagnostic
also closes an accepted session and requires a fresh production handshake with
a higher master epoch. Live Secure Enclave enrollment
runs the CLI from an Xcode-provisioned app wrapper with a distinct Keychain
access group; an ad-hoc SwiftPM executable remains compile-only. The current
Swift app may supervise that exact separately signed helper only through the
default-off executable plus independently supplied Apple-team development
opt-in. It validates the helper's Apple code identity, exact CDHash, and distinct Keychain group, clears
the child environment, accepts only strict bounded redacted monitor snapshots,
and exposes read-only bridge state. When the independently supplied agent
executable and data-directory opt-ins are also present, the app sends only
those paths to the helper through bounded stdin. The helper validates and
directly launches the exact agent, creates its owner-only runtime socket and
fresh bearer internally, pins both local peers by audit token, EUID, path, and
CDHash, and forwards bounded authenticated Windows event batches into the
agent's durable cursor. The additional exact fixture opt-in permits only the
registered synthetic lease/result and cancellation/acknowledgement routes
through that same helper/agent boundary. The Keychain identity and mTLS
connection never enter the Rust process. The helper is not bundled and this is
not unattended background operation or a general production distributed-job
runtime. Live fixture evidence requires a separately enrolled
fixture-capability device; an existing `mlx.reasoning` enrollment is not valid
fixture proof. That enrollment occupies a second device-only Keychain namespace
selected only by the helper/CLI's exact `--identity-profile fixture` argument.
The original standard accounts/key/certificate remain the compatibility
default, and the fixture profile rejects any non-exact or mixed capability
before staging or TLS. The `--run-fixture` live harness keeps task creation and
pause/resume in the Windows-local owner control plane, proves a bounded exact
success plus pause-dominated cancellation through sanitized receipts bound to
exact task/step event kinds and strictly increasing stream sequences. The
authenticated loopback-only `/v1/development/events/next` evidence route reuses
the redacted distributed metadata batch, exposes no context or result, and
remains absent from the enrolled-device router. The agent cursor must consume
the exact terminal sequences; cancellation also requires seven seconds without
a late or duplicate event, then drains all pages until a query completed after
the deadline observes `has_more:false`. Every observed same-task event must be
the next exact expected kind; stream/cursor regression or an unbounded page
tail fails closed. The harness then proves same-cursor helper/agent
restart and a fresh authenticated standard-profile connection, and emits only a
payload-free receipt.
Revocation and confirmed fixture-profile removal are explicit owner cleanup.

The standard profile now has one narrowly scoped repair path for the observed
stale exact fixture registration. `enrollment rebind-pair --confirm` retains the
raw grant secret only in the stopped Windows CLI process, snapshots the current
same-device/name/role registration, and permits only a strictly higher exact
singleton MLX target. Certificate issuance creates schema-v6 pending evidence
retained by schema v8;
it neither changes the active registry nor authorizes the new certificate. The
Mac `enrollment rebind prepare|stage --confirm` commands use a separate standard
replacement Secure Enclave key, certificate label, and staged record, validate
the same endpoint and pinned CA plus all certificate/public-key bindings. The
replacement key signs a domain-separated grant/device/revision/serial/
certificate-digest acknowledgement, and Windows verifies it against the exact
P-256 CSR public key held in pending evidence. A separate Windows
`rebind-activate --acknowledgement-stdin --confirm` transaction rechecks the
registration snapshot, short expiry, Emergency Pause, disconnected/no-attempt state, and exact
acknowledgement before updating the registration, inserting the replacement
certificate, revoking old certificates, and terminalizing the pending row.
It commits a metadata-only immutable audit row with every grant, issuance,
activation, or abort. The CA signs a separate domain-separated activation
transcript; only a receipt verified with the staged pinned CA allows Mac
`rebind promote --confirm` to select the replacement generation. Exact
lost-output retries reissue the original terminal timestamp, while mismatches
and non-canonical uppercase digests remain rejected. Mac destructive cancel is
limited to prepare-only state; after Windows issuance the owner aborts Windows
first. Once the Mac has staged and acknowledged the certificate, cancel refuses
and preserves all recovery material because activation may already have
committed. Post-promotion cancel cannot delete the selected replacement. This adds no automatic
capability change, fixture-profile mutation, model enablement, enqueue,
planning, repository, Codex, Git, publication, or unattended authority.

The separately invoked `--run-mlx` harness uses the existing exact standard
`mlx.reasoning` enrollment and requires the executable, model directory, and
model identifier as explicit Mac inputs. Windows-local owner commands in
`scripts/windows-mlx-live-control.ps1` enqueue one real success and one long
cancellation request. The combined cancellation action waits for the exact
lease and immediately activates emergency pause on Windows, avoiding an
operator/bridge timing race with a warmed local model; deliberate resume
remains separate. The harness
accepts only strict payload-free receipts, requires the Mac durable cursor to
reach the exact terminal sequences, binds every leased/terminal event to the
expected Mac device and one attempt epoch, observes seven seconds without a
late or duplicate cancelled-task event, and proves same-stream restart. It is live
two-device local-inference evidence only, not model quality, OS sandbox,
repository/Codex/Git authority, unattended reliability, signing/notarization,
or release readiness.
The MLX-only Swift request timeout is 610 seconds and the shared agent IPC
dispatch ceiling is 620 seconds, so both outlive the protocol's ten-minute
lease in the order needed for the agent's earlier deadline and bounded
process-group cleanup to complete. Other relay operations keep tighter
method-specific timeouts.

### RTX And Future Workers

The RTX worker is a restricted process separate from the master even when both
run on Windows. It registers lightweight inference capabilities over loopback.
It has no direct repository, database, memory, credential, Git, or publication
access. Future workers use the same capability and job protocol.

## Trust And Communication Model

The private overlay is not an authorization boundary. Assemblywright uses TLS 1.3
mutual authentication with per-device certificates and a master-owned private
enrollment CA.

1. A local Windows CLI creates a 256-bit, ten-minute, single-use enrollment
   grant after an explicit operator confirmation. The preferred interactive
   pairing process retains and zeroizes that raw secret locally, emits only a
   strict public invitation, and accepts only the public CSR reply on stdin.
2. The Mac generates its private device key locally and submits only its public
   key through the enrollment exchange.
3. The CA and device private keys remain in Windows credential protection or
   macOS Keychain and never enter SQLite, logs, command arguments, or jobs.
4. The master signs a short-lived device certificate. Role and enabled
   capabilities come from the master registry, not client assertions.
5. Certificate serial, device identity, registry revision, connection epoch,
   and protocol version bind the authenticated session. An application
   handshake is bound to the TLS channel exporter.
6. Every application stream uses a server-issued connection epoch and strictly
   monotonic sequence numbers; task IDs, attempt IDs, digests, and durable
   leases provide operation replay protection without trusting client clocks.
7. Revocation is a durable master record checked at connection and lease
   issuance. Authenticated certificate rotation occurs before expiry; device
   compromise requires revocation, and CA compromise requires explicit CA
   rotation plus re-enrollment.
8. Unknown, expired, revoked, duplicate-active, role-changing, replaying, or
   incompatible devices fail closed.

Each dispatched job contains bounded, versioned fields:

- Task, step, attempt, lease, and cancellation identifiers.
- Required capability and selected model.
- Sensitivity, context-handling policy, deadline, and payload digest.
- Task-specific context rather than database access or repository paths.

Results must match the same identifiers and digest. Late, duplicate, replayed,
oversized, mismatched, or post-cancellation results are rejected before they
enter task state or memory.

The first-release hard ceilings are:

| Resource | Ceiling |
| --- | --- |
| Enrolled devices | 16 |
| Concurrent authenticated connections per device | 2 |
| Concurrent application streams per device | 8 |
| Wire frame | 1 MiB |
| Context in one inference job | 256 KiB |
| Accepted inference result | 1 MiB |
| Event payload | 64 KiB |
| Queued nonterminal tasks | 256 |
| Steps per task | 64 |
| Attempts per step | 8 |
| Renewable lease | 10 minutes, heartbeat at most every 30 seconds |
| One step wall time | 2 hours |
| One task wall time | 24 hours before attention-required |
| Private raw execution evidence | 64 MiB per task, retained 30 days |
| Durable metadata audit entries | 10,000 per task; further work fails closed rather than dropping required evidence |
| Worktree and scratch allocation | 20 GiB per task; admission fails when the reservation cannot be honored |

Limits are configurable only downward in the first release. Private artifact
retention may remove bounded raw data after its window, but durable metadata
retains outcome, hashes, policy, and deletion evidence. Required audit evidence
is never silently truncated.

Connection loss revokes active leases. Stateless inference may be reissued
only after the prior attempt is durably marked abandoned. The co-located RTX
worker uses the same result-acceptance rules and a restricted worker identity;
loopback placement does not grant master authority.

## Safety Contract Migration

The current product contract permits only response-only Codex and blocks
workspace results from cloud continuation. Full-agent repository access and
prompt-free autonomous publication therefore remain disabled until
`DESIGN.md`, `docs/safety-rules.md`, policy schemas, tests, diagnostics, and
release evidence are revised together. This document proposes that explicit
contract change; it does not reinterpret the current rules as already allowing
the new authority.

Autonomy is represented by an owner-created repository automation grant, not
by the system silently treating registration as approval. The grant binds the
repository identity, allowed branches, path policy, validation contracts,
publication actions, maximum sensitivity, expiry, and revocation revision.
Before each push, pull-request mutation, or merge, the master creates and
durably claims one exact action record bound to that grant, task, commit, ref,
and expected remote state. A missing, expired, broadened, or revoked grant
requires a new explicit owner decision. This is a deliberate replacement for
the current prompt-per-effect policy and cannot ship until the safety contract
and regression tests accept the bounded standing authority model.

## Task Lifecycle And Routing

Every request becomes a durable master task before any work is dispatched. The
master binds the task to a repository policy, sensitivity, task class, required
authority, input snapshot, and routing-rule version.

The initial deterministic routing table is:

| Capability | Default work |
| --- | --- |
| RTX worker | Autocomplete, small isolated functions, formatting suggestions, and fast low-context generation. |
| M1 worker | Planning, repository Q&A, long-context reasoning, and RAG synthesis. |
| Windows utilities | Indexing, retrieval, builds, tests, static analysis, repository inspection, and CPU/RAM-heavy operations. |
| Codex response-only | Architecture analysis, difficult diagnosis, and independent final review. |
| Codex full agent | Complex implementation, broad refactors, migrations, and difficult repair loops. |

Ambiguous classification defaults to M1 planning. If the master still cannot
derive a safe class, Assemblywright asks for clarification. A manual override may
select another policy-eligible capability but cannot bypass sensitivity,
authority, availability, or repository policy.

Persisted task states are requested, planning, ready, leased, running, paused,
quarantined, validating, reviewing, publishing, attention-required, cancelled,
failed, and succeeded. Cancelled, failed, and succeeded are terminal.

Paused always carries a normalized reason such as waiting-for-master,
waiting-for-worker, waiting-for-Codex-authentication,
waiting-for-Codex-capacity, or awaiting-approval. Quarantined means an
interrupted Codex/worktree boundary has preserved inspectable state and cannot
resume automatically. Attention-required identifies the exact owner decision
needed. An effect-possible flag distinguishes recovery that may involve an
external side effect from safe resumable work.

Every transition records its rule, actor, reason, evidence, effect-possible
status, and accepted output digest.

Repository mutations are isolated by task worktree. Read-only planning,
inference, indexing, and review may run concurrently against explicit
snapshots. Only one publication or merge transition may hold a repository's
publication lease.

## Codex And Repository Execution

### Response-Only Codex

The response-only adapter keeps the existing constrained boundary:

- Temporary working directory.
- Bounded redacted context.
- Tool and repository surfaces mechanically disabled.
- Minimal environment and structured bounded final response.

It is used for architecture, difficult analysis, and independent review.

### Full-Agent Codex

The full-agent adapter runs non-interactively inside a task-specific worktree.
It receives workspace-write authority only for that worktree and bounded
scratch space. It runs under a restricted Windows identity with no access to
the master database, memory store, Git credentials, SSH keys, unrelated
repositories, browser state, or general user environment.

Full-agent Codex is a Developer capability, not an expansion of the existing
ChatGPT model-provider route. Its execution identity is placed in a Windows Job
Object so descendants retain the restricted token, resource limits, and
bounded termination contract. Repository validation commands are untrusted
even when declared. The first release permits only exact executable and
argument-template contracts run by the restricted executor with network
disabled, no inherited handles or credentials, and filesystem ACLs limited to
the worktree, toolchain, dependency cache, and scratch directory. Repositories
that require live network access during validation are ineligible for
autonomous publication in this release.

The dedicated Codex runner identity owns exactly one reusable external secret:
the owner's Codex CLI authentication material in that identity's platform
credential store. The master never copies it into SQLite, job payloads,
environment variables, worktrees, or logs. Codex loads authentication before
tool execution; model-generated commands run under the CLI sandbox plus a
further restricted child token that cannot read the runner profile or
credential store. Release evidence must prove a hostile command cannot read or
export the authentication material. If that separation cannot be proven for
the supported CLI and Windows build, full-agent Codex remains disabled.

Only the Codex executable may use outbound TLS, and only to an exact
release-manifest allowlist of OpenAI authentication and Codex service endpoints
verified for the supported CLI version. Validation commands and descendants
have no network route. Endpoint drift fails visibly and requires a reviewed
manifest update; it never broadens to unrestricted egress.

Codex cannot push or merge directly. The master retains Git credentials and performs publication
only after all of these conditions pass:

1. Required validation gates succeed.
2. Changed paths remain within repository policy.
3. Secret and sensitive-data scans succeed.
4. A fresh review session that receives no implementation transcript accepts
   the exact bound diff and commit, and repository-required non-model checks
   pass. This is context independence, not a claim of provider diversity.
5. Branch protection and repository merge policy permit publication.

Codex events are bounded, parsed, and redacted before becoming task evidence.
Prompts, source content, credentials, and raw command output do not enter public
audit surfaces.

Full-agent sessions default to one at a time. If Codex is unavailable, logged
out, usage-limited, interrupted, or unable to honor its required CLI contract,
the assigned task pauses without local substitution.

Codex's own sandbox is not treated as host-isolation proof. The external
boundary requires a restricted Windows identity, filesystem ACLs, minimized
environment, child-process restrictions, credential separation, and outbound
network policy proven on the target host.

## Memory, RAG, And Durable Data

Windows owns canonical SQLite records and rebuildable retrieval indexes.
Personal and Developer modes share one memory corpus. Mode remains provenance
and routing context rather than a hard retrieval silo. Sensitivity, source,
review state, lifecycle status, and task policy determine eligibility.

The existing memory-context rules remain unchanged in the first release:

- Retrieval is explicit per-command opt-in and never proactive.
- The canonical index must be current and records must be active and reviewed.
- Only Public, Workspace, or Personal records are eligible.
- Private, CredentialAdjacent, Restricted, deleted, unreviewed, stale, corrupt,
  missing, oversized, or over-budget input fails closed.
- At most four records and 4 KiB of memory value enter a task.
- Retrieved memory remains local-model-only, framed as untrusted data, and
  absent from public audit, diagnostics, route evidence, and errors.

Consequently, exact approval for Personal or Private Codex context applies to
user-supplied task content, not automatic memory retrieval. Canonical memory
and memory-derived context do not continue into either Codex lane in this
release. Changing that rule requires a later safety-contract revision.

Sensitivity is monotonic across derivation. A plan, generated file, diff,
review, or artifact inherits the highest sensitivity of its inputs unless an
exact owner-approved declassification records the source set, destination,
reviewed output digest, and rationale. Developer tasks that consume Personal or
Private memory cannot enter publication states before that declassification.
Restricted-derived artifacts cannot reach Codex or external publication.

For repository RAG, Windows performs path filtering, secret detection,
chunking, snapshot binding, and result selection. It may delegate embedding or
reranking computation to an eligible local worker, but canonical documents,
metadata, and indexes remain on Windows. Codex never builds the private memory
index.

Workers may persist model weights, runtimes, and non-sensitive capability
configuration. They may not persist prompts, retrieved passages, source
snippets, task results, repository snapshots, or memory records. Context is
discarded after result acceptance or cancellation.

Each task binds retrieval to explicit record and repository snapshot versions.
Later changes cannot silently rewrite evidence for an active task. Public audit
exposes counts, decisions, selected capability, and snapshot identifiers rather
than retrieved values or source content.

Windows backup and recovery are operator-controlled and encrypted. The master
creates encrypted incremental backups at least every 15 minutes and a daily
full backup, retaining 30 daily restore points outside the live database
volume. The live encryption key remains protected by Windows platform
credentials; a separately generated recovery key is held outside the machine
in owner-controlled secure custody. Neither key enters SQLite, logs, or
diagnostics.

Cutover requires a successful full restore drill. Post-cutover evidence
requires quarterly restore validation covering schema, task/audit counts,
repository registry, automation grants, and index rebuildability. Target RPO is
15 minutes and target RTO is four hours. Stale backup or failed restore status
degrades the master and blocks new autonomous publication. Corrupt canonical
state, stale indexes, or failed migrations block affected work until validated
recovery or an explicit index rebuild succeeds.

## Failure Handling And Recovery

Master startup reconciles persisted tasks, leases, worktrees, Git references,
and publication intents before accepting new work.

Cancellation has an active control path in addition to lease/result rejection.
The master sends a cancellation frame bound to the task, attempt, lease, and
connection epoch. Remote inference must acknowledge and stop before a bounded
deadline; otherwise the connection is closed, its lease is revoked, and every
later result is rejected. On Windows, Codex and validation run inside Job
Objects that receive bounded graceful termination followed by forced tree
termination and reaping. Cancellation cannot reverse an external effect that
already occurred; those steps retain effect-possible evidence and require
reconciliation.

- Stateless inference may be retried only after the previous lease is durably
  marked abandoned. Late or duplicate results are rejected.
- An unavailable M1 worker pauses M1-assigned work. The master does not silently
  move it to RTX; a deliberate policy-checked override is required.
- Codex authentication loss, usage limits, process failure, or incomplete
  output pauses the task and quarantines its worktree. Existing edits remain
  inspectable, but no automatic rerun or replacement occurs.
- Build and test repair loops obey repository-configured attempt and time caps.
  Exhaustion moves the task to attention-required.
- Git publication uses durable intent records bound to exact commits and
  expected remote refs. Ambiguous push, PR, or merge outcomes are reconciled
  against remote state before any retry.
- If the Mac disconnects, already-authorized Windows, RTX, and Codex work may
  continue. M1 and Apple-specific steps pause. The Mac resumes durable event
  streaming from a server-issued cursor after reconnect.

Emergency pause is authoritative on Windows. The implemented Developer Mode
control is the owner-authenticated local loopback action above; a connected-Mac
control and a dedicated Windows operator CLI remain presentation work. Pause
blocks new leases and publication, durably cancels safe active fixture work,
and marks potentially effectful interruptions for review.

## User Experience And Operations

The Mac app exposes explicit Personal and Developer modes. Switching modes
changes workflows and presentation but not memory authority or safety policy.

Developer Mode shows:

- Registered repository and policy status.
- Proposed deterministic task route.
- Worktree, validation, review, pull-request, and merge progress.
- Worker and Codex health.
- Policy-eligible route override.
- Pause, cancel, emergency pause, and attention-required recovery.
- Redacted evidence for routing and publication decisions.

Pause, Cancel, and Emergency Pause have distinct user contracts. Pause stops
new steps after the current safe checkpoint. Cancel actively stops the current
attempt and ends the task as cancelled when no effect is possible; otherwise
it enters attention-required recovery. Emergency Pause applies globally,
blocks new leases and publication, cancels safe work, and preserves ambiguous
effects for review.

A route override preview shows why the current capability was selected, the
candidate's relative quality and latency class, context ceiling, and authority.
It applies only to an unstarted step. A running step must first pause or cancel
and then start a new attempt; Assemblywright never moves live context between workers.

Cold model loading is a visible `Loading Model` state with elapsed time and an
estimate when the runtime can supply one. It is excluded only from the warm
latency metric, not from the user's observed task duration.

The app renders authoritative Windows connection generation, protocol
compatibility, enrolled devices, capabilities, and task events. It does not
infer health from saved configuration. Reconnection resumes from a durable
event cursor.

The Mac persists only enrollment material, endpoint configuration, the last
accepted event cursor, and bounded metadata-only diagnostics. Prompt text,
source snippets, retrieved context, model results, and task transcripts remain
in memory while displayed and are not written by the bridge. Mac diagnostics
are capped at 16 MiB and seven days. Explicit canonical memory writes remain
Windows commits made only after policy approval.

After reconnect, the Mac re-fetches the selected task summary, accepted final
result, approvals, and current recovery state from Windows. Ephemeral partial
model output is not reconstructed. Content already expired under an explicit
Windows retention policy appears as expired metadata rather than a blank or
apparently current transcript.

When Windows is unavailable, the app enters an explicit Master Offline state.
Command entry, voice submission, inference, memory mutation, and administrative
actions are disabled.

Repository registration, repository automation, and effect execution are
separate owner flows. Registration makes repository metadata and declared
workflows eligible for inspection; it grants no mutation or publication
authority. The owner then reviews a policy preview and separately confirms an
automation grant with repository, branches, paths, actions, sensitivity,
expiry, and revocation terms. The UI states plainly whether autonomous push,
PR mutation, and merge are disabled or enabled.

Other exact approvals include Personal or Private user-supplied context sent
through Codex, policy broadening, device enrollment, digest-bound
declassification, and ambiguous-effect recovery. Declassification shows the
source sensitivity, destination, reviewed output digest, and publication that
will become eligible. Routine publication needs no new prompt only while the
bounded automation grant and every exact effect claim remain valid.

Windows remains headless for this release. Its local CLI owns installation,
enrollment grants, health, migrations, backups, emergency pause, diagnostics,
and recovery. Manual upgrades require compatible versioned builds.

The owner journey is explicit:

1. Windows setup validates storage, backup-key custody, service identity, and
   containment, then prints a fixed setup receipt and a short-lived enrollment
   grant.
2. Mac Connection Setup accepts the master endpoint and grant, displays the
   master fingerprint and matching verification code, and becomes connected
   only after Windows confirms the enrollment.
3. Health and maintenance failures appear in the Mac app with the exact Windows
   CLI action required; administrative authority never silently moves to Mac.
4. Upgrade enters maintenance mode, verifies a fresh backup and matching
   versions, drains safe work, migrates, runs health checks, and emits a fixed
   success or rollback receipt before work resumes.
5. Restore lists backup age and restore points, restores into staging, validates
   schema and evidence counts, then requires explicit activation and emits a
   restore report visible from Mac health.
6. Certificate rotation normally occurs while authenticated. Expiry or
   compromise produces an actionable re-enrollment or revocation flow rather
   than a generic offline error.

Routine maintenance includes certificate rotation before the final 20 percent
of certificate lifetime, daily backup-freshness checks, quarterly restore
drills, bounded diagnostic cleanup, and matching-build upgrade verification.
Failure is visible in both the Windows CLI and Mac health surface.

Scheduling uses separate interactive and background queues. At least one of the
four job slots is reserved for interactive planning and routing, and at most
one CPU-heavy indexing or build utility runs in the background by default.

## Testing And Proof Strategy

### Unit And Contract Proof

- Deterministic routing, capability eligibility, sensitivity rules,
  repository policy, leases, cancellation, replay rejection, state transitions,
  publication guards, and redacted evidence.
- Versioned Rust/Swift golden fixtures for enrollment, registration, jobs,
  events, reconnect cursors, errors, bounds, unknown fields, and incompatible
  versions.

### Cross-Process And Security Proof

- Real master with fake RTX, M1, Codex, Git, and Apple adapters.
- Worker loss, master restart, duplicate/late results, expired leases,
  cancellation races, corrupt payloads, Codex interruption, ambiguous Git
  outcomes, and emergency pause.
- Unknown, revoked, duplicated, and role-changing device rejection.
- Worker denial from repositories, credentials, memory, and master storage.
- Path exclusion, secret scanning, payload bounds, traversal rejection, and
  evidence redaction.
- Actual Windows containment checks for Codex identity, filesystem,
  environment, child-process, credential, and network boundaries.
- Hostile Codex tool commands attempting to read the runner credential store,
  authentication files, process handles, or tokens; failure keeps the
  full-agent lane disabled.
- Certificate enrollment, expiry, authenticated rotation, revocation,
  connection-epoch replay, sequence rollback, TLS channel binding, device
  compromise recovery, and CA-rotation drills.
- Sensitivity propagation and declassification checks proving Personal,
  Private, and Restricted inputs cannot leak through generated artifacts,
  commits, review, diagnostics, or publication.
- Active cancellation tests proving Mac inference, RTX inference, Codex and
  validation descendants stop or become effect-possible before late output can
  be accepted.

### Acceptance Proof

1. A deterministic fixture repository completes planning, distributed
   inference, isolated editing, validation, review, publication, and merge with
   stubbed external boundaries.
2. An owner-supervised dogfood task performs one real Assemblywright change through
   live Windows, Mac, Codex-account, and GitHub paths.
3. Performance evidence covers acknowledgement latency, routing latency,
   reconnect recovery, and four-job concurrency.
4. Existing macOS release gates remain required alongside new Windows,
   distributed-protocol, and cross-machine gates.
5. Backup freshness, independent-key restore, RPO/RTO drills, queue admission,
   hard resource ceilings, and degraded publication blocking.

Repository tests do not prove Windows host hardening, private-overlay setup,
live Codex availability, signing, notarization, or real-device network
reliability. Those require separately recorded evidence.

## Major Risks And Accepted Boundaries

- Moving authority from the app-supervised Mac core to a Windows master is a
  large migration. The local bridge preserves the Mac security boundary but
  does not remove storage, scheduler, runtime, and release migration risk.
- Codex CLI behavior, authentication, models, and plan limits may change. The
  adapter must version-check, capability-probe, and fail closed.
- Windows containment is a release blocker for full-agent autonomy; Codex's
  sandbox alone is not sufficient proof.
- Unified memory increases cross-context leakage risk; sensitivity and
  provenance remain authoritative despite the shared corpus.
- Automatic merge amplifies policy mistakes. Assemblywright never bypasses branch
  protection, protected paths, required checks, exact-commit review, or secret
  scanning.
- Windows is an accepted single point of failure. There is no failover or
  offline mode in this release.
- Stateless workers may retain models and runtime configuration, but task and
  repository cleanup must be verified rather than assumed.
- Future extensibility ends at the capability/job protocol. Fleet scheduling,
  consensus, automatic updates, and distributed storage remain deferred.
- The current app-supervised Mac architecture remains the release default while
  the remote-master path is built behind an explicit development gate. State
  migration requires a preflight backup, schema/version proof, one-way import
  report, and rollback to the untouched Mac store before cutover. Final
  Windows-master mode intentionally has no Mac fallback.

## Decision Log

| Decision | Alternatives considered | Rationale | Objections and resolution |
| --- | --- | --- | --- |
| Two explicit Personal and Developer modes | One blended mode; Developer-only replacement | Keeps user intent and workflows clear while preserving the existing assistant. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Preserve but feature-freeze Personal Mode | Expand both modes; remove Personal surfaces | Concentrates the pivot without creating a regression mandate. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Windows as sole master | Mac master; multi-master | Uses the machine with more CPU/RAM for repositories, utilities, and state while avoiding consensus complexity. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Existing Mac app remains primary UI | Windows UI; dual UIs | Preserves native voice and Apple UX and avoids a second GUI in the first release. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Mac bridge plus capability worker | Direct Swift-to-Windows; broker/workflow engine | Reuses the local Mac trust boundary and keeps cross-platform orchestration in Rust without premature infrastructure. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| RTX provides lightweight inference | No Windows inference; all inference on Windows | Uses the available GPU for bounded fast work while reserving stronger M1 models for deeper reasoning. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| M1 is a stateless stronger-model worker | Mac state replica; Mac master fallback | Uses the stronger local model without splitting authority or recovery. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Unified memory corpus | Separate mode stores; explicit-share namespaces | Enables cross-domain continuity while sensitivity and provenance remain authoritative. | Skeptic objected that derived artifacts could publish sensitive memory. Accepted: monotonic sensitivity propagation and exact digest-bound declassification now gate publication. |
| Workers receive ephemeral bounded context | Read-only synchronized checkouts; full working copies | Minimizes leakage, stale state, and recovery complexity. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Backend-neutral capability protocol | Ollama-only; LM Studio-only | Supports present and future runtimes without coupling orchestration to one server. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Codex account rather than OpenAI API | Platform API; local-only | Uses the owner's existing Codex workflow and avoids Platform API keys and billing. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Separate response-only and full-agent Codex lanes | Response-only only; full-agent only | Matches authority to task risk and preserves constrained architecture/review calls. | Skeptic identified a conflict with the current response-only/cloud-workspace safety contract. Accepted: full-agent Codex is a new Developer capability and remains disabled until the design and safety contracts are explicitly revised and proven. |
| Full Codex executes on Windows | Mac Codex; either host | Keeps repository and worktree authority on the master. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Deterministic routing with eligible override | Learned scoring; manual-only routing | Provides predictable, explainable first-release behavior. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Codex failures pause without fallback | Always local fallback; capability-aware fallback | Prevents silent quality and authority changes. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Up to four jobs and one full Codex session | Serialized execution; throughput-first fleet | Balances responsiveness and resource use for one owner and two machines. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Policy-controlled publication and merge | Approval before publication; approval before merge | Enables genuine autonomy while retaining exact repository gates and branch protection. | Skeptic objected that broad repository policy is not exact execution approval. Accepted: an explicit bounded automation grant plus one exact durable claim for every remote mutation replaces implicit standing authority. |
| Fail closed when Windows is unavailable | Limited local mode; Mac failover | Preserves one source of truth and avoids reconciliation in the first release. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Private overlay plus Assemblywright authentication | LAN-only; direct internet exposure | Supports remote personal use without treating network location as identity. | Skeptic found enrollment and replay handling conceptual. Accepted: TLS 1.3 mTLS, short-lived certificates, durable revocation, channel binding, connection epochs, monotonic sequences, rotation, and compromise recovery are now specified. |
| Manual matching-version deployment | Master-pushed updates; independent tracks | Minimizes updater and compatibility complexity in the first release. | No scoped objection; accepted by the Arbiter as owner-confirmed. |
| Registered repositories with declared workflows | Assemblywright-only; language-limited support | Generalizes safely through explicit per-repository policy rather than language assumptions. | Skeptic objected that repository commands are untrusted. Accepted: exact command contracts run network-disabled under a restricted token, ACLs, Job Object, and no credentials; network-dependent repositories are initially ineligible for autonomous publication. |
| Sandbox fixture plus supervised dogfood | Fixture-only; dogfood-only | Combines deterministic proof with one realistic end-to-end validation. | No scoped objection; accepted by the Arbiter as owner-confirmed. |

### Structured Review Resolutions

| Reviewer objection | Disposition | Resolution |
| --- | --- | --- |
| Full-agent Codex conflicts with current response-only and local-workspace safety rules. | Accepted | Treat it as a new Developer capability and a gated safety-contract migration; current behavior remains the release default until docs, policy, tests, and evidence change together. |
| Autonomous publication lacks exact effect approval. | Accepted | Require an explicit bounded automation grant and one durable exact claim for each push, PR mutation, or merge. |
| Repository commands can escape path-based controls. | Accepted | Run only exact declared command contracts under a restricted token, Job Object, ACL-limited filesystem, no credentials, and disabled network; network-dependent validation is initially ineligible. |
| Unified-memory sensitivity can leak through derived artifacts. | Accepted | Propagate the highest input sensitivity to every derivative and require exact digest-bound declassification before publication. |
| Lease expiry does not actively cancel running work. | Accepted | Add cancellation frames, acknowledgements, connection termination, Job Object tree termination, output suppression, and effect-possible reconciliation. |
| Device enrollment and replay controls were conceptual. | Accepted | Specify TLS 1.3 mTLS, short-lived device certificates, durable revocation, channel binding, connection epochs, monotonic sequences, rotation, and compromise recovery. |
| Same-provider review was called independent. | Accepted clarification | Define independence as a fresh context without implementation transcript, not provider diversity, and retain non-model repository checks. |
| Stateless cleanup overstated absence of residue. | Accepted clarification | Limit the claim to no deliberate application persistence and require encrypted hosts plus disabled prompt logging/telemetry. |
| First-release scope is broad. | Acknowledged, not changed | The owner explicitly selected autonomous coding across registered repositories; feature gates and release proof prevent partial mechanics from being presented as complete. |
| Windows authority conflicts with current Mac-local architecture. | Accepted | Keep the current path as default during gated migration, require backup/import/rollback evidence, and cut over only after the remote path clears its release gates. |
| Memory migration did not enumerate preserved privacy rules. | Accepted | Preserve explicit opt-in, non-proactive, current-index, reviewed/active eligibility, four-record/4-KiB caps, local-model-only retrieval, and existing redaction; Codex approval does not authorize memory retrieval. |
| Bounded resources lacked enforceable ceilings. | Accepted | Add hard limits for devices, connections, streams, frames, context, results, queues, steps, attempts, leases, wall time, evidence, audit, and disk admission. |
| Full-agent Codex credential access was contradictory. | Accepted | Isolate one Codex credential under a dedicated runner, deny child access with sandbox plus restricted token, constrain Codex egress to exact release-manifest endpoints, and block the lane unless hostile-command proof passes. |
| Windows recovery lacked RPO, RTO, key custody, and restore proof. | Accepted | Require 15-minute incrementals, daily full backups, 30 restore points, separate owner-held recovery key, RPO 15 minutes, RTO four hours, and quarterly restore validation. |
| Lifecycle states did not explain safe, resumable, quarantined, effect-possible, or terminal outcomes. | Accepted | Add paused, quarantined, attention-required, cancelled, failed, and succeeded states with normalized reasons and an effect-possible flag. |
| Repository registration, automation authority, and declassification approval were conflated or absent. | Accepted | Separate registration, policy preview, explicit bounded automation grant, exact effect claims, and digest-bound declassification into visible owner flows. |
| Headless Windows operations lacked an end-to-end owner journey. | Accepted | Define setup, paired enrollment, actionable Mac health guidance, maintenance-mode upgrade, staged restore, fixed receipts, and certificate recovery flows. |
| Personal Mode was described as regression-free despite losing offline availability. | Accepted | State explicitly that features are preserved while availability intentionally becomes dependent on the Windows master. |
| Pause, cancel, override, cold-load, and reconnect behavior were unclear. | Accepted clarification | Define distinct stop semantics, unstarted-step-only overrides, visible cold loading, and authoritative Windows re-fetch after reconnect. |

## Review Status

- Understanding Lock: confirmed by owner.
- Initial design: accepted section by section by owner.
- Skeptic / Challenger: completed; six blocking objections accepted and
  resolved in the design, plus four non-blocking risks clarified.
- Constraint Guardian: approved after four blocking constraints and four
  non-blocking clarifications were accepted and resolved.
- User Advocate: approved after four blocking UX gaps and four non-blocking
  clarifications were accepted and resolved.
- Integrator / Arbiter: approved every recorded resolution; no objection was
  rejected and no design issue remains unresolved.
- Final disposition: APPROVED for implementation planning, not implementation.

## Protocol v5 bounded general coding worker

The local-coding capability accepts only immutable digest-bound implementation
packets with sorted normalized relative-path allowlists and exact deterministic
`file.write.v1` or `file.delete.v1` argument schemas. Rust and production Swift
share a 16 KiB complete-job, 12 KiB context, and 4 KiB aggregate replacement
limit. The agent holds owner-private no-follow descriptors for every parent;
create is exclusive atomic install, replacement is atomic swap plus displaced-
inode verification, and delete is identity-checked `unlinkat`. No command,
general shell, credential, network, Git, canonical checkout, integration, test
gate, review, publication, or lifecycle-advancement authority is present. The
canonical multi-file artifact binds the exact packet. A successful attempt is
sealed for at most one hour until exact cancellation/resolution or expiry.
Schema v13 records immutable retention metadata after a verified backup-first
v12 migration. Separately, the agent keeps one bounded owner-private recovery
record outside the attested workspace; it binds the exact job, attempt, sealed
name, post-edit tree digest, and expiry and never enters SQLite, audit, logs, or
the remote protocol. Restart re-hashes exactly one pair, restores cancellation,
blocks new admission while unresolved, and rejects tamper or ambiguity.
Delete first atomically captures the leaf in the held parent and removes only the
verified displaced inode; mismatch atomically restores the replacement. Windows
independently applies the protocol-owned canonical artifact-to-packet comparison
and matches stored artifact/retention/expiry metadata to the terminal result,
without trusting Swift as the authority source.
