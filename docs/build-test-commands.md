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
./scripts/repository-gate-proof-controller.sh --check
./scripts/repository-gate-proof-controller.sh --self-test
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

## Repository-Gate Proof Controller

Validate the controller contract and its disposable native Git/process
regression matrix with:

```sh
./scripts/repository-gate-proof-controller.sh --check
./scripts/repository-gate-proof-controller.sh --self-test
```

After the implementation is committed to `main`, fetched as
`refs/remotes/origin/main`, and the checkout is clean and exactly equal to that
ref, the owner may create the local proof receipt with:

```sh
./scripts/repository-gate-proof-controller.sh --run
```

`--run` accepts no alternate command or repository. It runs only the exact
committed `scripts/release-local.sh` bytes via argument-free Bash stdin,
requires stable pre/post HEAD, tree, origin and
status, and atomically writes an owner-only, path-free bounded JSON receipt plus
raw SHA-256 sidecar under `target/repository-gate-proof/`. The receipt is local
repository-gate evidence only. It is not posted or admitted automatically and
does not prove Windows activation, signing, notarization, live-device behavior,
restricted-worker execution, a selected review provider, GitHub publication,
restart recovery, Mac/Windows control streaming, or production readiness.
Handled cancellation and every rejected or failed run leave no receipt.
The controller rejects a symlinked or non-directory `target` before touching
external state. After validating the fixed owner-matched directory chain, a new
run invalidates only the prior fixed receipt and sidecar so a failed rerun
cannot be mistaken for a new pass.
Git observations discard inherited Git environment/config/redirection,
disable replace objects, and remain fixed to the controller root. The
self-test additionally covers committed-byte execution, hostile Git/internal-
marker environment, group/world-writable target rejection, concurrent
target/output directory replacement, held-directory cleanup, and TERM of a
gate descendant process group with bounded drain and no late sentinel. It
allows a bounded natural process-group drain after the gate leader exits, but
fails the run and force-cleans the group when a descendant persists.
That grace period is success-only: a failed gate terminates its complete group
immediately, and the self-test proves no delayed side effect survives.
It also rejects assume-unchanged, skip-worktree, or any other non-`H` tracked
index tag. Pre/post stability is edge detection for concurrent same-UID changes
to other gate-consumed files, not host-isolation proof.
The self-test exercises the default, unknown, and extra-argument CLI shapes and
proves that a rejected rerun removes the prior fixed receipt pair instead of
leaving stale evidence that appears current.

## Restricted-worker Live Proof Controller

Static prerequisites and the requirements-derived disposable Git/process suite
run inside the canonical local gate:

```sh
./scripts/restricted-worker-proof-controller.sh --check
./scripts/restricted-worker-proof-controller.sh --self-test
./scripts/review-provider-proof-controller.sh --check
./scripts/review-provider-proof-controller.sh --self-test
```

After this feature is committed to clean published `main`, the Windows checkout
is fast-forwarded to the same commit, schema-v19 service health is confirmed,
and the signed Mac helper remains current, set the exact current owner-control
designation revision and start the owner-supervised proof:

```sh
ASSEMBLYWRIGHT_FEATURE_CONVEYOR_OWNER_CONTROL_DESIGNATION_REVISION=<revision> \
  ./scripts/restricted-worker-proof-controller.sh --run
```

The command prints only fixed Windows-local actions. Run each against the
already-authenticated Windows session with the exact committed
`scripts/windows-local-coding-live-control.ps1`, then paste its single sanitized
JSON receipt into the controller. On an interactive terminal the controller
temporarily disables canonical input and echo only while reading each bounded
receipt, so receipts larger than macOS `MAX_CANON` are accepted; it restores the
exact prior terminal state before continuing, failing, or handling a signal.
Input storage remains capped at 8,192 bytes; an oversized line is drained
through its newline and rejected before the controller exits.
Coordination receipts are read on a separate
descriptor from the committed Mac harness bytes. A complete pass atomically
writes `target/restricted-worker-live-proof/restricted-worker-live-proof.json`
and its raw SHA-256 sidecar. The receipt is path-free and binds the exact
published HEAD/tree, the committed Mac and Windows controller definitions, and
the digest of the complete validated private live transcript. The transcript itself is
removed. `--run` does not admit the receipt or activate the conveyor.

If Prepare is interrupted after creating the disposable marker or any subset of
the three grants, rerun the same controller and repeat the emitted Prepare
action. The marker is flushed, byte-validated, and atomically renamed into place
before grants. Windows resumes only the marker-bound exact state and reconstructs the
same approved request. If the signed-helper enqueue committed but its receipt
was lost, that rerun recognizes only the sole exact queued revision-1 feature
at the marker baseline plus one and skips a duplicate enqueue. Any other state
fails closed; use the explicit Cleanup action only after the queue is empty.
An interrupted clone before marker creation is removed on retry only if it is
an exact clean normal-index clone of the fixed source with that source as its
sole origin and the queue is still empty.

The self-test covers the fixed CLI, committed-byte/receipt-descriptor split,
strict terminal record and revision/digest validation, dirty/wrong-branch/
origin/status drift, hidden-index rejection, environment isolation,
stale-receipt invalidation, malformed or duplicate
success/action markers, complete-transcript digest sensitivity, process failure,
cancellation and descendant reaping, hostile/swap/
writable target rejection, atomic publication failure, permissions, redaction,
and failure-without-output. The functional native two-device proof still runs
the signed Swift relay, real Rust agent, Keychain `local-coding` identity, mTLS,
Windows owner-loopback actions, SQLite-backed queue/artifact/candidate state,
and cleanup. Playwright, screenshots, visual regression, and cross-browser
testing do not apply to this native boundary.

This receipt proves only the restricted-worker category. It is not evidence of
a host sandbox or OS-wide egress control, review-provider quality, GitHub
publication, restart recovery, Mac/Windows owner-control streaming, signing
distribution, notarization, clean-profile install, or production readiness.

## Review-provider Live Proof Controller

Static validation and disposable native Git/process regressions run in the
canonical local gate:

```sh
./scripts/review-provider-live-e2e.sh --check
./scripts/review-provider-proof-controller.sh --check
./scripts/review-provider-proof-controller.sh --self-test
```

On exact clean Windows `main == origin/main`, provision the fixed pinned adapter
and restart the service from the authenticated Administrator session:

```powershell
& .\scripts\windows-review-provider-live-control.ps1 -Action Provision -ConfirmAction
```

After Mac and Windows equal the exact published commit, start the Mac controller:

```sh
./scripts/review-provider-proof-controller.sh --run
```

When it emits the single Windows action marker, run and return the one sanitized
JSON line:

```powershell
& .\scripts\windows-review-provider-live-control.ps1 -Action Run -ConfirmAction
```

The flow is fixed to `openai.codex`, `gpt-5.6-sol`, and Codex `0.148.0`. It
uses the production schema-v16 provider process/Job Object path and requires a
known-good approval plus a known-bad rejection. A pass writes the owner-private
`target/review-provider-live-proof/review-provider-live-proof.json` and raw
SHA-256 sidecar. Provider output and credentials are absent. The receipt is not
admitted automatically and is not evidence of general review competence, an
actual queued-feature gateway lifecycle, GitHub publication, signing,
notarization, or production readiness. Playwright does not apply to this native
Rust/process/PowerShell boundary.

Run the canonical local gate before the live `--run` lane. The proof pair is an
ignored owner-private artifact under `target/`, so terminal success alone does
not establish that it remains retained after later build, packaging, or release
activity. Before closeout, recheck that both files are owner-only mode `0600`
and single-link regular files whose raw sidecar equals the receipt SHA-256. The proof
directory must also remain ordinary, owner-owned, mode `0700`, non-symlink,
and canonical. If the directory or either file is absent or invalid, rerun the
exact clean published proof controller. Do not reconstruct a missing proof pair
from terminal output.

## Windows Distributed Gate

The schema-v9 snapshot-claim, schema-v10 coding-dispatch, schema-v11 owner-resolution,
schema-v13 result-artifact admission, schema-v14 artifact integration/candidate
freezing, schema-v15 validation-gate persistence/contracts, and ephemeral
snapshot-transfer/materialization slices have focused portable coverage:

```sh
cargo test -p assemblywright-protocol --test protocol_contract repository_snapshot_claim_contract_is_strict_exact_and_path_free_on_receipt
cargo test -p assemblywright-master --lib snapshot::tests
cargo test -p assemblywright-master --test feature_conveyor_kernel
cargo test -p assemblywright-master --test feature_conveyor_kernel artifact -- --nocapture
cargo test -p assemblywright-master --test master_process_e2e repository_preflight_is_owner_only_filesystem_identity_observation_and_redacted
cargo test -p assemblywright-master --test master_process_e2e repository_snapshot_claim_is_authenticated_path_free_and_durable
cargo test -p assemblywright-master --bin assemblywright-master snapshot_claim_reservation_survives_blocking_task_timeout
cargo test -p assemblywright-protocol --test local_coding_contract
cargo test -p assemblywright-protocol --test artifact_integration_contract
cargo test -p assemblywright-protocol --test owner_resolution_contract
cargo test -p assemblywright-master --test feature_conveyor_kernel coding_dispatch
cargo test -p assemblywright-master --test feature_conveyor_kernel owner_resolution
cargo test -p assemblywright-master --test feature_conveyor_kernel master_process_v10
cargo test -p assemblywright-master --test master_process_e2e owner_resolution_routes_are_authenticated_strict_cas_bound_and_redacted -- --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel emergency_pause_cancels_coding_attempt_and_resume_rejects_pre_pause_acknowledgement
cargo test -p assemblywright-master --test feature_conveyor_kernel terminal_coding_ack_allows_validation_and_lifecycle_change_invalidates_replay
cargo test -p assemblywright-master --test feature_conveyor_kernel result_artifact_admission_is_exact_idempotent_and_required_before_result
cargo test -p assemblywright-master --test feature_conveyor_kernel artifact_store_exact_retry_and_startup_orphan_cleanup_fail_closed
cargo test -p assemblywright-master --test feature_conveyor_kernel artifact_integration -- --nocapture
cargo test -p assemblywright-master --test artifact_integration_e2e -- --nocapture
cargo test -p assemblywright-master --test master_process_e2e artifact_integration -- --nocapture
cargo test -p assemblywright-protocol --test validation_gate_contract -- --nocapture
cargo test -p assemblywright-protocol --test review_gateway_contract -- --nocapture
cargo test -p assemblywright-protocol --test publication_coordinator_contract -- --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel artifact_integration_and_validation_gate_freeze_candidate_advance_and_reject_drift -- --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel validation_gate -- --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel review_ -- --nocapture
cargo test -p assemblywright-master --test review_provider_e2e -- --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel publication_ -- --nocapture
cargo test -p assemblywright-master --test master_process_e2e artifact_integration_routes_are_owner_loopback_only_strict_and_redacted -- --nocapture
cargo test -p assemblywright-agent --test local_coding_admission
cargo test -p assemblywright-agent snapshot::tests
cargo test -p assemblywright-agent --test local_relay_e2e authenticated_uds_local_coding_snapshot_admission_cancellation_and_restart_cleanup -- --nocapture
cargo test -p assemblywright-master --test remote_mtls_e2e remote_local_coding_dispatch_is_exporter_bound_exact_and_pause_dominant -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter DeveloperBridgeTests
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows-local-coding-live-control.ps1 -Action Check
./scripts/mac-windows-bridge-live-e2e.sh --run-local-coding
```

The schema-v15 tests above prove strict ordered-plan decoding, canonical
request binding, immutable digest-only attempt/evidence rows, exact candidate
binding, owner-only route admission, remote-route absence, and the all-pass
`validating -> reviewing` kernel transition. The portable runner contract also
proves deterministic requirements/path/documentation/knowledge/safety/secret
checks. An unprovisioned runner returns `validation_runner_unavailable` before
creating an attempt.

The schema-v16 review tests prove strict packet/result decoding; exact
specification, commit/diff, evidence, provider/model, grant, lifecycle, queue,
and pause binding; strict deny-unknown-fields review-safe DTO and sensitive-
context rejection; polarity-preserving patch
extraction; exact master-derived requirement coverage; packet-only evidence
references; default-unavailable production configuration; one fresh
cleared-environment bounded provider process per call; Unix native descendant
termination and executable-replacement rejection; legacy sensitive-context revalidation;
exact-minimum capability admission and rejection of every provider/model,
input/output, structured-output, response-only, fresh-session, tokenization,
and token-ceiling drift; pre-call and post-response cancellation suppression;
malformed/outage/incomplete failure without a
repair charge; fixed backoff; three-calls-per-candidate and twelve-per-feature
ceilings; interruption and post-response-drift terminalization/quarantine; rejection retention;
exact approval-only `reviewing -> publishing`;
idempotency; and owner-loopback/remote-route separation. They do not prove a
live selected provider, provider competence, Windows service deployment, or
publication. Windows native proof must additionally cover the verified image-
handle lock, gate-before-provider-spawn Job assignment, and descendant reaping.

The schema-v17 publication tests prove strict path-free admission, exact
approval/candidate/spec/evidence/provider/grant/queue/pause/remote-base/branch-
policy binding, immutable seven-action intent ordering, completed idempotence,
stage-specific self-bound remote/PR/check/protection/merge/gate evidence,
in-flight cancellation and concurrent Emergency Pause dominance, pause and
restart quarantine, merge-intent abandonment protection, exact remote-main
equality, fixed post-merge gate,
and atomic success/lease/queue advancement. A native controlled bare-Git remote
proves process and Git-ref sequencing only. Production remains unavailable
before intent without a fixed credential-owning adapter; no live GitHub API,
credential custody, required hosted checks, branch protection, or merge policy
is claimed.

The schema-v19 orchestration and owner-activation kernel is covered by the native conveyor suite:

```sh
cargo test -p assemblywright-master --test feature_conveyor_kernel orchestration -- --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel substantive_validation_failure -- --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel provider_backoff -- --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel activation_ -- --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel owner_pause -- --nocapture
cargo test -p assemblywright-protocol --test owner_activation_contract
```

These tests prove default-inert behavior, stale CAS and exact idempotence,
24-hour budget enforcement, substantive failure classification, no fabricated
replacement candidate, provider-pause time exclusion, and restart-safe resume
of one effect-free checkpoint. They also prove Emergency Pause excludes all
paused wall time and both immutable review-call ceilings require owner
attention. The owner-token process test proves strict, redacted, authenticated
evidence admission. The Windows-only remote-mTLS E2E proves pre-handshake,
wrong-role, non-designated, stale, malformed, and positive designated-bridge
owner-control behavior over the real TLS/exporter-bound route. Live activation
still requires six genuine proof-controller receipts; tests must never invent
those receipts to claim deployment readiness.

On the owner-controlled Windows validation host, the connected containment
runner and its hostile fixture boundary have native coverage:

```powershell
cargo test -p assemblywright-master --test windows_validation_containment_e2e -- --nocapture
cargo test -p assemblywright-master --test validation_runner_contract -- --nocapture
cargo test -p assemblywright-master --test windows_validation_containment_e2e appcontainer_sid_cannot_read_or_write_outside_the_granted_execution_root -- --ignored --nocapture
cargo test -p assemblywright-master --test windows_validation_containment_e2e zero_capability_appcontainer_cannot_open_a_loopback_network_connection -- --ignored --nocapture
```

This proves fixed command selection, design-digest/changed-path requirements
binding, an exact llvm-cov argv contract that emits a summary and requests the
protocol-owned 70% minimum line threshold, scratch verification and cleanup, the
restricted token/AppContainer, minimal environment, bounded output, active
cancellation with Job Object tree reaping, one outside-root denial, and
loopback TCP/UDP nondelivery. Live activation additionally requires a private
runner bundle at `<data-dir>/validation-runner/toolchain` containing Cargo,
rustc/rustfmt, clippy/fmt, and `cargo-llvm-cov`, plus a credential-free
`dependency-cache-seed`. Current proof does not establish installed-service
identity, a real populated bundle/cache, signed Mac E2E, credential-store
denial, actual above/below-threshold llvm-cov behavior, or OS-wide egress
enforcement.

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
| Windows remote mTLS observer, designated-owner enqueue, owner-control projection, activation, and denial/success | `cargo test -p assemblywright-master --test remote_mtls_e2e remote_listener_requires_enrollment_tls13_and_channel_bound_identity -- --nocapture` |
| Swift strict Feature Conveyor observer, one-shot owner action, and helper lifecycle | `swift test --disable-sandbox --package-path apps/mac --filter DeveloperBridgeTests` |
| Enrollment, two-phase capability rebind, and identity | `cargo test -p assemblywright-master --test enrollment_identity_e2e` |
| Remote mTLS | `cargo test -p assemblywright-master --test remote_mtls_e2e` |
| Windows snapshot-bound coding dispatch and bounded transfer mTLS/process E2E | `cargo test -p assemblywright-master --test remote_mtls_e2e remote_local_coding_dispatch_is_exporter_bound_exact_and_pause_dominant -- --nocapture` |
| Master-owned artifact integration and exact candidate Git boundary | `cargo test -p assemblywright-master --test artifact_integration_e2e -- --nocapture` |
| Event cursor | `cargo test -p assemblywright-master --test event_cursor_e2e` |
| Windows service lifecycle | `cargo test -p assemblywright-master --test windows_service_lifecycle_e2e -- --ignored` |
| Mac agent relay | `cargo test -p assemblywright-agent --test local_relay_e2e` |
| Native metadata-only coding admission | `cargo test -p assemblywright-agent --test local_coding_admission` |
| Native ephemeral snapshot, descriptor-relative general coding, retained-attempt restart/tamper/expiry, cancellation, and cleanup | `cargo test -p assemblywright-agent snapshot::tests` |
| Production Swift relay to supervised Rust-agent general-coding success plus final-verification cancellation/restart cleanup E2E | `./scripts/mac-local-coding-snapshot-e2e.sh` |
| Windows live-controller immutable Git-blob/CRLF unit regression | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows-local-coding-live-control.ps1 -Action Check` |
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

```sh
ASSEMBLYWRIGHT_FEATURE_CONVEYOR_OWNER_CONTROL_DESIGNATION_REVISION=<exact-current-revision> \
  ./scripts/mac-windows-bridge-live-e2e.sh --run-local-coding
```

The fixture closeout binds a synthetic echo job to exact event sequences over
the authenticated Windows loopback control plane. The MLX closeout binds one
real local completion and a pause-dominated cancellation. Neither is
model-quality, OS-sandbox, repository, Git, unattended, signing, notarization,
or release evidence.

The base `--run` lane additionally requires the signed helper monitor and the
production app lifecycle to receive and strictly decode the schema-v9 Feature
Conveyor snapshot over the accepted MacBridge session. This proves live
read-only observation only; it grants no queue mutation or owner-action
authority.

The `--run-local-coding` lane uses the separately enrolled local-coding
identity, production signed Swift relay, real Rust agent, and Windows-local
owner controller. It binds a disposable repository snapshot to the exact
feature, worker registration, queue/lifecycle/pause revisions, task, step,
attempt, and work packet. A terminal success proves the protocol-v5 result was
accepted only after its exact schema-v13 artifact admission. The lane then
invokes the owner-loopback schema-v14 integration action, verifies the frozen
candidate commit and tree are detached, clean, fsck-valid, and remote-free,
proves an exact integration retry is idempotent, and confirms the registered
source checkout remains clean. It also proves the private retained-state shape:
one `.sealed` workspace plus a
filename-matched `.retention.json` recovery record after success. The live lane
does not parse that private record or claim its semantic bindings. The
disposable harness then removes only that validated shape from its harness-owned temporary root; native relay
tests separately prove product restart recovery, exact cancellation, expiry,
tamper/orphan rejection, and cleanup. The lane finally proves owner
cancellation, safe abandonment, empty queue/lease/attempt and Windows transfer
state, revocation of all temporary grants, and removal of the marker-bound
disposable checkout. It is functional native two-device artifact-integration
evidence, not test-gate, review, publication, registered-source mutation,
Developer ID distribution, notarization, or clean-profile release evidence.

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
  prove strict schema-v9 decoding, request ordering, exact digest shapes,
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

## Protocol v5 general-worker validation

```bash
cargo test -p assemblywright-protocol --test local_coding_contract
cargo test -p assemblywright-master --test feature_conveyor_kernel master_process_v12 -- --nocapture
cargo test -p assemblywright-agent --test local_relay_e2e authenticated_uds_local_coding_snapshot_admission_cancellation_and_restart_cleanup -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter DeveloperBridgeTests
./scripts/mac-local-coding-snapshot-e2e.sh
```

These are repository and native-boundary checks, not Windows deployment, signing, notarization, live-device, or release proof.
