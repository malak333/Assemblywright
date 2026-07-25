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

The portable distributed foundation has a separate Windows gate. For a fresh
Windows checkout, install the MSVC Rust toolchain pinned by
`rust-toolchain.toml` after the Visual Studio C++ Build Tools and Windows SDK
are present:

```powershell
rustup toolchain install 1.95.0 --profile minimal --component clippy --component rustfmt
```

`.github/workflows/windows-protocol.yml` runs formatting, clippy, the protocol
and master crates, the master-process E2E, and the elevated Windows SCM service
lifecycle E2E.

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
| Feature Conveyor kernel | `cargo test -p assemblywright-master --test feature_conveyor_kernel` |
| Master process E2E | `cargo test -p assemblywright-master --test master_process_e2e` |
| Enrollment and identity | `cargo test -p assemblywright-master --test enrollment_identity_e2e` |
| Remote mTLS | `cargo test -p assemblywright-master --test remote_mtls_e2e` |
| Event cursor | `cargo test -p assemblywright-master --test event_cursor_e2e` |
| Windows service lifecycle | `cargo test -p assemblywright-master --test windows_service_lifecycle_e2e -- --ignored` |
| Mac agent relay | `cargo test -p assemblywright-agent --test local_relay_e2e` |
| Local transport and release | `cargo test -p assemblywright-core` |
| CLI naming contract E2E | `cargo test -p assemblywright-cli --test naming_contract_e2e` |
| Swift package | `swift test --disable-sandbox --package-path apps/mac` |
| One Swift test | `swift test --disable-sandbox --package-path apps/mac --filter <test>` |
| Codex workflow | `./scripts/validate-codex-workflow.sh` |
| Docs contract | `./scripts/release-docs-drift-smoke.sh` |
| Naming contract | `./scripts/release-naming-contract-smoke.sh --check` |
| Shell portability | `./scripts/release-shell-portability-smoke.sh --check` |

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
  audits.
- Enrollment identity: digest-only grants, signed-CSR issuance, expiry and
  replay denial, rotation, revocation, schema migration, real Windows DPAPI
  round trips, and the CLI stdin boundary.
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
