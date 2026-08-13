# Assemblywright

> Orchestrated intelligence. Verified software.

Assemblywright is an owner-controlled developer-agent system. A Windows master
holds durable authority over an owner-approved feature queue; restricted local
coding agents implement one feature at a time against credential-free
repository snapshots; a frontier model may assist with planning and final
review but never writes to a repository. A macOS app is the owner's control and
observation surface.

This repository is foundation work. The durable contracts, the master kernel,
the enrollment and mTLS identity path, the Windows service lifecycle, the Mac
bridge and worker agent, and the release gate are implemented. Autonomous
dispatch, general-purpose repository execution, review-provider invocation,
GitHub publication, and the queue control UI are still design. The Windows
master now implements the first repository-mutation boundary: admitted
artifacts can be integrated only into an isolated master-owned candidate
repository, never the registered source checkout.

## What Is Implemented

**`assemblywright-protocol`** — the current protocol version: typed device, task,
step, attempt, lease, and cancellation identifiers; bounded capability
advertisements; handshake, job, and result envelopes; strict bound-before-decode
JSON entry points; nil-identity rejection; fixed-schema Feature Conveyor owner-
control request/receipt contracts; and a golden compatibility fixture.

**`assemblywright-master`** — a portable schema-v15 SQLite database retaining
the schema-v4 device lifecycle, schema-v5 Feature Conveyor, schema-v6
capability rebind evidence, schema-v7 Emergency Pause revision, and schema-v8
single owner-control bridge designation, schema-v9 immutable repository
snapshot-claim evidence, schema-v10 metadata-only coding-dispatch evidence, and
schema-v11 immutable owner-resolution origin evidence, schema-v13 immutable
result-artifact metadata, schema-v14 immutable artifact-integration and
candidate-commit evidence, and schema-v15 immutable validation-gate evidence
with backup-first fail-closed legacy migration,
plus a headless single-owner
executable.

- *Distributed device lifecycle*: registered device metadata, connection epochs
  and sequence high-water marks, queued steps, immutable leased job envelopes,
  attempts, cancellation and expiry outcomes, accepted payload digests, and a
  metadata-only event journal with one server-issued stream ID and contiguous
  sequence. It enforces a 256-step admission ceiling, four global leases, one
  live lease per device connection, exact leased-attempt result identity, and
  durable abandon-before-reissue on disconnect or restart.
- *Feature Conveyor repository kernel* (default-inert): immutable owner-approved
  specification revisions binding canonical manifest and evidence digests,
  three independent repository-grant revisions, dependencies, and a snapshotted
  review provider. Its queue has a 100-item nonterminal ceiling, one global
  compare-and-set revision, strict head and dependency ordering, and one durable
  active lease. A loopback-only owner action creates one independent,
  credential-free, no-remote shallow snapshot from only the exact current
  commit/tree/blob graph, excluding parent history and deleted objects, and
  atomically binds it to the exact strict queue head, provider, grants, pause,
  and queue revisions. A fail-fast singleton reservation serializes bounded
  snapshot work and remains held by any timed-out blocking task until cleanup.
  A separate owner-token loopback-only action may explicitly queue one
  `local.coding.v1` metadata-only work packet bound to that exact feature lease,
  snapshot ID and digest, lifecycle, queue and Emergency Pause revisions, and
  one exact current `InferenceWorker` registration. The action, queued step,
  distributed event, immutable dispatch binding, and redacted audit commit in
  one transaction. After the exact lease, the separate default-off transfer
  lane gives the native agent only bounded authenticated snapshot chunks. It
  reconstructs a private no-remote workspace and applies only the immutable
  protocol-v5 packet's bounded deterministic `file.write.v1` and
  `file.delete.v1` operations across its sorted normalized relative-path
  allowlist. Descriptor-relative no-follow traversal, atomic create/swap/delete,
  exact changed-path verification, and a canonical multi-file artifact remain
  bounded by the packet. There is no child, shell, `exec`, provider, test,
  credential, or network authority. Success seals the workspace plus one
  separate private recovery record until exact resolution or bounded expiry;
  this does not claim a host sandbox or host-level egress control.
  A further owner-token loopback-only action integrates the complete exact set
  of terminal accepted artifacts for the active feature; a path-free owner-local
  plan projection supplies its current exact IDs and revision bindings. The
  master reopens and
  re-hashes each private artifact, independently validates its protocol-v5
  operations against the immutable dispatch, derives application order from
  immutable packet ordinals, and applies only non-overlapping changes to a
  private no-remote integration repository created from the claimed snapshot.
  Conflict or compare-and-set drift records bounded redacted evidence without a
  partial candidate or lifecycle advance. Success freezes one exact Git commit
  and tree, durably links every artifact to it, advances only
  `implementing -> validating`, and makes exact retries idempotent. It never
  mutates the registered source checkout and grants no test, review,
  publication, credential, network, or autonomous authority.
  Schema v15 adds a separate owner-loopback test/evidence gate. When a fixed
  private Windows toolchain and credential-free offline dependency cache are
  provisioned, the master validates a disposable copy of the exact frozen
  candidate, binds the approved design digest to exact admitted requirements
  paths, and runs deterministic docs/knowledge/safety/secret checks plus fixed
  70%-minimum line coverage, unit, native-E2E, format, lint, build, and
  repository commands, and stores only immutable result digests and bounded
  metadata. All required passes advance only `validating -> reviewing`.
  Missing runner provisioning rejects before creating an attempt; no review,
  publication, or registered-source mutation authority is implied.
  Two additional owner-token loopback-only resolution actions cancel one exact
  active feature and explicitly abandon-and-advance one already cancelled or
  quarantined feature. Both bind the feature, lifecycle, queue, and Emergency
  Pause revisions inside the authoritative transaction. Cancellation cancels
  bound coding work but retains the feature lease and never advances the queue;
  abandonment requires a nonzero safe-reconciliation digest and, after any
  merge, a verified healthy-main digest before releasing the lease. Their
  receipts are path-free and redacted, and neither action exists on enrolled-
  device mTLS.
  Enqueue, reorder, claim, lifecycle, cancellation, abandonment,
  and startup quarantine commit with redacted audit evidence in the same
  transaction. Success releases the lease only with verified healthy-main
  evidence; cancellation retains it until explicit safe abandonment. One
  owner-token-authenticated loopback-only `GET /v1/feature-conveyor/status`
  exposes bounded, redacted lifecycle observation for current queue and
  retained-lease entries plus one fixed-enum owner-guidance summary bound to
  queue, Emergency Pause, and optional feature lifecycle revisions. The
  guidance distinguishes the queue head, dependency blocking,
  retained-lease reconciliation, and Emergency Pause without exposing
  dependency identifiers or asserting claimability. Its action labels are
  display-only. A dedicated
  `GET /v1/distributed/feature-conveyor/status` returns that same projection
  only after an exporter-bound application session is accepted for an enrolled
  MacBridge; other device roles are denied and no owner token is forwarded.
  Neither observation route grants mutation, execution, review, repository,
  Git, publication, activation, or callable owner-action authority. The
  owner-token loopback-only `POST /v1/feature-conveyor/repository-grants` and
  `GET /v1/feature-conveyor/repositories/:repository_id/grants` routes prepare
  and inspect one current digest-only revision for each independent grant.
  Recording is contiguous, compare-and-set and Emergency-Pause-revision bound;
  active authority is blocked while paused, revocation remains available, and
  redacted audit commits atomically. The routes inspect no repository and are
  absent from enrolled-device mTLS. A separate owner-token loopback-only
  `POST /v1/feature-conveyor/repository-preflight` accepts one strict local
  repository scope whose canonical digest, registration-grant revision, exact
  single-component base branch, and exact HEAD commit are owner-bound. It performs only a
  bounded, point-in-time filesystem identity inspection of a standard local
  `.git` directory, symbolic HEAD, and exact loose branch ref. It executes no
  Git process, loads no repository configuration or attributes, and rejects
  UNC, device, mapped/non-fixed-volume, reparse, worktree, and submodule paths.
  On Windows it holds non-reparse filesystem handles for the fixed-volume path,
  identity directories, symbolic HEAD, and loose ref, then reopens and compares
  the complete canonical pathname and identities immediately before the final
  grant, Emergency Pause, and audit transaction recheck.
  It does not prove a clean tree or inspect repository content. The path is
  neither returned nor stored; success returns a
  path-free digest receipt only after the active grant and Emergency Pause
  revision are rechecked and a redacted audit commits atomically. Separately,
  an owner-token-authenticated loopback action designates exactly one current
  non-fixture MacBridge under compare-and-set revision and atomic redacted
  audit. Only that exact device, after a fresh exporter-bound application
  session, may call
  `POST /v1/distributed/feature-conveyor/approved-features` with a strict,
  bounded, already-approved specification bound to the current queue,
  designation, and Emergency Pause revisions. The action only appends the
  immutable specification and queued row; it does not claim, dispatch, execute,
  review, publish, or activate the feature.
- *Identity*: a DPAPI-current-user protected ECDSA P-256 enrollment CA,
  ten-minute single-use digest-only grants, verified client CSRs, 30-day client
  certificates bound to a server-selected device ID, rotation, and immediate
  certificate and device revocation. An explicit
  `serve --remote-bind <ip>:<port>` adds a TLS 1.3-only listener that requires
  enrolled client certificates, rechecks revocation per request, and binds the
  application handshake to the TLS exporter.
- *Windows service*: install, start, stop, status, maintenance enter/exit,
  recover, and uninstall, with automatic start, bounded 5/15/60-second restart
  attempts, and a durable fail-closed maintenance marker that blocks new
  enqueue and lease admission while already-started results settle.

**`assemblywright-agent`** — the Mac worker. It reuses the hardened local UDS
transport, requires direct-parent supervision and a fresh startup-stdin bearer,
and stores only stream ID, sequence, and update time under a single-owner lock.
Its default-off fixture lane holds at most one synthetic echo attempt in
memory. Its default-off singleton MLX lane runs one bounded, no-retention
request with a cleared offline environment, prompt-only stdin, bounded stdout,
null stderr, and dedicated process-group reaping. Cancellation, timeout,
disconnect, or emergency pause dominates completion and suppresses late output.
Its mutually exclusive local-coding lane applies only protocol-v5 deterministic
write/delete operations inside a bounded normalized relative-path allowlist,
reports tests as not run, constructs one canonical multi-file patch artifact,
and seals the per-attempt workspace until exact resolution or bounded expiry.

**`assemblywright-core`** — the shared local foundation: the peer-identity Unix-socket
transport, its startup validation, and read-only release readiness and evidence
inspection. It holds no conversation, model, tool, memory, scheduler, plugin, or
repository authority.

**`apps/mac`** — a SwiftUI Developer Mode client. The app supervises only the
exact separately signed bridge helper; the helper keeps the Secure Enclave
Keychain identity and the outbound mTLS session, directly supervises the pinned
agent, and forwards authenticated metadata pages into a durable cursor. The
enrolled key and mTLS session never leave the helper. The helper also strictly
  decodes the bounded schema-v8 Feature Conveyor projection and the app renders
  its queue/guidance summary as read-only text only in authenticated state. A
  separate one-shot signed-helper command,
  `feature-conveyor approve-and-enqueue --confirm`, reads one bounded approved
  request from stdin, uses the standard Keychain identity without forwarding an
  owner token, and emits only a revision-bound redacted receipt.

## Current Scope

Risky side effects must be blocked or require approval, and every meaningful
decision must be auditable. Do not describe this as a finished product.

Not yet implemented, and not claimed:

- Autonomous dispatch, registered-source-checkout mutation, or publication of any kind. The
  implemented preflight remains read-only; the default-off owner-local
  snapshot claim creates durable isolated state and one lease, and a separate
  explicit owner-local action may queue one snapshot-bound metadata-only coding
  admission. The exact leased lane transfers repository material and performs
  only packet-bound deterministic writes/deletes across at most 64 sorted
  normalized relative paths with 4 KiB aggregate replacement data. It seals a
  successful attempt workspace for exact resolution or bounded expiry, but
  accepts no shell, arbitrary tool, provider, test, network, or credential
  operation. The schema-v13 master can admit the exact canonical multi-file
  artifact bytes into private state and bind a metadata-only result and retention
  expiry to them. Schema v14 can apply the complete accepted artifact set only
  to a master-owned isolated integration repository and freeze an exact
  candidate commit; it cannot execute tests, mutate the registered source
  checkout, invoke a provider, review, publish, or activate the feature.
- Worker execution against real repositories, review-provider invocation, or
  GitHub branch/PR/merge authority.
- Mac Feature Conveyor UI controls or hosted brainstorming. The app remains
  observation only. The implemented one-shot signed-helper action can enqueue
  only an already-approved specification after separate Windows owner-bridge
  designation and three current repository-grant revisions.
- Developer ID signing, notarization, stapling, clean-profile installation, or
  Finder/LaunchServices validation.
- Live cross-device reliability, host hardening, or unattended operation.

Repository validation is deliberately distinct from signing, notarization,
live-device QA, and owner-recorded external evidence. A green local gate is not
a production-readiness claim.

## Build And Test

For executable PR evidence, run the canonical local release gate:

```sh
./scripts/release-local.sh
```

It wraps version consistency, CI workflow and docs drift smoke, the Mac/Windows
bridge live-E2E preflight, Rust fmt/clippy/tests including ignored release
proofs, cargo package verification, distribution self-tests, the unsigned
structure and launch checks, release runbooks, evidence preflights, external
handoff generation, and the Swift build and test suites.

Focused commands for local iteration:

```sh
cargo test --workspace
```

```sh
swift test --disable-sandbox --package-path apps/mac
```

```sh
cargo run -p assemblywright-cli -- release readiness
```

The `assemblywright` CLI is a read-only release and evidence client. Its
subcommands prefer a configured IPC endpoint and fall back to local metadata or
local file and report inspection; they execute no release side effects.

Canonical commands and their exact proof boundaries live in
[docs/build-test-commands.md](docs/build-test-commands.md).

## Docs

- [Feature Conveyor design](docs/feature-conveyor-design.md) — the approved
  target design and the implemented repository-kernel slice.
- [Distributed Developer Mode design](docs/distributed-developer-mode-design.md)
  — accepted authority, security, routing, recovery, and rollout target.
- [Architecture map](docs/architecture-map.md) — current implementation and its
  evidence boundary.
- [Safety rules](docs/safety-rules.md)
- [Build and test commands](docs/build-test-commands.md)
- [Release checklist](docs/release-checklist.md)
- [Development agent workflow](docs/development-agent-workflow.md)
- [Knowledge-base facts](docs/knowledge-base/assemblywright-project-facts.md)
- [Brand system](docs/brand.md)

## License

Assemblywright is licensed under the
[Apache License 2.0](LICENSE). The crates are named `assemblywright-*`. A few
`ASSEMBLYWRIGHT_*` / `assemblywright` identifiers survive deliberately as compatibility
contracts: environment variable names, Keychain and Application Support
namespaces, the `com.nobiletechnology.assemblywright` code-signing identity, and the
bundled CLI filename inside the app. Those bind installed state and signed
artifacts; they are not the product name.
