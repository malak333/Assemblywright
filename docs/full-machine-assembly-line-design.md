# Full-Machine Assembly Line Design

Status: approved target; reviewed planning/creation, inert execution-control, and authenticated zero-effect Windows IPC source with effects fail-closed; production routing, execution, and live evidence pending; current master is protocol-v5/schema-v22

This document is the approved replacement target and preserves its required safety
exception. Strict protocol contracts, schema-v20 Windows planning persistence/routes,
a schema-v21 durable effect-free execution-control ledger and schema-v22 fail-closed activation controller,
a catalog-bound planning-effect coordinator, a fixed tool-free Codex adapter, exact
GitHub create/reconcile adapter, MacCore transport, and the owner review/approval UI
are implemented in source. Windows runtime loading deliberately remains unavailable
until capability-separated planning and GitHub identities/ACLs are provisioned and
proved. These changes do not replace the bounded executor or grant execution
authority. The owner approved one
exception to literal full-machine access:
Assemblywright's control plane, audit, signing keys, and executor-enforcement
components remain inaccessible to feature execution.

Full-machine target phase: planning/creation containment has bounded native Windows LocalSystem/AppContainer service proof; execution admission is implemented and effects remain unavailable.

Containment source status: `assemblywright-executor` and `assemblywright-broker` are
distinct binaries and libraries. The private schema-v2 action envelope is Ed25519
signed and binds executor/broker executable digests plus a digest of all named
protected-control-plane categories and the exact authenticated authority revision.
Legacy schema-v1 grants reject, and pause/resume or revocation revision changes
invalidate grants signed against the earlier authority state. Executor restart requires
the exact authenticated snapshot revision and digest restored from the master ledger;
an authentic historical unpaused snapshot cannot roll back a durable pause or
revocation. Broker admission policy is testable, and one Windows `CreateDirectory`
adapter is implemented behind a dedicated one-shot native proof seam. It retains the
complete canonical ancestry without delete sharing, performs one handle-relative native
leaf creation, and distinguishes path-free applied evidence from effect-possible
reconciliation evidence. A negative native status or missing handle after the call is
effect-possible, never terminal pre-effect proof. The long-running broker runtime remains effect-disabled until
active-effect termination and durable reconciliation are implemented; file
replacement/removal and restricted service adapters remain closed. The executor
holds and revalidates macOS executable/cwd descriptors plus signed parent/object
identities, but refuses spawn because process-group membership is escapable by hostile
code. Windows source holds no-reparse executable/cwd chains, verifies the suspended
image, assigns a kill-on-close Job, and rechecks authority before resume. It remains
uninstalled and unavailable through product routes pending native Windows proof. Repository
tests are not OS code-signing, service installation, dedicated identity/ACL, Windows
native execution, resource-reservation, or live two-host proof.

The deployed Windows planning lane remains effect-free. Source now contains bounded
Public-only cloud disclosure, provider invocation, frozen owner approval, durable
pre-effect intent, and exact GitHub reconciliation. Windows rejects loading those
effect adapters until its capability separation is installed, so deployed project
approval still cannot call GitHub. Feature enqueue requires an exact `created`
repository with creation evidence and never dispatches work. Auto-run defaults on and
supports only an exact replay-safe CAS setting change. Authenticated
Start/Stop/Emergency routes fail closed through an unavailable production dispatcher;
production executor/broker services remain unavailable. A local Windows IPC foundation
independently Master-signs bounded hop-specific Broker and Executor frames, requires
the Broker to forward the byte-exact Executor envelope, verifies exact peer service
SIDs on local-only named pipes, and accepts separately service-signed path-free
zero-effect acknowledgements only through pinned public keys. Append-only hash-chained
intent/ack state provides contiguous sequence/nonce replay protection, byte-exact
pending recovery, original-ack replay, and durable quarantine on ambiguity. It exposes
health and dispatch validation only; no privileged effect adapter, activation receipt,
product Master route, or enabled production service installation is wired.

Planning process creation uses a fresh create-only private window station and desktop
per call. Their protected DACL contains only LocalSystem, the provisioning owner, and
the exact provider or GitHub AppContainer profile. The master serializes the brief
process-global station switch, restores its original station before creating the
suspended child, retains both GUI-object handles through complete Job termination, and
never changes `Winsta0`.
The closed Windows Codex environment separately maps `LOCALAPPDATA` to the validated
provider `local-app-data` root and `TEMP`/`TMP` to the validated provider temp root;
ambient owner paths never cross into the Codex child.
The Windows package root is derived from the Common Application Data known folder as
`Assemblywright/planning-runtime/<runtime_instance>`; `DataDir` retains only the
master-private schema-v4 locator/config. Provisioning merges exact non-inheriting
traverse ACEs for both fixed profile SIDs onto held handles for Common Application Data,
the vendor directory, and the planning namespace while keeping unrelated parent ACLs.
The known-folder ACE is necessary because the LocalSystem-derived restricted
AppContainer is not an ordinary Users token. Load and every call bind and revalidate the canonical ancestry,
directory identities, and exact profile rights; schema/path/ACL drift is unavailable.
The master first matches each fixed profile to its deterministic expected SID and then
registers that profile idempotently for its current Windows identity. This is required
because an owner-provisioned AppContainer profile does not establish LocalSystem's
identity-scoped profile registration. `STARTF_USESTDHANDLES` launches inherit valid
stdin, stdout, and stderr handles; stderr is drained concurrently and discarded within
the fixed bound without entering diagnostics or audit.

An explicitly owner-confirmed native diagnostic may exercise the real staged Codex
only while the exact installed master service is fully stopped. The command validates
that its executable and data directory match the SCM registration, revalidates the
complete schema-v4 planning runtime, then uses the same restricted token, fixed
provider profile/SID, private desktop, exact ACL roots, closed environment, inherited
handles, deadline, and kill-on-close Job. Exact `login status` gates one fixed Public
structured request with all tools disabled. The fixed receipt contains only outcome
categories, numeric exit/diagnostic codes, hashes, and binding identities; no process
text, prompt, path, token, credential, or specification content may cross the receipt
boundary. The diagnostic mutates no master planning state and grants no approval or
effect authority.

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
  approval prompts while that execution epoch remains valid.
- Only one feature is active. Auto-run defaults on and advances only after complete
  implementation, validation, independent review, publication, and final-main
  verification. Stop and Emergency Pause preserve resumable durable state.

## Explicit Authority

Full-machine authority includes file reads and writes, process execution, network,
credentials, system configuration, destructive operations, and external effects on
both registered machines, except for the protected Assemblywright control plane. It
belongs only to the signed local executor and its privileged action brokers. It never
belongs to the planning orchestrator.

The owner grants this authority by starting a nonempty assembly line. The grant is
bound to one Windows-issued assembly-line session. Each active feature receives one
narrower child execution epoch bound to the designated Windows and Mac executors,
queue revision, and feature lifecycle. Auto-run may derive the next child epoch only
from the same active owner-started session after exact prior-feature success. An
executor must stop when either binding becomes invalid.

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
Pre-effect URL, ownership, authentication, or provider failure shows one bounded
reason with **Edit** and **Retry**. An effect-possible GitHub response shows
**Reconcile creation** and never offers a blind retry.

### New Feature

1. Select a repository by Git URL.
2. Select an orchestrator profile and enter the feature idea.
3. Run the same structured brainstorming workflow.
4. Review the generated feature specification.
5. Approve and Add to Assembly Line.

A rejected or unavailable brainstorming attempt keeps the editable idea and displays
one bounded reason with **Edit** and **Retry**. It creates no feature.

The normal UI does not expose UUIDs, grant revisions, raw evidence digests, allowed
paths, or validation-obligation fields. Full-machine scope is explicit. Testing,
documentation, knowledge-base, safety, publication, and final-verification
obligations are derived from the approved specification. Internal identities and
evidence remain available under diagnostics.

### Assembly Line

- Start is disabled while the queue is empty.
- Auto-run defaults on and can be disabled before or during execution.
- With auto-run on, a fully successful feature advances to the next queue item.
- With auto-run off, success ends the active execution epoch and enters
  `waiting_for_owner_start`; the UI shows **Start next feature**.
- Stop reaches the next declared durable checkpoint, terminates remaining executor
  processes, and preserves the feature for resume.
- Emergency Pause immediately terminates active executor processes and preserves the
  last durable checkpoint for later resume. It does not impose a separate network
  shutdown or automatic quarantine rule.
- Closed owner-visible recovery states are `stopping`, `paused_at_checkpoint`,
  `emergency_paused`, `waiting_for_host_reconnect`, `reconciliation_required`, and
  `incomplete_termination`; each renders one exact next action.

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

The executor is a distinct signed unprivileged runtime rather than a widening of the
planning orchestrator. A separately signed privileged action broker runs on each
host. The executor has no raw administrator/root token; functional full-machine
authority is exercised through the broker, whose fixed deny boundary protects the
master service and database, audit, signing keys, broker identity/configuration, and
pause/epoch enforcement.

Every action uses a versioned envelope containing action and feature identities,
session and child epochs, action type, target identities, command or operation
digest, working-directory identity, environment-key names without values, effect
classification, deadline, cancellation behavior, and one adapter-owned
reconciliation strategy. The broker rejects unknown action types or any protected
control-plane target before effect. Generic shell processes receive a stripped token
and protected-control-plane ACL denial; operations that require privilege use typed
broker adapters.

All directly launched processes must belong to an OS-enforced descendant boundary that
untrusted code cannot leave. A Windows Job Object may satisfy that boundary; a macOS
process group alone does not, so Mac execution remains unavailable. Privileged adapters never accept
an executable, script, interpreter input, command line, service body, scheduled-task
body, dynamic library, or plugin selected by a feature. Arbitrary code never runs as
SYSTEM, root, the master identity, or the broker identity.

A feature may install durable executable software only under the restricted feature
execution identity. Its immutable executable digest, fixed arguments, environment-key
allowlist, service/task identity, and protected-control-plane denial profile are
recorded before activation. That identity cannot modify the broker/master services,
their configuration or update paths, authority state, audit, trust roots, or reserved
resources. The completed durable effect is not claimed to be an executor process and
Stop or Emergency Pause does not roll it back. Untracked process delegation is
rejected. Checkpoints commit before and after externally effectful steps.

### Protected Control Plane And Broker TCB

The protected closure includes the master and broker process identities, binaries,
service definitions, IPC endpoints, configuration, database and backups, audit,
owner-control tokens, signing and encryption keys, code-signing trust roots, release
evidence, update/install staging, enforcement policy, execution-session state, and
the CPU, memory, process, and disk reservation needed to observe and enforce pause.

On Windows, the master and broker use dedicated service SIDs and LocalSystem-owned
ordinary single-link storage whose ACL excludes the feature execution identity. The
feature identity receives no administrator group membership, ownership, backup,
restore, debug, driver, impersonation, service-control-manager, task-scheduler,
security-policy, audit-policy, or trusted-installer privilege. Closed broker adapters
may perform a bounded system change but may not change the protected closure or grant
one of those privileges. Durable feature services and tasks run only as the restricted
feature identity and never as SYSTEM or a privileged account.

On macOS, the broker is a separately signed root launch daemon with root-owned
non-writable code and state. The feature executor uses a dedicated unprivileged
identity. It receives no entitlement, authorization right, task port, Full Disk
Access database mutation, launchd-system-domain mutation, security-policy mutation,
or code-signing bypass that could alter the protected closure. Closed root adapters
reject executable or script input and cannot install privileged persistent code.

Master/broker installation or update is an owner-run stopped-service ceremony from an
exact signed release and is unavailable to feature execution. Broker and master keep
reserved process slots, a bounded private state volume/quota, and memory/CPU priority
outside executor job limits. Exhausting unreserved host resources may fail a feature,
but cannot authorize it to consume or rewrite the reservation.

The Windows execution-host security substrate is represented by a dedicated
idempotent provisioning tool. `DryRun` is non-mutating; `Check` requires the exact
LocalSystem Master/Broker plus restricted LocalService Executor service-SID model and
rejects owner-account or user-local deployment; `Apply` requires an elevated explicit
stopped-service ceremony and never starts a service or enables effects. Master and
Broker use distinct unrestricted service SIDs in their LocalSystem tokens; these are
capability tags, not distinct logon accounts. The Executor uses LocalService with a
restricted service SID, and that exact SID is denied on protected service objects,
configuration, state, audit, update staging, and the allocated disk reserve. Protected
receipts contain categories only, never paths or secrets.
The Executor's immutable launch path and exact configuration are the narrow
exception to total read denial: the restricted service SID receives read/execute
only through non-inheriting grants on those exact files and required ancestors while
an inheritable mutation-rights deny blocks write, delete, permission, and ownership
changes on current and later descendants. Runtime configurations must also carry the
Windows read-only attribute. Siblings and later-created files inherit no Executor read
grant. All other protected storage stays unavailable to that SID until a later bounded
state/IPC design grants an exact need.

Check and Apply accept only the three fixed filenames below canonical Program Files,
bind their exact SHA-256 values and one exact valid Authenticode signer through the
SYSTEM-owned protected release manifest, and verify protected install-root, bin-root,
manifest, and image ACLs. A source-built unsigned image cannot satisfy Apply. This is
intentional: production provisioning stays unavailable until signed release evidence
exists instead of treating a repository build as an installed identity.
Each SCM ImagePath is a complete canonical role-specific argv: Master binds the fixed
ProgramData authority directory, loopback bind, service name, mode, and LocalSystem
identity; Broker and Executor bind their fixed service-host modes, service names,
configuration paths, and manifest configuration digests. Extra, reordered, alternate,
remote-bind, or omitted arguments reject. All three substrate services are disabled,
stopped own-process/noninteractive services with no dependencies, triggers, recovery
command/actions, or launch protection, and with exact role-specific required privileges.
This is a pre-activation persistence contract, not permission to start them.
The Broker and Executor binaries implement the corresponding Windows ServiceMain
and Stop/Shutdown control-handler lifecycle. Their native E2E starts and stops the
actual shipped binaries with randomized disposable service names and exact
digest-bound read-only configurations, then proves hostile digest and argv state
does not become a running service. Broker ServiceMain constructs its effect-disabled
semantic runtime before reporting `RUNNING`; Executor ServiceMain validates the full
semantic bootstrap without creating an active runtime or materializing a signing
secret. A correctly re-digested schema-invalid config must stop with a service-specific
failure. Serialized Executor configuration contains no receipt-signing seed. When an
optional protected IPC bootstrap is present, these entrypoints also host one local-only
service-SID-authenticated named-pipe endpoint. Broker and Executor acknowledgement
secrets are separate 32-byte protected leaves supplied out of band; only their paths
and pinned public identities are serialized. The disposable three-service E2E proves a
valid Master-to-Broker-to-Executor inert roundtrip, wrong SID/unsigned/tampered/gap/
stale-authority rejection, restart exact-ack replay, and zero effects. This is
authenticated inert IPC evidence only; product routing, adapter execution, active
cancellation, signing/install cutover, and effect activation remain separate phases.

The protected policy records concrete Windows Job CPU-rate, commit, and active-process
limits that the production executor must attest before any effect can activate. The
provisioner also installs auditable master/broker IFEO priority settings and allocates a
LocalSystem-owned, non-sparse, non-compressed disk-reserve file. The registry effects
gate is written and verified off before any other Apply mutation. Existing policy or
reserve leaves are accepted only when already ordinary, single-link, protected, and
byte-exact; otherwise Apply rejects rather than following, truncating, or repairing a
hostile leaf. The Job values are policy substrate only until the
production executor creates and verifies the Job; they are not yet an enforced runtime
reservation and do not make execution available.
Apply re-reads all three stopped states before and after every mutation cluster and
before its receipt. It establishes disabled start before SID, DACL, storage,
reservation, or priority mutation; any drift leaves the effects gate off and yields no
success receipt.

## Failure And Recovery

- Brainstorming failure creates neither repository nor feature.
- Repository creation persists intent before GitHub access and reconciles ambiguous
  results without blindly creating a second repository.
- Stop prevents new steps, reaches the next checkpoint, and terminates descendants.
- Emergency Pause terminates descendants immediately and retains the latest durable
  checkpoint.
- A disconnect pauses the feature. Resume requires both hosts to bind the same current
  execution epoch and checkpoint.
- Completed checkpointed actions are not repeated. An effect-possible action resumes
  automatically only when its versioned adapter defines and satisfies an exact
  reconciliation predicate. Otherwise the feature enters `reconciliation_required`
  for owner resolution; no per-action prompt is added to normal execution.
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
  no blind retry; checkpointed resume; backup-first migration with verified rollback
  before the new schema becomes authoritative.
- Responsiveness: authenticated local state changes appear in the UI within one
  second of receipt. Stop permits at most five seconds to reach a declared checkpoint,
  then allows five seconds for graceful termination and five seconds for forced
  termination. Any survivor enters `incomplete_termination`.
- Provider budget: a brainstorming attempt accepts at most 16 KiB of owner input,
  produces at most 64 KiB of strict specification output, makes at most three provider
  calls, and allows at most 15 minutes per call. There is no automatic provider retry
  or fallback.
- Persistence budget: retained brainstorming, checkpoint, and action metadata is
  capped at 1 MiB per feature excluding SQLite indexes; raw process output and file
  content are not retained. Audit retention remains durable and owner-managed.
- Resource boundary: executor processes may consume host resources, but broker and
  master reservations must retain sufficient process, memory, and disk capacity to
  observe Stop and Emergency Pause. Exhaustion that defeats the reservation fails the
  feature and blocks resume until health is restored.
- Security: planning has no execution authority; execution authority is explicit,
  epoch-bound, revocable, and auditable even though its granted scope is machine-wide.
- Maintainability: strict versioned protocol types, database migrations, native Rust
  and Swift tests, and no dynamic provider or skill loading.

## Native Verification Strategy

- Swift unit tests cover the simplified forms, URL/visibility validation, default
  orchestrator, queue-empty Start gating, auto-run default/toggle, Stop, and Emergency
  Pause presentation.
- Rust unit and kernel tests cover URL identity mapping, public/private creation
  intent, brainstorming freeze/approval, execution epochs, single-feature FIFO,
  auto-run advancement, checkpoint resume, stale-epoch rejection, and redacted audit.
- Native process E2E uses disposable Mac and Windows fixtures to prove process-tree
  termination, checkpoint recovery, cross-host epoch binding, GitHub adapter
  reconciliation, and one-at-a-time advancement. It must not use real destructive
  machine-wide operations as repository-gate evidence.
- Hostile native broker tests attempt direct and indirect protected-file mutation,
  ACL/ownership takeover, trust-root or updater replacement, SYSTEM/root service and
  task creation, child detachment, remote/untracked delegation, reparse/symlink
  substitution, broker identity drift, and resource-reservation exhaustion. Every
  attempt must fail before effect and leave the control plane healthy and auditable.
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
| Protect the Assemblywright control plane | Literal access to the enforcement substrate | Owner approved the sole exception required for Windows authority, audit, Stop, and Emergency Pause to remain enforceable. |
| Broker full-machine effects through versioned actions | Give the worker a raw administrator/root token | Brokered access preserves functional machine authority while protecting the control plane and producing actionable provenance. |

## Known Architecture Changes

This design deliberately targets replacement of the current restricted-worker
execution policy. The target documentation, versioned protocol contracts, backup-first
schema-v20 planning persistence/routes, contained source adapters, MacCore planning
transport, and simplified authoritative queue/auto-run Mac UI exist. Implementation
still requires Windows capability-separated enablement for the planning and GitHub
adapters, Start/Stop/Emergency effect routes, executor containment and recovery,
privileged brokers, release evidence, and native hostile/live E2E. Until those changes
and their evidence are complete, the existing bounded executor remains authoritative
and the new UI must not claim full-machine execution readiness.

## Implementation And Evidence Phases

The target lands as gated slices. Completion of an earlier phase never implies the
authority or proof of a later phase:

1. **Contract acceptance:** canonical design, safety, architecture, knowledge, build,
   and drift documents agree on the target and current/target boundary. This phase
   is complete and changes no runtime authority.
2. **Versioned authority foundation:** protocol types, strict action envelopes,
   repository-URL identity, brainstorming and GitHub intents, assembly-line sessions,
   child epochs, checkpoints, redacted audit, and backup-first schema migration land
   with negative-path and compatibility tests. The strict protocol contract and
   backup-first schema-v20 inert planning subset are implemented. No session, child
   epoch, action envelope, termination, executor, broker, or external effect is issued.
3. **Planning and creation:** allowlisted planning-only brainstorming, Public-only
   disclosure binding, frozen owner approval, and idempotent GitHub
   creation/reconciliation are implemented in source with a real cross-process adapter
   E2E. Windows production loading remains fail-closed until distinct restricted
   planning and GitHub identities/ACLs, atomic Job Object containment, provisioning,
   and native denial proof land. This authorizes no execution.
4. **Broker containment:** the first source slices have landed: strict signed
   envelopes and receipts, distinct unprivileged executor/privileged broker crates,
   complete protected-manifest digest binding, replay/gap rejection, held macOS
   target-identity validation with spawn disabled, held-image Windows Job Object source,
   portable hostile filesystem tests, and a proof-only Windows `CreateDirectory` adapter
   with complete held ancestry and explicit effect-possible reconciliation evidence. Its
   production dispatch remains disabled. A guarded Windows host-security provisioner,
   source contract, and disposable hostile native service E2E now define the required
   service-SID/ACL/resource-policy substrate; an owner-account production service does
   not satisfy it and remains untouched before a stopped-service cutover. Completion
   still requires successful production provisioning, protected IPC, durable replay
   restore, remaining closed adapters, runtime Job enforcement, OS code signatures,
   and owner-recorded deployment evidence. Repository tests are not signed or
   deployed-host proof.
5. **Assembly Line control:** the simplified UI, queue-empty Start denial, default-on
   auto-run, single-active child epochs, Stop, Emergency Pause, reconnect, resume, and
   reconciliation states land with native cross-process E2E. The current UI observes
   the authoritative queue and performs only the default-on CAS auto-run setting with
   exact restart-safe reconciliation; brainstorming and Start/Stop/Emergency remain
   disabled and the effect routes are absent. Full-machine
   activation remains default-off.
6. **Cutover and live evidence:** exact signed releases are deployed through the
   owner-only stopped-service ceremony. Windows migration/rollback, GitHub creation,
   signed brokers, protected-control-plane denial, termination, checkpoint recovery,
   and real two-host operation are owner-recorded. Only then may the owner explicitly
   activate the new authority and may the UI claim it is available.

Compatibility is fail-closed. The protocol-v5/schema-v22 master preserves existing
schema-v19 grants, activation evidence, queue rows, and restricted-worker leases with
their old meanings. A new
binary may migrate only exact supported state; records without an unambiguous target
mapping remain inert for owner reconciliation. The forward schema must be rejected by
old binaries. No legacy activation, queue receipt, or owner-control designation may be
interpreted as an Assembly Line session or full-machine grant.

## Structured Review Log

### Skeptic / Challenger

Disposition: changes required.

| Objection | Resolution status | Rationale |
| --- | --- | --- |
| A full-access Windows executor can rewrite the master, database, audit, credentials, and pause enforcement, collapsing Windows authority. | Resolved | Owner approved a protected-control-plane exception; the unprivileged executor uses a signed broker that denies those targets. |
| Arbitrary privileged work can escape a terminable process tree through services, scheduled tasks, remote delegation, or supervisor replacement. | Resolved | Direct children are job/process-group bound; durable system effects require typed actions and are explicitly outside process-termination claims. |
| Arbitrary irreversible effects do not always have an exact reconciliation predicate. | Resolved | Only adapter-defined exact predicates permit automatic reconciliation; all other ambiguity enters owner resolution without retry. |
| Auto-run changes queue and feature bindings, invalidating a single feature-bound epoch. | Resolved | One owner-started line session derives a separate child epoch for each feature after exact predecessor success. |
| The executor has no strict executable action/provenance contract. | Resolved | The design now defines a versioned action envelope and signed privileged broker interception boundary. |

### Constraint Guardian

Disposition: changes required.

| Objection | Resolution status | Rationale |
| --- | --- | --- |
| Machine-wide authority makes authority and redacted audit self-bypassable on the Windows control host. | Resolved | The owner-approved exception and broker boundary deny the protected control plane. |
| Stop and Emergency Pause cannot cover services, scheduled tasks, remote delegation, or a replaced supervisor. | Resolved | Claims are limited to broker-launched processes; durable effects are typed, audited, and never described as terminated or rolled back. |
| Recovery and provenance remain undefined for arbitrary actions. | Resolved | Unknown action types reject; every allowed action defines provenance, cancellation, and reconciliation semantics. |
| Performance, resource, provider-cost, retention, checkpoint-growth, and migration-rollback limits are not measurable. | Resolved | The design now includes explicit input/output/call/time/storage/termination budgets and backup-first rollback. |

### User Advocate

Disposition: changes required.

| Objection | Resolution status | Rationale |
| --- | --- | --- |
| Auto-run off leaves the visible on/off and authorization state ambiguous. | Accepted | The UI must enter `waiting_for_owner_start` after success, show `Start next feature`, and create a new feature execution epoch under the still-configured but inactive line. |
| Recovery states and next owner actions are not visible. | Accepted | The Assembly Line must render closed states for stopping, checkpoint-paused, emergency-paused, host-reconnect wait, reconciliation-required, and incomplete termination. |
| Brainstorming and creation failure recovery is undefined. | Accepted | Each pre-effect failure must show a bounded reason plus Edit and Retry; URL conflict and effect-possible GitHub creation must use exact reconciliation or owner resolution rather than a blind retry. |

### Integrator / Arbiter, Revision 1

Disposition: revision required.

| Objection | Resolution status | Rationale |
| --- | --- | --- |
| Typed persistent services or tasks could still run arbitrary elevated code, escape the process tree, and indirectly alter the protected control plane. | Resolved in revision 2; re-arbitration pending | Privileged adapters are now closed non-executable operations. Arbitrary durable code runs only as the restricted feature identity under a recorded control-plane denial profile. |
| The protected TCB did not name its identities, storage, service configuration, trust/update roots, reservations, or hostile proof. | Resolved in revision 2; re-arbitration pending | The design now defines the complete protected closure, per-platform identities and privilege exclusions, owner-only update ceremony, resource reservation, and hostile native test matrix. |

### Integrator / Arbiter, Revision 2

Disposition: APPROVED.

The arbiter accepted the closed privileged-adapter contract, restricted durable
execution identity, complete protected-control-plane closure, per-platform privilege
exclusions, owner-only update ceremony, resource reservations, and hostile native
verification matrix. Approval is architectural only. The current bounded runtime
remains authoritative. The inert schema-v20 planning migration does not complete the
provider, GitHub, live UI wiring, control-route, executor, broker, deployment, or
owner-recorded live-evidence phases required for execution cutover.
