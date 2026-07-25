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
dispatch, repository execution, review-provider invocation, GitHub publication,
and the queue UI are still design.

## What Is Implemented

**`assemblywright-protocol`** — the current protocol version: typed device, task,
step, attempt, lease, and cancellation identifiers; bounded capability
advertisements; handshake, job, and result envelopes; strict bound-before-decode
JSON entry points; nil-identity rejection; and a golden compatibility fixture.

**`assemblywright-master`** — a portable schema-v5 SQLite kernel plus a headless
single-owner executable.

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
  active lease. Enqueue, reorder, claim, lifecycle, cancellation, abandonment,
  and startup quarantine commit with redacted audit evidence in the same
  transaction. Success releases the lease only with verified healthy-main
  evidence; cancellation retains it until explicit safe abandonment. One
  owner-token-authenticated loopback-only `GET /v1/feature-conveyor/status`
  exposes bounded, redacted lifecycle observation for current queue and
  retained-lease entries. It does not report dependencies, blocker reasons,
  claimability, or owner-action guidance and must not drive owner action. It is
  absent from the enrolled-device remote mTLS router and grants no mutation,
  execution, review, repository, Git, publication, or activation authority.
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

**`assemblywright-core`** — the shared local foundation: the peer-identity Unix-socket
transport, its startup validation, and read-only release readiness and evidence
inspection. It holds no conversation, model, tool, memory, scheduler, plugin, or
repository authority.

**`apps/mac`** — a SwiftUI Developer Mode client. The app supervises only the
exact separately signed bridge helper; the helper keeps the Secure Enclave
Keychain identity and the outbound mTLS session, directly supervises the pinned
agent, and forwards authenticated metadata pages into a durable cursor. The
enrolled key and mTLS session never leave the helper.

## Current Scope

Risky side effects must be blocked or require approval, and every meaningful
decision must be auditable. Do not describe this as a finished product.

Not yet implemented, and not claimed:

- Autonomous dispatch, repository mutation, or publication of any kind.
- Worker execution against real repositories, review-provider invocation, or
  GitHub branch/PR/merge authority.
- The Mac queue UI, hosted brainstorming, or `Approve and Enqueue`.
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
