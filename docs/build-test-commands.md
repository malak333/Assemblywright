# Build And Test Commands

Run commands from the repository root unless noted otherwise.

## Required Local Gate

Run the full local release gate with:

```sh
./scripts/release-local.sh
```

The script is a wrapper around the ordered command set below and intentionally
stays local-only. Use this gate as the default PR evidence for current
foundation work unless a narrower docs-only change justifies a focused
documentation check.

On GitHub, `.github/workflows/release-local.yml` runs the same gate on
`macos-15` with SHA-pinned checkout/toolchain actions and Rust `1.95.0` for
pull requests, pushes to `main`, and manual dispatch. CI sets
`ASSEMBLYWRIGHT_RELEASE_LOCAL_HEARTBEAT_SECONDS=60` so long-running commands
periodically print elapsed-time heartbeat lines without changing the canonical
command list or proof boundary. The workflow is configuration evidence only; it
does not perform Developer ID signing, notarization, clean-profile
installation, Finder launch validation, or live-device QA. Release readiness
exposes this lane as `release_ci_gate` with the same boundary.

The gate runs, in order:

```text
./scripts/release-version-consistency.sh --check
./scripts/release-ci-workflow-smoke.sh
./scripts/release-docs-drift-smoke.sh
./scripts/release-naming-contract-smoke.sh --check
./scripts/release-naming-contract-smoke.sh --self-test
./scripts/release-shell-portability-smoke.sh --check
./scripts/release-shell-portability-smoke.sh --self-test
./scripts/release-protocol-version-contract-smoke.sh --check
./scripts/release-protocol-version-contract-smoke.sh --self-test
./scripts/mac-windows-bridge-live-e2e.sh --check
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace -- --ignored
cargo build --workspace
./scripts/release-cargo-package.sh
./scripts/package-distribution.sh --check
./scripts/package-distribution.sh --check-guidance-self-test
./scripts/package-distribution.sh --entitlements-policy-self-test
./scripts/package-distribution.sh --version-consistency-self-test
./scripts/package-distribution.sh --provenance-self-test
./scripts/package-distribution.sh --running-app-guard-self-test
./scripts/package-distribution.sh --running-app-guard-e2e
./scripts/package-distribution.sh --unsigned-launch-check
cargo run -p assemblywright-cli -- release signed-distribution-runbook
cargo run -p assemblywright-cli -- release live-device-runbook
./scripts/release-live-device-qa.sh --check
./scripts/release-live-device-qa.sh --self-test
./scripts/release-evidence-bundle.sh --check
./scripts/release-evidence-bundle.sh --self-test
./scripts/release-evidence-doctor.sh --check
./scripts/release-evidence-doctor.sh --self-test
./scripts/release-external-handoff.sh --check
./scripts/release-external-handoff.sh --self-test
swift test --disable-sandbox --package-path apps/mac
swift build --disable-sandbox --package-path apps/mac
```

## Windows Distributed Gate

The schema-v9 snapshot-claim, schema-v10 coding-dispatch, schema-v11 owner-resolution,
schema-v12 result-artifact admission, and ephemeral snapshot-transfer/materialization slices have focused portable
coverage:

```sh
cargo test -p assemblywright-protocol --test protocol_contract repository_snapshot_claim_contract_is_strict_exact_and_path_free_on_receipt
cargo test -p assemblywright-master --lib snapshot::tests
cargo test -p assemblywright-master --test feature_conveyor_kernel
cargo test -p assemblywright-master --test feature_conveyor_kernel artifact -- --nocapture
cargo test -p assemblywright-master --test master_process_e2e repository_preflight_is_owner_only_filesystem_identity_observation_and_redacted
cargo test -p assemblywright-master --test master_process_e2e repository_snapshot_claim_is_authenticated_path_free_and_durable
cargo test -p assemblywright-master --bin assemblywright-master snapshot_claim_reservation_survives_blocking_task_timeout
cargo test -p assemblywright-protocol --test local_coding_contract
cargo test -p assemblywright-protocol --test owner_resolution_contract
cargo test -p assemblywright-master --test feature_conveyor_kernel coding_dispatch
cargo test -p assemblywright-master --test feature_conveyor_kernel owner_resolution
cargo test -p assemblywright-master --test feature_conveyor_kernel master_process_v10
cargo test -p assemblywright-master --test master_process_e2e owner_resolution_routes_are_authenticated_strict_cas_bound_and_redacted -- --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel emergency_pause_cancels_coding_attempt_and_resume_rejects_pre_pause_acknowledgement
cargo test -p assemblywright-master --test feature_conveyor_kernel terminal_coding_ack_allows_validation_and_lifecycle_change_invalidates_replay
cargo test -p assemblywright-master --test feature_conveyor_kernel result_artifact_admission_is_exact_idempotent_and_required_before_result
cargo test -p assemblywright-master --test feature_conveyor_kernel artifact_store_exact_retry_and_startup_orphan_cleanup_fail_closed
cargo test -p assemblywright-agent --test local_coding_admission
cargo test -p assemblywright-agent snapshot::tests
cargo test -p assemblywright-agent --test local_relay_e2e authenticated_uds_local_coding_snapshot_admission_cancellation_and_restart_cleanup -- --nocapture
cargo test -p assemblywright-master --test remote_mtls_e2e remote_local_coding_dispatch_is_exporter_bound_exact_and_pause_dominant -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter DeveloperBridgeTests
./scripts/mac-windows-bridge-live-e2e.sh --run-local-coding
```

The portable real-process route test proves authentication, path-free response,
failure-without-lease, durable snapshot/lease binding, and source path/content
absence from durable authority. The additional positive identity proof is
Windows-native because fixed-volume non-reparse handle admission is authoritative
there. It must use a disposable standalone
repository and additionally prove owner authentication, remote route absence,
path-free receipt/audit, independent no-remote Git metadata, no lease on
failure, unreferenced-snapshot cleanup, and restart quarantine. Portable/macOS success is
repository implementation evidence, not live Windows-service proof.

The portable distributed foundation has a separate Windows gate. For a fresh
Windows checkout, install the MSVC Rust toolchain pinned by
`rust-toolchain.toml` after the Visual Studio C++ Build Tools and Windows SDK
are present:

```powershell
rustup toolchain install 1.95.0 --profile minimal --component clippy --component rustfmt
```

`.github/workflows/windows-protocol.yml` runs formatting, clippy, the protocol
and master crates, the master-process E2E, and the elevated Windows SCM service
lifecycle E2E. The native master tests are the authoritative proof for Windows
final-DOS-path normalization, alternate-path rejection, POSIX rename/replacement
revalidation, and held-handle identity comparison; macOS execution cannot prove
those Win32 behaviors.

Use the workflow's package-scoped protocol and master clippy commands on
Windows. Do not substitute the macOS/Linux workspace-wide clippy command: the
workspace also contains Unix-only local transport targets, so that substitution
fails before it reaches the authoritative Windows protocol/master lanes.

## Focused Commands

Use these when iterating on one surface, then run the full gate before treating
executable changes as release evidence.

| Surface | Command |
| --- | --- |
| Rust format | `cargo fmt --check` |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Whole workspace | `cargo test --workspace` |
| Protocol contract | `cargo test -p assemblywright-protocol` |
| Master kernel | `cargo test -p assemblywright-master` |
| Feature Conveyor kernel, grant CAS/projection, owner designation, enqueue, snapshot claim, coding dispatch, and status | `cargo test -p assemblywright-master --test feature_conveyor_kernel` |
| Master process E2E, including authenticated loopback grant/preflight/snapshot/dispatch/status/designation routes | `cargo test -p assemblywright-master --test master_process_e2e` |
| Windows remote mTLS observer and designated-owner enqueue denial/success | `cargo test -p assemblywright-master --test remote_mtls_e2e remote_listener_requires_enrollment_tls13_and_channel_bound_identity -- --nocapture` |
| Swift strict Feature Conveyor observer, one-shot owner action, and helper lifecycle | `swift test --disable-sandbox --package-path apps/mac --filter DeveloperBridgeTests` |
| Enrollment, two-phase capability rebind, and identity | `cargo test -p assemblywright-master --test enrollment_identity_e2e` |
| Remote mTLS | `cargo test -p assemblywright-master --test remote_mtls_e2e` |
| Windows snapshot-bound coding dispatch and bounded transfer mTLS/process E2E | `cargo test -p assemblywright-master --test remote_mtls_e2e remote_local_coding_dispatch_is_exporter_bound_exact_and_pause_dominant -- --nocapture` |
| Event cursor | `cargo test -p assemblywright-master --test event_cursor_e2e` |
| Windows service lifecycle | `cargo test -p assemblywright-master --test windows_service_lifecycle_e2e -- --ignored` |
| Mac agent relay | `cargo test -p assemblywright-agent --test local_relay_e2e` |
| Native metadata-only coding admission | `cargo test -p assemblywright-agent --test local_coding_admission` |
| Native ephemeral snapshot, fixed contained-coding fixture, cancellation, and cleanup | `cargo test -p assemblywright-agent snapshot::tests` |
| Production Swift relay to supervised Rust-agent contained-coding success plus final-verification cancellation/cleanup-before-ack E2E | `./scripts/mac-local-coding-snapshot-e2e.sh` |
| Local transport and release | `cargo test -p assemblywright-core` |
| Readiness protocol proof unit | `cargo test -p assemblywright-core protocol_readiness_proof_is_version_independent` |
| CLI naming contract E2E | `cargo test -p assemblywright-cli --test naming_contract_e2e` |
| CLI readiness proof E2E | `cargo test -p assemblywright-cli --test release_readiness_e2e` |
| Swift package | `swift test --disable-sandbox --package-path apps/mac` |
| One Swift test | `swift test --disable-sandbox --package-path apps/mac --filter <test>` |
| Codex workflow | `./scripts/validate-codex-workflow.sh` |
| Docs contract | `./scripts/release-docs-drift-smoke.sh` |
| Naming contract | `./scripts/release-naming-contract-smoke.sh --check` |
| Shell portability | `./scripts/release-shell-portability-smoke.sh --check` |
| Protocol version contract | `./scripts/release-protocol-version-contract-smoke.sh --check` |

## Release Evidence Commands

The `assemblywright` CLI is read-only. Each subcommand prefers a configured IPC
endpoint and falls back to local metadata or local file and report inspection.

```sh
cargo run -p assemblywright-cli -- release readiness
```

```sh
cargo run -p assemblywright-cli -- release evidence-status
```

```sh
cargo run -p assemblywright-cli -- release signed-distribution-runbook
```

```sh
cargo run -p assemblywright-cli -- release live-device-runbook
```

```sh
cargo run -p assemblywright-cli -- release evidence-bundle-runbook
```

Add `--json` for the exact structured payload, or `--all-commands` on
`readiness` for the full readable runbook.

## Live Closeouts

These are owner-controlled and default-off. They are not part of the local
gate and are recorded as external evidence.

```sh
./scripts/mac-windows-bridge-live-e2e.sh --check
```

```sh
./scripts/mac-windows-bridge-live-e2e.sh --run
```

```sh
./scripts/mac-windows-bridge-live-e2e.sh --run-fixture
```

```sh
./scripts/mac-windows-bridge-live-e2e.sh --run-mlx
```

The fixture closeout binds a synthetic echo job to exact event sequences over
the authenticated Windows loopback control plane. The MLX closeout binds one
real local completion and a pause-dominated cancellation. Neither is
model-quality, OS-sandbox, repository, Git, unattended, signing, notarization,
or release evidence.

The base `--run` lane additionally requires the signed helper monitor and the
production app lifecycle to receive and strictly decode the schema-v8 Feature
Conveyor snapshot over the accepted MacBridge session. This proves live
read-only observation only; it grants no queue mutation or owner-action
authority.

## Release Evidence Boundary

Passing `./scripts/release-local.sh` proves the Rust workspace builds, passes
standard and ignored release-proof tests, packages the crates, runs local
release and runbook preflights plus fake-fixture evidence self-tests, produces
a valid unsigned distribution layout, and passes the Swift build and test gate.

Deterministic cross-process coverage proves:

- The protocol seam from Mac capability advertisement through master
  acceptance, leased job, exact result acceptance, and wrong-lease rejection.
- Durable master lifecycle: success, duplicate and wrong-lease denial,
  cancellation, expiry, capability-specific bounds, restart abandonment,
  late-result rejection, and safe reissue.
- A real master process and fixture worker: one-owner database exclusion,
  bearer non-disclosure, unauthorized and oversized-body denial, authenticated
  loopback health and job completion, and restart reconciliation.
- The Feature Conveyor kernel: approved specification revisions, queue capacity
  and ordering, dependency blocking, compare-and-set revisions, singleton
  active lease, exact lifecycle advancement, cancellation without advancement,
  explicit abandonment, startup quarantine, and same-transaction redacted
  audits. Its observation proof covers the owner-token-authenticated
  loopback-only `GET /v1/feature-conveyor/status`, empty state, deterministic
  current-lifecycle counts and ordering, exclusion of terminal history, the
  100-entry cap with explicit truncation, exact JSON-key allowlists, and
  unchanged local owner boundary. Kernel coverage proves
  the fixed-enum owner-guidance precedence for idle, ready,
  dependency-blocked, active, reconciliation-required, and Emergency Pause
  states, including queue/lifecycle/pause revision binding and malformed-state
  rejection. The real-process E2E proves authenticated serialization,
  boundedness, redaction, and the idle and reconciliation-required states; the
  Windows remote-mTLS E2E proves pre-handshake denial, MacBridge-only success,
  non-MacBridge denial, and exact bounded/redacted allowlists for the dedicated
  `/v1/distributed/feature-conveyor/status` route. Schema-v8 coverage additionally
  proves nullable/default-deny owner designation, compare-and-set rebinding,
  fixture/non-designated/revoked denial, migration, and atomic redacted audit.
  Repository-grant coverage proves authenticated loopback-only strict requests,
  contiguous compare-and-set revisions, digest-only current projection, expiry,
  pause-bound active-grant denial, revocation while paused, redaction, audit
  rollback, and absence from the enrolled-device router.
  Repository-preflight coverage proves canonical scope-digest validation,
  exact active registration-grant and Emergency Pause binding, bounded
  filesystem-only identity observation against a disposable standard
  repository, hostile filter-configuration non-execution, and
  UNC/device/non-fixed-volume/reparse/worktree/submodule/detached/wrong-branch/
  wrong-HEAD/symlink/non-Git denial. It also proves fixed redacted failures,
  atomic path-free audit, owner authentication, request bounds, and absence
  from the enrolled-device router. It does not claim clean-tree or content
  validation. This is native process E2E; no browser or Playwright surface is
  involved.
  Schema-v10 coding-dispatch coverage proves strict path-free protocol framing,
  exact capability/device registration including field drift, zero and maximum
  numeric authority bindings, malformed result/status/digest/mutation denial,
  feature/specification/lifecycle lease,
  snapshot ID/digest, queue and Emergency Pause binding, atomic queued-step/
  immutable-row/event/audit creation, audit rollback, owner authentication,
  enrolled-device route absence, cancellation dominance, lifecycle blocking,
  and restart quarantine. Snapshot-transfer coverage additionally proves exact
  post-lease authorization around bounded filesystem reads, strict sequential
  chunk identity/digests, and a deterministic bounded raw-object bundle. Swift
  fake-session coverage proves strict bridge ordering and cancellation, while
  separate real-process UDS coverage proves native-agent admission and cleanup;
  agent library coverage reconstructs and validates an independent private
  no-remote shallow Git repository. These are three bounded native seams, not a
  combined live Windows-to-Mac transfer proof. They do not prove signed-helper
  deployment, retained coding runtime containment, mutation, or integration.
  The designated-owner POST is bound to the exact queue, designation, and pause
  revisions and reuses the manifest, grants, dependency, capacity, immutable-
  specification, and atomic-enqueue checks without claiming a lease. Swift tests
  prove strict schema-v8 decoding, request ordering, exact digest shapes,
  self-dependency rejection, server-authoritative canonical-digest handling,
  redacted-receipt validation, fail-closed cancellation, authenticated snapshot
  propagation, and read-only app presentation. The display labels do not
  establish claimability or callable owner authority.
- Enrollment identity: digest-only grants, signed-CSR issuance, expiry and
  replay denial, rotation, revocation, schema migration, two-phase pending
  capability rebind with replacement-key acknowledgement verification,
  CA-signed activation verification, exact lost-output retry, Emergency Pause,
  immutable redacted audit rollback, stale/replay/expiry preservation, real
  Windows DPAPI round trips, and the CLI stdin boundary.
- Remote mTLS: mutual certificate authentication, durable certificate and
  device checks, pre-handshake health denial, exporter-bound replay denial,
  reconnect epoch advance, socket-close reconciliation, and revoked-certificate
  denial.
- The event cursor: bounded paging, durable resume, stream-mismatch and
  future-cursor rejection, metadata redaction, and disconnect/requeue events
  after restart.
- The Mac agent: default-off bounded execution, cancellation, late-output
  suppression, bearer and identity checks, and cursor confinement, proven
  cross-process on macOS.
- The local transport: bounded framing, peer code-identity validation before
  request parsing, current-EUID checks, method whitelisting, EOF requirements,
  owner-only filesystem setup, and validated leaf cleanup.

It does not prove Developer ID signing, notarization, stapling, clean-profile
installation, Finder or LaunchServices behavior, live cross-device reliability,
autonomous dispatch, repository mutation, publication authority, or unattended
operation. Those remain owner-recorded external evidence.

## Owner-Confirmed Standard Capability Rebind

The rebind is a four-receipt stdin ceremony, not one pipeline: keep each
Windows process stopped/single-owner and pass only the emitted public document
to the next command. Never place a CSR, acknowledgement, certificate, or raw
grant secret in argv or a shell history.

```text
Windows: assemblywright-master ... enrollment rebind-pair --device-id UUID \
  --capabilities-file mlx-capability.json --master-endpoint IP:PORT --confirm
Mac:     assemblywright-mac-bridge enrollment rebind prepare --confirm
Mac:     assemblywright-mac-bridge enrollment rebind stage --confirm
Windows: assemblywright-master ... enrollment rebind-activate \
  --acknowledgement-stdin --confirm
Mac:     assemblywright-mac-bridge enrollment rebind promote --confirm
```

Mac `enrollment rebind cancel --confirm` is destructive only after `prepare`
and before `stage` has persisted a certificate/acknowledgement. If abandoning
in that window after Windows issuance, confirm Windows
`enrollment rebind-abort --grant-id UUID --confirm` first, then cancel the Mac
prepare-only stage. Once Mac `stage` emits the signed acknowledgement, local
cancel refuses and preserves the replacement key, certificate, and staged
receipt because Windows activation may already have committed; retry the exact
Windows activation to recover its receipt and promote. After promotion, Mac
cancellation recognizes the installed replacement generation and cannot delete
its selected key or certificate. Repository tests
do not prove the Xcode-provisioned Secure Enclave path, live Windows DPAPI CLI,
cross-device promotion, reconnect under the higher revision, or MLX execution.
