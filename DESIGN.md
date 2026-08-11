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

`assemblywright-master` owns durable state and every authority decision. Its schema-v10
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

Schema v10 adds one separate owner-token-authenticated loopback-only coding-
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
chunk, bundle, path, and aggregate digests, and immediately removes the
materialized repository before returning a path-free
`snapshot_materialized` receipt. Cancellation, shutdown, restart, malformed or
out-of-order chunks, identity drift, links, duplicate/colliding/unsafe paths,
trailing data, and digest or size drift fail closed and clean partial state. No
repository source path, command, tool, coding process, provider, credential,
patch, retained workspace, commit, mutation, integration, review, publication,
queue advancement, or autonomous activation exists in this slice.

The local-coding lane uses a separate `local-coding` Secure Enclave/Keychain
identity namespace. Its enrollment profile accepts only the
`inference_worker` role with the exact singleton `local.coding.v1` capability;
the standard and fixture profiles remain exact `mac_bridge` identities. The
process lifecycle selects that identity only when the local-coding snapshot
opt-in is explicitly enabled, and capability rebind remains standard-profile
only.

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
  agent relay. The local-coding snapshot lane additionally runs the production
  Swift relay and launcher against the real supervised Rust agent process.
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
