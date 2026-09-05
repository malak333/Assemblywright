# Build And Test Commands

## Local Model Selection Focused Validation

```bash
cargo test -p assemblywright-protocol --test local_model_selection_contract -- --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel local_model_selection -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter LocalModelSelection
swift test --disable-sandbox --package-path apps/mac --filter localModelTerminalRejectionCannotResume
swift test --disable-sandbox --package-path apps/mac --filter modelsPresentationIsBounded
```

Windows-only native mTLS coverage is:

```powershell
cargo test -p assemblywright-master --test remote_mtls_e2e remote_model_selection_rejects_old_profile_and_reconciles_exact_target_profile -- --nocapture
```

It proves the real TLS/application-session route, same-session old-profile
rejection, and exact target-profile reconciliation on Windows. Mac source
validation cannot substitute for that Windows execution, signed-helper
packaging, notarization, clean-profile installation, or live MLX model launch.

Run commands from the repository root unless noted otherwise.

## Full-Machine Assembly Line Target Validation

The approved target contract is documented in
`docs/full-machine-assembly-line-design.md`. Protocol v5 now includes strict
full-machine Assembly Line schema-v1 contracts, the Windows master is schema v22 with
planning persistence/routes plus a fail-closed durable activation controller, and the Mac has strict frozen review/approval transport
plus restart-safe reconciliation in the simplified UI. Source includes catalog-bound
provider/GitHub process adapters. Source now has bounded native LocalSystem/AppContainer
service proof when its fixed runtime is provisioned exactly; production configuration,
real-provider behavior, and execution activation remain separate unproved boundaries.

Full-machine target phase: planning/creation containment has bounded native Windows LocalSystem/AppContainer service proof; execution admission is implemented and effects remain unavailable.

```sh
cargo test -p assemblywright-protocol --test full_machine_assembly_line_contract
cargo test -p assemblywright-master --test assembly_line_planning
cargo test -p assemblywright-master --test assembly_line_execution_control
cargo test -p assemblywright-master --test assembly_line_execution_http
cargo test -p assemblywright-broker --all-targets
cargo test -p assemblywright-executor --all-targets
cargo test -p assemblywright-master planning_effects::tests
cargo test -p assemblywright-master --test assembly_line_planning_http
swift test --disable-sandbox --package-path apps/mac --filter DeveloperBridgeTests
swift test --disable-sandbox --package-path apps/mac --filter AssemblywrightMacAppTests
swift test --disable-sandbox --package-path apps/mac --filter assemblyLineStartingRequiresAvailableExecutionRuntime
./scripts/release-docs-drift-smoke.sh
git diff --check
```

The schema-v19-to-v22 backup-first migration and legacy-table preservation test can be
run directly with:

```sh
cargo test -p assemblywright-master --test assembly_line_planning \
  schema_v19_file_upgrade_is_backup_first_and_preserves_legacy_tables -- --exact
```

On a native Windows toolchain, the designated-Mac mTLS planning boundary is:

```powershell
cargo test -p assemblywright-master --test assembly_line_planning_mtls
```

The contained planning source slice is covered by:

```sh
cargo test -p assemblywright-protocol --test full_machine_assembly_line_contract
cargo test -p assemblywright-master --lib planning_effects::tests -- --nocapture
cargo test -p assemblywright-master --test brainstorming_provider_adapter_e2e -- --nocapture
cargo test -p assemblywright-master --test assembly_line_planning_http -- --nocapture
swift test --disable-sandbox --package-path apps/mac
```

These planning tests bind the sandbox split to the source contract: brainstorming is
tool-free and planning-only; Windows uses Codex inner `danger-full-access` inside the
Assemblywright restricted-token AppContainer, exact ACL tree, closed environment,
pinned executable, and kill-on-close Job, while non-Windows uses inner `read-only`.
The Windows inner value is not repository-write, implementation, or host authority.
Windows native planning proof must also show that schema v4 derives the package root
from canonical Common Application Data, keeps the locator/config master-private under
`DataDir`, preserves unrelated shared-parent ACLs, and rejects missing, broadened, or
identity-drifted profile traversal before an effect. The deterministic fixed profile SID
must match before idempotent registration for the current Windows identity; owner-time
registration does not establish LocalSystem registration. A launch using
`STARTF_USESTDHANDLES` must inherit valid stdin, stdout, and stderr handles and drain
stderr concurrently without retaining its content.

Run the synthetic LocalSystem/AppContainer planning boundary from an elevated native
Windows PowerShell in a disposable checkout:

```powershell
cargo test -p assemblywright-master `
  profile_registration_is_idempotent_for_the_current_windows_identity -- --nocapture
cargo test -p assemblywright-master --test windows_planning_private_desktop_contract
cargo test -p assemblywright-master --test windows_planning_service_e2e_contract
cargo build -p assemblywright-master --bin assemblywright-master `
  --bin assemblywright-brainstorming-provider --release

& .\scripts\windows-planning-service-e2e.ps1 `
  -MasterExe .\target\release\assemblywright-master.exe `
  -ProviderExe .\target\release\assemblywright-brainstorming-provider.exe `
  -OutputSchema .\crates\assemblywright-master\resources\brainstorming-output-schema.json `
  -ServiceName AssemblywrightPlanningE2E_Local `
  -Port <unused-port> `
  -Confirm
```

The native E2E installs one exact disposable LocalSystem master, uses the synthetic
Codex fixture in the fixed provider AppContainer, drains 32 KiB of fixture stderr,
checks the planning response, and removes only its exact service and data roots. It
asserts that the production service is untouched. This proves the synthetic native
service boundary, not real Codex authentication, network/model quality, GitHub effects,
production planning configuration, or deployed execution authority.

After the exact production runtime is provisioned, an elevated owner may run the
real-Codex diagnostic without creating a draft, approval, repository, or queue entry:

```powershell
cargo test -p assemblywright-master --test windows_planning_real_codex_probe_contract

& .\scripts\windows-planning-real-codex-probe.ps1 `
  -MasterExe C:\exact\installed\assemblywright-master.exe `
  -DataDir C:\exact\master\data `
  -ServiceName AssemblywrightMaster `
  -Confirm
```

The script stops the exact installed service, and the native command independently
requires the service to be fully stopped and bound to that same executable and data
directory. It then revalidates schema-v4 runtime hashes, ACLs, profile SID, and private
roots; runs exact `codex login status`; and only on success runs one fixed Public,
tool-disabled structured request through the same restricted-token AppContainer,
private desktop, inherited handles, closed five-variable Codex environment, and
kill-on-close Job. Its four supplied path values use the provider adapter's exact
Windows local-drive formatting: only `\\?\C:\...`-form local-drive prefixes are
removed; UNC and all other namespaces remain unchanged. Before each process and after
it exits, the full runtime and profile boundary are revalidated. While the child runs,
the 25 ms authority probe checks that the SCM service remains stopped with the exact
configuration and current token identity and that the durable pause revision remains
unchanged; drift terminates the Job. It does not repeatedly walk provider-owned
writable state while Codex is legitimately updating that state. The health endpoint is
derived and validated from the exact SCM `--bind` before stopping, never caller supplied. The script restarts the service and verifies that preflight-bound health endpoint in `finally` when it
was initially running, even when the probe emits no receipt. Its receipt contains only fixed outcome categories, numeric
exit/diagnostic codes, hashes, and exact binding identities. It never includes process
text, prompts, paths, tokens, or credentials. A successful receipt is live diagnostic
evidence for that exact run only; it is not GitHub-effect, production availability,
signed-release, or owner-usability proof.

Before launch, authority-open rejection exposes only one fixed nonsecret cluster code:
`native_probe_service_configuration`, `native_probe_executable_command_binding`,
`native_probe_account_identity`, `native_probe_profile_revalidation`,
`native_probe_stopped_state`, or `native_probe_binding_digest`. These codes never
include the rejected SCM value, identity, path, command, or process content. Later
continuous-authority drift remains the generic `native_probe_authority` boundary.

The unit suite covers exact login-status empty-output admission, closed numeric stderr
classification, zeroization, command allowlisting, path-namespace boundaries, service
configuration and identity drift, cancellation, and post-command authority dominance.
The contract test covers the owner-confirmed stop/probe/restart/health sequence. The
native script is the real Windows process/service E2E; Playwright is inapplicable to
this non-browser boundary.

The inert Phase-4 containment source slice is covered by native Rust contracts and
process tests (not Playwright):

```sh
cargo test -p assemblywright-protocol --test execution_containment_contract -- --nocapture
cargo test -p assemblywright-broker --test protected_boundary -- --nocapture
cargo test -p assemblywright-broker --test windows_create_directory_e2e -- --nocapture
cargo test -p assemblywright-executor --test process_group_e2e -- --nocapture
cargo test -p assemblywright-executor --lib -- --nocapture
cargo test -p assemblywright-executor --test windows_job_e2e -- --nocapture
cargo test -p assemblywright-protocol --test windows_execution_ipc_contract -- --nocapture
cargo test -p assemblywright-master --test windows_execution_ipc_foundation -- --nocapture
cargo test -p assemblywright-master --test windows_execution_ipc_source_contract -- --nocapture
cargo test -p assemblywright-broker --test inert_execution_ipc -- --nocapture
cargo test -p assemblywright-executor --test inert_execution_ipc -- --nocapture
cargo clippy -p assemblywright-protocol -p assemblywright-broker -p assemblywright-executor --all-targets -- -D warnings
```

The protocol and protected-boundary suites prove strict
signature/digest/schema/target/replay rejection at a portable repository boundary.
`windows_create_directory_e2e` must run on the native Windows owner host. It exercises
the proof-only adapter's complete no-delete-share ancestry, handle-relative single-leaf
creation, path-free result, and effect-possible quarantine boundary; the long-running
broker dispatch remains effect-disabled. On macOS, `process_group_e2e` proves real held
parent/object identity checks, rejects replacement before preparation, and proves that
an adversarial `setsid` escape command is denied before spawn. Process groups are not
claimed as descendant containment. `windows_job_e2e` must run on the native Windows
owner host: it launches a real descendant only after suspended-image verification and
Job assignment, then requires a signed receipt with a signaled root and zero active Job
processes. It also queries the live Job's CPU hard cap, aggregate commit cap, and
active-process limit. Executor library tests cover host-capacity derivation and
exact-limit matching, including native page alignment and large-memory arithmetic;
Windows-only tests exercise current-host and awkward-capacity Job round trips,
tampered Job limits, and bounded
process-limit rejection. Run the library suite on Windows as well as macOS: a macOS
pass cannot execute its Windows-only tests. These checks do not authenticate the
protected provisioned policy or prove installed disk/priority reservations.
Cross-compilation does not prove either native Windows boundary. The
authenticated inert Windows IPC foundation is covered separately below. Installed
production services, product Master routing, production broker effects, and activation
receipts remain unavailable. None
of these commands
prove OS code signatures, dedicated service identities/ACLs, privileged IPC,
resource reservations, activated Start/Stop/Emergency effects, deployment, or live
two-host use.

The required `Assemblywright Windows Distributed Gate` runs the broker and executor
Clippy checks plus both native suites on `windows-latest`. Its hosted success proves the
repository's Windows API and process boundaries for the exact commit; it does not prove
installation under the production service identities, owner-host deployment, or live
two-machine operation.

The broker unit/contract matrix covers accepted exact operations, malformed and empty
leaf names, reserved device aliases, native length overflow, signature/identity/digest
drift, protected/link/case targets, sequence gaps and replay, deterministic path-free
post-state binding, and ambiguous post-effect quarantine. Runtime IPC E2E covers
authenticated Stop/Emergency intent and FIFO replay behavior. Concurrent active-effect
cancellation and restart reconciliation are intentionally not claimed: the one-shot seam
is proof-only and production dispatch remains disabled until those contracts exist.

The Windows host-security substrate has separate source and native-service coverage:

```sh
cargo test -p assemblywright-master --test windows_execution_host_security_contract -- --nocapture
```

```powershell
& .\scripts\windows-execution-host-provision.ps1 -Mode DryRun
& .\scripts\windows-execution-host-provision.ps1 -Mode Check
& .\scripts\windows-execution-host-provision.ps1 -Mode SelfTest
& .\scripts\windows-execution-host-security-e2e.ps1 -Confirm
& .\scripts\windows-execution-ipc-e2e.ps1 -Confirm
```

`Check` is expected to reject an owner-account or user-local installed master; that is
evidence that the production substrate is not provisioned, not a reason to weaken the
contract. `Apply` additionally requires `-ConfirmStoppedServiceCeremony`,
already-installed stopped Broker/Executor service hosts, and an already stopped
LocalSystem Master; it deliberately cannot create service hosts. It never starts a
service or enables effects. The fixed Program Files images and their protected ancestry
must match the protected release manifest's exact SHA-256 values and one valid exact
Authenticode signer. Unsigned repository builds therefore keep Apply unavailable. The native E2E
uses randomized disposable service names and a real LocalService restricted service-SID
payload: an allowed marker proves execution before protected file overwrite, disk-reserve
deletion, and master service-reconfiguration attempts are verified unchanged. It then
queries both protected service objects under bounded CPU/memory pressure and removes
only its disposable objects. It also invokes the actual production DryRun and Check,
plus the production provisioner's real disposable SelfTest. SelfTest feeds hostile
hardlink, symlink, and effects-enabled prestate through the same policy-file,
disk-reserve, and registry validators used by Apply/Check and proves rejection without
mutation. It also proves a directory ACE with matching SID/type/rights but missing
required trusted inheritance is rejected, an inheritable Executor read ACE is rejected,
and a later-created sibling inherits no Executor read grant. It never targets
production. Browser E2E is inapplicable.

The same native E2E builds the shipped Broker and Executor binaries, generates
digest-bound read-only runtime configurations, and installs each binary under a
randomized disposable SCM name. Both binaries must reach `RUNNING`, expose a
nonzero process ID, accept SCM Stop, and reach a clean `STOPPED` state. A hostile
Broker configuration digest, a correctly re-digested but semantically invalid
runtime schema, and hostile extra Executor argv must fail closed. Broker ServiceMain
constructs the effect-disabled semantic runtime before reporting `RUNNING`; Executor
ServiceMain validates the complete semantic bootstrap while deliberately constructing
no active runtime and holding no receipt-signing secret.
The security E2E invokes the dedicated IPC E2E, including delayed response consumption
and a stalled reader that must fail within the five-second delivery deadline. First
and last pipe errors plus SCM exit codes distinguish the original rejection from
later retries after a service stops. That proof creates randomized
disposable Master, Broker, and restricted Executor SCM services; requires the exact
service-SID-authenticated local named-pipe roundtrip; verifies independently signed
Master hop frames and separately signed path-free Broker/Executor acknowledgements;
requires each pipe owner/DACL to use a configured service SID that is enabled and
owner-capable in the server process token, including the restricted Executor self-SID
access check; proves an unrelated LocalService service cannot open the pipe normally or
for `WRITE_DAC`;
rejects wrong SID, unsigned, tampered, sequence-gap, stale, and stale-authority frames;
restarts Broker/Executor and requires byte-identical acknowledgement replay; and records
zero effects. The same native exchange requires identification-only client SQOS, exact
canonical service-SID shape, process exit before deletion, and complete fixture-root
removal. A delayed-write service scenario proves the server reads a bounded client
message before binding impersonation to that message. Portable unit tests additionally cover exact pending-intent recovery,
expired byte-exact completed-ack replay without reprocessing, and partial-journal
quarantine. This proves only inert health/dispatch validation and
durable replay. The product dispatcher remains unavailable; adapter execution, active
cancellation, production deployment, and effect activation remain separate.

SelfTest additionally creates one randomized disabled disposable SCM service and passes
its valid binding through the real production validators. It then proves extra argv,
interactive/type drift, demand start drift, failure actions, and trigger persistence all
reject before cleanup. Production Check/Apply require canonical complete role-specific
argv, exact own-process/noninteractive disabled service configuration, no dependency/
trigger/recovery persistence, exact required privileges, and stopped-state rechecks
around every mutation cluster.
The restricted Executor has read/execute access only to its immutable executable,
configuration file, and the required ancestor directories through non-inheriting
grants; an inheritable exact mutation-rights deny prevents write, delete, permission,
or ownership changes. The restricted-token payload must fail to read a sibling created
under the readable ancestor. Other protected storage remains inaccessible, and both
runtime configuration files must be read-only.
The serialized Executor configuration contains no receipt-signing seed. Active Windows
runtime construction remains fail-closed until authenticated Broker IPC supplies that
secret out of band after payload-to-Executor process access is denied and proved.

The provisioned Job CPU/commit/process values remain activation-attested policy until
production executor runtime wiring creates and verifies the Windows Job. The allocated
disk reserve and protected ACL/service-SID boundary are native host mechanisms, but
repository or disposable E2E success is not signed production deployment proof.

These checks prove strict contracts, inert persistence, authenticated/bounded routes,
schema migration, strict Mac decoding/transport behavior, and presentation defaults at
repository-test boundaries. The loopback process E2E drives one exact Private project
through draft, frozen brainstorming specification, approval, exact replay, and
authoritative projection, then proves New Feature remains rejected until GitHub
creation evidence exists. They prove the source adapter process, frozen provenance,
Public-only disclosure, intent/reconciliation, and Swift owner review boundaries. They
do not prove Windows capability-separated provider/GitHub execution or live Windows execution of the
simplified UI action path, Start, Stop,
Emergency Pause, executors, broker containment, process termination, full-machine
effects, Windows deployment, signed/notarized Mac artifacts, or live two-host behavior.

The current fail-closed contract coverage includes strict GitHub URL and visibility handling,
owner-bound drafts/specifications/approvals, Public default/Private option,
`creation_pending` without external effect, Created-plus-evidence feature gating, FIFO,
default-on compare-and-set auto-run, stale session/epoch contract rejection, and
redacted planning audit. Start/Stop/Emergency are authenticated routes, but production
uses an unavailable effect dispatcher until the protected runtime is installed.
The planning-effects and real adapter-process suites prove the coordinator's catalog binding,
idempotent brainstorming reconciliation, pre-inspection GitHub intent, Public/Private
mapping, cancellation fencing, and exact post-effect observation. Windows still needs
distinct restricted identities/ACLs, atomic suspended-process-to-Job containment,
provisioning checks, and native hostile denial proof before the adapters may load.
Executable phases must add native E2E for those routes, one-at-a-time execution,
termination, reconciliation, and hostile protected-control-plane mutation denial.

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
./scripts/windows-repository-onboarding-self-check.sh
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

When the owner is running the distribution app, run packaging validation against a
separate output directory. The guard protects the exact app and bundled-core paths;
it should not require stopping the owner's app for an unrelated repository gate:

```sh
ASSEMBLYWRIGHT_DISTRIBUTION_DIR="$PWD/target/distribution-validation" \
  ./scripts/release-local.sh
```

This produces separate validation artifacts and does not update the running app.

## Windows Repository Onboarding

Onboard one existing, clean, standalone Windows repository in two deliberate
steps. Planning performs no owner-control mutation and returns a path-free
summary containing the plan ID:

```powershell
.\scripts\windows-repository-onboarding.ps1 -Action SelfTest
.\scripts\windows-repository-onboarding.ps1 -Action Plan -RepositoryPath C:\absolute\project
.\scripts\windows-repository-onboarding.ps1 -Action Check -PlanId <PLAN_UUID>
```

After reviewing the plan, record the three independent revision-1 grants and
run repository preflight with one explicit approval command:

```powershell
.\scripts\windows-repository-onboarding.ps1 -Action Approve -PlanId <PLAN_UUID> -ConfirmRegistration -ConfirmCloudDisclosure -ConfirmAutonomousPublication
```

`Check` is read-only and remains available after the plan expires. An expired
plan cannot create fresh grants. The same three confirmations may only resume
its exact partial or complete revision-1 grant set through preflight and receipt
publication, or replay its exact stored receipt when all three exact grants
remain current. If no grants exist, or any grant is foreign, drifted, inactive,
or revoked, create and review a new plan instead.

Paste the resulting compact `repository_onboarding_ready` JSON into **Author
an approved feature > Import repository onboarding receipt** in the Mac app.
Import fills only the repository ID and three grant revisions; the form still
requires approval evidence, a current authenticated snapshot, review, and the
second enqueue confirmation. The script does not create a repository, enqueue,
dispatch, mutate source, publish, or activate the conveyor. For a new project,
create and commit the empty standalone `main` repository first. There is not yet
a general owner-control dispatcher for arbitrary newly onboarded repositories.

The portable contract regression is:

```sh
./scripts/windows-repository-onboarding-self-check.sh
```

The Windows-hosted native process E2E is:

```powershell
cargo test -p assemblywright-master --test windows_repository_onboarding_e2e --locked -- --nocapture
```

It launches the real master on a disposable loopback endpoint and runs real
PowerShell `Plan`, `Approve`, and `Check` processes against a disposable clean
standalone `main` repository. It verifies separate approval switches, rejects
post-plan repository drift and expired fresh approval without recording grants
or audit evidence, proves read-only expired-state inspection and exact partial
and complete-without-receipt revision-1 forward recovery, requires three exact
active revision-1 grants and a preflight-bound path-free receipt, and proves that an exact expired completed-plan
replay returns the immutable receipt without advancing authority or duplicating
redacted audit evidence. It does not
touch the installed service, any real repository, external network, activation,
queueing, publication, signing, notarization, or live-device readiness.
Normal test teardown kills and reaps the spawned master, then removes the
temporary repository, master data, owner token, private plan, and receipt. The
fixture creates no remote and uses no external credentials.

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
is fast-forwarded to the same commit, schema-v20 service health and the preserved
schema-v19 Feature Conveyor state are confirmed, and the signed Mac helper remains
current, set the exact current owner-control
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

## GitHub-publication Live Proof Controller

Static validation and disposable controller regressions run in the canonical
local gate:

```sh
cargo test -p assemblywright-master --test github_publication_adapter -- --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel publication_ -- --nocapture
cargo test -p assemblywright-protocol --test publication_coordinator_contract -- --nocapture
./scripts/github-publication-live-e2e.sh --check
./scripts/github-publication-proof-controller.sh --check
./scripts/github-publication-proof-controller.sh --self-test
```

These focused Rust suites cover the smallest portable adapter and coordinator
units, including unavailable configuration, strict request/evidence binding,
policy and workflow provenance, cancellation/deadlines, restart ambiguity,
idempotence, remote-base drift, cleanup failure, command-path isolation, and
sanitized receipt rejection. The controller self-test supplies the applicable
maximum-input, hostile-output, process-reaping, Git-reconciliation, and private-
receipt fixtures; coordinator tests exercise concurrent cancellation and pause
races. The controller check statically enforces the committed Windows mutex,
staging-ownership, and transactional rollback contract, while only the live
Windows lane executes that PowerShell path. `cargo llvm-cov` is not part of the
pinned toolchain, so closeout must not claim a numeric coverage percentage
unless an independently provisioned coverage run is recorded.

On exact clean protected GitHub `main`, validate or provision the fixed Windows
GitHub CLI/Git identities and owner-private authentication boundary from the
authenticated Administrator session:

```powershell
& .\scripts\windows-github-publication-live-control.ps1 -Action SelfTest
& .\scripts\windows-github-publication-live-control.ps1 -Action Check
& .\scripts\windows-github-publication-live-control.ps1 -Action Provision -ConfirmAction
```

After Mac and Windows equal the exact published implementation commit, start
the Mac controller:

```sh
./scripts/github-publication-proof-controller.sh --run
```

When it emits the single Windows action marker, run and return the one sanitized
JSON line:

```powershell
& .\scripts\windows-github-publication-live-control.ps1 -Action Run -ConfirmAction
```

The controller keeps a bounded one-hour receipt-input window because the live
action must complete both the pull-request checks and the resulting protected-
main post-merge checks before Windows may emit the sanitized receipt. The
window does not relax receipt freshness or any source, tool, check, merge, or
cleanup binding.

The fixed live lane uses GitHub CLI secure storage for `malak333`, repository
`malak333/Assemblywright`, protected `main`, normal merge, and the exact
`Release local gate` plus `Protocol, master, identity, mTLS, and SCM` contexts.
Windows PowerShell 5.1 represents GitHub CLI's successful `auth status` stderr
as `NativeCommandError` under the control's global fail-closed preference. The
control scopes only that native status call to `SilentlyContinue`, discards its output,
captures its exit code immediately, and still accepts authentication only on
zero; a null sentinel rejects process-launch failure instead of reusing a stale
native exit code. The Windows-hosted self-test exercises successful stderr,
nonzero exit, and launch failure. All surrounding validation remains
`Stop`-dominant.
The adapter retains canonical Windows paths for containment and identity checks,
but removes only a recognized `\\?\` or `\\?\UNC\` prefix when constructing
Git's `--git-dir`, `--work-tree`, and credential-helper process arguments. Git
for Windows rejects the verbatim form even though the same directory is valid;
normalization must remain confined to this child-process boundary and must
still produce an absolute DOS or UNC path.
It creates a disposable bounded metadata-only proof-marker commit and
intentionally advances `main` through one non-admin protected pull request,
then verifies the exact merge and
post-merge `Release local gate`. A pass writes the owner-private
`target/github-publication-live-proof/github-publication-live-proof.json` plus
raw SHA-256 sidecar. The receipt is not admitted automatically and is not an
actual queued-feature publication, signing/notarization, clean-profile, or
production-readiness claim. Playwright does not apply to this native
Rust/Git/GitHub/PowerShell boundary.

## Restart-recovery Live Proof Controller

Portable controller and exact native restart coverage:

```sh
./scripts/restart-recovery-live-e2e.sh --check
./scripts/restart-recovery-proof-controller.sh --check
./scripts/restart-recovery-proof-controller.sh --self-test
cargo test -p assemblywright-agent --test local_relay_e2e authenticated_uds_local_coding_snapshot_admission_cancellation_and_restart_cleanup -- --exact --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel restart_quarantines_ambiguous_active_feature_without_retry -- --exact --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel startup_quarantine_audit_failure_rolls_back_and_blocks_open -- --exact --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel publication_restart_with_unresolved_external_intent_quarantines_without_retry -- --exact --nocapture
cargo test -p assemblywright-master --test feature_conveyor_kernel restart_quarantines_an_indeterminate_review_call_without_retry -- --exact --nocapture
```

The agent E2E starts the real Rust process, retains the exact workspace/record
pair, restarts, and proves exact cleanup. The master tests remain repository
evidence for effect-possible startup quarantine and audit rollback; the live
controller does not manufacture active work to exercise that boundary.

After Mac and Windows both equal the exact clean published implementation SHA,
start the fixed Mac controller:

```sh
./scripts/restart-recovery-proof-controller.sh --run
```

Before the Windows `Check`, the installed service executable must already be
an ordinary single-link file with a protected ACL limited to the fixed owner
and SYSTEM. A direct `cargo build --release` output can remain hard-linked to a
file under `target\release\deps` and inherit broader checkout ACLs; that is not
eligible proof state. Stop the service, preserve the exact executable digest,
materialize those same bytes into a new single-link service-path file, apply the
owner/SYSTEM-only ACL, verify the digest is unchanged, restart, and recheck
health. This is explicit deployment preparation, creates no proof receipt, and
must not be confused with the subsequent recovery proof.
The fixed loopback foundation health route must report mode
`developer_foundation`; `developer_remote_master` belongs to the enrolled mTLS
route, and the nonexistent `developer_local_master` string is not an alias for
either boundary. The pinned Cargo and rustc files must likewise already be
ordinary single-link files with protected ACLs limited to the fixed owner and
SYSTEM; `Check` validates but never rewrites that tool trust state.
The Windows control passes the fixed quoted `vcvars64.bat` call to
`cmd.exe /d /s /c` as one PowerShell argument; embedding an additional outer
quote pair makes `cmd.exe` reject the fixed path before Cargo runs and must fail
closed with exact healthy service restoration and no receipt.
On Windows PowerShell 5.1, Cargo writes normal progress to stderr; with the
control's global `ErrorActionPreference=Stop`, a direct native invocation turns
that progress into a terminating `NativeCommandError`. The fixed build runs
Cargo in a child scope with `ErrorActionPreference=Continue`, redirects every
stream to the bounded private build log, and accepts success only from Cargo's
captured native exit code. The global fail-closed preference remains unchanged.
Cargo can also hard-link the fresh release output to its `deps` artifact. The
control treats that as valid compiler output but not as an eligible service
image: it hashes the ordinary Cargo output, copies those bytes into a new
proof-owned single-link file, applies the owner/SYSTEM-only ACL, and requires
the materialized digest to match before installation.
The fixed Windows link step is not treated as byte-reproducible across builds.
The schema-v2 live receipt therefore binds the transient exact-source rebuild
digest separately from the original/restored installed-service digest; neither
digest may substitute for the other, and the proof makes no installed-image
source-provenance claim.

When it prints the single action marker, copy its `expected_source_head` into
the authenticated Windows Administrator session and return only the sanitized
JSON line:

```powershell
& .\scripts\windows-restart-recovery-live-control.ps1 -Action Check
& .\scripts\windows-restart-recovery-live-control.ps1 -Action Run -ExpectedSourceHead <CONTROLLER_HEAD> -ConfirmAction
```

The Windows run is a real stopped-state recovery-and-restoration sequence for
the fixed `AssemblywrightMaster` service. It freezes/hashes the sidecar-free
database, rebuilds exact HEAD offline with the fixed absolute toolchain,
materializes Cargo's output as a rebuilt owner/SYSTEM-only single-link image,
requires it to match the Cargo output and binds that transient digest separately
from the original/restored service digest, proves a healthy changed PID, stops
and requires the exact same full
database SHA plus logical/migration continuity, restores the original image,
then proves a distinct final healthy PID and empty state. Failure attempts exact
healthy restoration and emits no receipt. The Mac controller likewise uses
digest-pinned Git, Cargo, and rustc with a validated fixed Cargo home, rejects
checkout/ancestor Cargo configuration, and executes through absolute system
tools under a system-only PATH. A pass writes the ignored owner-private
`target/restart-recovery-live-proof/restart-recovery-live-proof.json` and raw
SHA-256 sidecar. It does not admit the digest or prove active-effect crash
recovery, SCM retry policy, signed-helper behavior, Feature 6 streaming,
activation, signing/notarization, installation, or production readiness.
Playwright is inapplicable because this is a native Rust/process/SQLite/SCM/
PowerShell boundary with no browser surface.

## Feature 6 Mac/Windows Control-Streaming Proof

Static and disposable negative-path coverage run in the canonical local gate:

```sh
./scripts/mac-windows-control-streaming-proof-controller.sh --check
./scripts/mac-windows-control-streaming-proof-controller.sh --self-test
```

The live owner-run command is deliberately separate and requires exact clean
published `main`, the fixed signed helper and agent, the enrolled Keychain mTLS
identity, Tailscale reachability, and the authoritative Windows master:

```sh
./scripts/mac-windows-control-streaming-proof-controller.sh --run
```

The controller executes only committed `mac-windows-bridge-live-e2e.sh` bytes
through fixed Bash stdin in `--run-relay` mode. The native E2E proves the signed
Swift helper's exporter-bound owner-control projection and a durable same-stream
event cursor advancing after helper/agent restart. It hashes and deletes the
private transcript and retains no endpoint or stream ID. The resulting
owner-private schema-v1 receipt is proof material only: do not admit it or infer
activation, protocol/schema/runtime authority, built-binary source linkage,
Developer ID distribution, notarization, installation, unattended operation,
or production readiness. Playwright is inapplicable because no browser surface
participates.

## Feature 7 Windows Owner Evidence Admission

The PowerShell 5.1 onboarding client has a mutation-free disposable self-test
and an authenticated redacted status/check surface:

```powershell
.\scripts\windows-owner-evidence-admission.ps1 -Action SelfTest
.\scripts\windows-owner-evidence-admission.ps1 -Action Status
.\scripts\windows-owner-evidence-admission.ps1 -Action Check
.\scripts\windows-owner-evidence-admission.ps1 -Action Check -ReceiptPath <FIXED_RECEIPT_PATH> -DigestPath <FIXED_SIDECAR_PATH>
```

After copying one canonical proof-controller pair to an ordinary single-link
Windows file pair, protect both files to only the current owner and SYSTEM with
inheritance disabled. Admission is one separate deliberate action:

```powershell
.\scripts\windows-owner-evidence-admission.ps1 -Action Admit -ReceiptPath <FIXED_RECEIPT_PATH> -DigestPath <FIXED_SIDECAR_PATH> -Confirm
```

The endpoint is fixed to owner-authenticated loopback. The tool reads the token
from protected master state, validates canonical controller bytes through held,
non-reparse, no-write/no-delete-share file handles before preflight, sends digest
metadata only, and never activates. If the POST response is ambiguous, rerun
`Status` or the same confirmed command: the fresh GET preflight recognizes an
exact current digest before pause/activation denial and without a second POST.
Do not reorder, whitespace-rewrite, reserialize, or change the fixed boundary
of a receipt, relax
its ACL, manufacture a digest, or describe admission as release proof.

Focused portable/native coverage:

```sh
cargo test -p assemblywright-protocol --test owner_activation_contract
cargo test -p assemblywright-master --test feature_conveyor_kernel activation_ -- --nocapture
cargo test -p assemblywright-master --test master_process_e2e artifact_integration_routes_are_owner_loopback_only_strict_and_redacted -- --nocapture
cargo test -p assemblywright-master --test remote_mtls_e2e remote_listener_requires_enrollment_tls13_and_channel_bound_identity -- --nocapture
```

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

`.github/workflows/windows-protocol.yml` runs formatting; package-scoped clippy and
tests for the protocol, master, broker, and executor crates; the master-process E2E;
the native broker create-directory and executor Job Object E2Es; and the elevated
Windows SCM service lifecycle E2E. The native tests are the authoritative proof for
Windows final-DOS-path normalization, alternate-path rejection, POSIX
rename/replacement revalidation, held-handle identity comparison, handle-relative
directory creation, and Job Object descendant reaping; macOS execution cannot prove
those Win32 behaviors.

Use the workflow's package-scoped protocol, master, broker, and executor clippy commands on
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
| Windows repository onboarding across disposable Git, PowerShell, and the real owner-loopback master | `cargo test -p assemblywright-master --test windows_repository_onboarding_e2e --locked -- --nocapture` |
| Windows remote mTLS observer, designated-owner enqueue, owner-control projection, activation, and denial/success | `cargo test -p assemblywright-master --test remote_mtls_e2e remote_listener_requires_enrollment_tls13_and_channel_bound_identity -- --nocapture` |
| Swift strict Feature Conveyor observer, typed approved-feature authoring/enqueue, one-shot owner actions, and helper lifecycle | `swift test --disable-sandbox --package-path apps/mac --filter DeveloperBridgeTests` |
| SwiftUI approved-feature form and production-Swift-to-strict-Rust authoring fixture | `swift test --disable-sandbox --package-path apps/mac --filter AssemblywrightMacAppTests` and `cargo test -p assemblywright-protocol swift_approved_feature_authoring_fixture_strictly_decodes_in_rust` |
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
  prove strict schema-v9 decoding, typed review-safe authoring, exact validation-
  gate insertion, current-revision request binding, exact digest shapes,
  duplicate/self-dependency/secret-shaped rejection, deterministic client
  bytes, redacted-receipt validation, bounded real-process one-shot execution,
  fail-closed cancellation, authenticated snapshot propagation, explicit UI
  confirmation with frozen request bytes, authenticated revision tuple, and
  request SHA-256, fixed provisioned provider/model,
  identical-byte lost-receipt reconciliation, replay drift rejection, one-audit
  idempotence, and observation restart. The signed-helper snapshot test carries
  alphabetic admitted-evidence UUIDs through production snapshot encoding and
  the app lifecycle's strict decoder, while unit cases reject uppercase
  evidence, activation, and active-feature UUID text. A shared fixture proves
  those exact production Swift request bytes strict-decode and
  canonical-digest-validate in Rust. Windows remains canonical-digest and queue
  authority; the app form does not establish grant, claim, dispatch,
  repository, provider, publication, or activation authority.
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

## Owner Connection Setup

The Mac app's Developer Mode Connection card replaces normal-product launch-variable
configuration. Choose the separately Xcode-provisioned helper and enter the independently
verified ten-character Apple team identifier. The app validates the fixed helper identity
before saving an owner-private locator and before every supervision or one-shot command.
It then exposes the same bounded enrollment and rotation stdin ceremonies documented
below. Public invitations, CSR replies, and certificate receipts remain in memory; the
Windows pairing process retains the only raw grant.

Focused native coverage:

```sh
swift test --disable-sandbox --package-path apps/mac --filter persistedHelperLocator
swift test --disable-sandbox --package-path apps/mac --filter persistedOwnerSetupNativeProcessE2E
swift test --disable-sandbox --package-path apps/mac --filter AssemblywrightMacAppTests
swift test --disable-sandbox --package-path apps/mac --filter DeveloperBridge
```

These tests cover strict persistence, signed-helper command whitelisting, presentation,
and a real persisted-configuration-to-monitor/status process lifecycle with bounded reap.
The native process fixture uses a test signature validator. These tests do not prove the Xcode-provisioned Keychain group,
live Windows enrollment, Developer ID distribution, notarization, or clean-profile launch.

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

## Owner-Confirmed Standard Certificate Rotation

Use the two-document rotation ceremony before certificate expiry or to recover
an already-expired standard MacBridge identity. Stop the Windows service first;
the pairing process owns the authority while it retains and zeroizes the raw
grant. Pass only the public invitation, CSR reply, and issued certificate
receipt through stdin. Never place any ceremony document or raw grant in argv
or shell history, and never persist the raw grant. The master alone persists
the exact public issued-certificate receipt in its protected recovery journal
before the authority commit.

```text
Windows: assemblywright-master ... enrollment rotate-pair --device-id UUID \
  --master-endpoint IP:PORT --confirm
Mac:     assemblywright-mac-bridge enrollment rotate prepare --confirm
Mac:     assemblywright-mac-bridge enrollment rotate install --confirm
Windows: assemblywright-master ... enrollment rotate-recover-acknowledge \
  --grant-id UUID --confirm
```

If the issued receipt is lost after CSR submission, do not create another
rotation grant. With the service still stopped, re-emit the byte-identical
grant-bound receipt as often as necessary:

```text
Windows: assemblywright-master ... enrollment rotate-recover \
  --grant-id UUID --confirm
Mac:     assemblywright-mac-bridge enrollment rotate install --confirm
```

Run `rotate-recover-acknowledge` only after Mac installation and authenticated
reconnect are verified. Until that explicit acknowledgement, recovery remains
idempotent and the owner-private journal remains available. Acknowledgement
removes only that exact validated public receipt journal.

The rotation preserves the exact device ID, name, role, registry revision,
capabilities, endpoint, pinned CA, and owner-control designation. Windows
revokes the prior certificate serials when issuance commits; Mac installation
validates the new leaf against the currently selected Secure Enclave key before
forward-resumably replacing only its certificate and installed record.
Interruption before CSR acceptance issues nothing. Missing output after CSR
submission is resolved only through the exact confirmed recovery command above.
Focused native coverage is:

```sh
cargo test -p assemblywright-master --test enrollment_identity_e2e -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter DeveloperBridge
```

## Protocol v5 general-worker validation

```bash
cargo test -p assemblywright-protocol --test local_coding_contract
cargo test -p assemblywright-master --test feature_conveyor_kernel master_process_v12 -- --nocapture
cargo test -p assemblywright-agent --test local_relay_e2e authenticated_uds_local_coding_snapshot_admission_cancellation_and_restart_cleanup -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter DeveloperBridgeTests
./scripts/mac-local-coding-snapshot-e2e.sh
```

These are repository and native-boundary checks, not Windows deployment, signing, notarization, live-device, or release proof.


## Windows execution and IPC hardening coverage

The `unit-testing-test-generate` workflow was applied to the testable policy,
verification, framing, and recovery boundaries below. Coverage is assessed by
behavior and failure mode; no line-coverage percentage is claimed. The
`e2e-testing` workflow uses the real native boundaries for this slice. There is
no browser surface, so Playwright, browser matrices, and visual regression are
not applicable.

| Phase | Focused unit and negative coverage | Native E2E and CI coverage |
| --- | --- | --- |
| Windows Job resource limits | `windows_resource_policy_derivation_is_bounded_and_fail_closed` and `resource_policy_attestation_rejects_every_limit_and_flag_drift`: minimum/maximum capacity, page alignment, overflow, invalid telemetry, and each queried limit/flag | Native Job tamper/rounding/process-limit tests and `windows_job_e2e`; Windows distributed gate |
| Provisioner resource policy | Production `SelfTest` exercises exact large-memory arithmetic, awkward page alignment, and invalid capacity/page inputs | `windows-execution-host-security-e2e.ps1` runs SelfTest plus restricted service SID, protected storage/reserve, and real service-host checks in Windows CI |
| Authenticated IPC delivery and replay | `windows_execution_ipc_contract`: strict framing/signatures, stale/binding drift, journal intent/ack/replay, partial-state quarantine | `windows-execution-ipc-e2e.ps1`: real three-service named pipes, hostile peer access, delayed/stalled readers, exact timeout diagnostics, restart replay, cleanup; invoked by host-security E2E |
| Mac peer verification | `timed_out_identity_workers_retain_bounded_permits_without_convoying_distinct_tokens`, `peer_frame_is_not_dispatched_before_identity_success`, exact success-cache and rejecting-verifier tests | Real audit-token verification, `local_relay_e2e`, native snapshot E2E, and macOS release-local gate |
| Planning/PTY test startup | Readiness is separate from existing behavior deadlines; timeout/cancellation paths retain child cleanup | Master tests plus restricted-worker, GitHub-publication, and restart-recovery controller self-tests in release-local |

Test fixtures use disposable directories/services and bounded fake verifiers where
needed to control failure timing; real OS checks remain separate. The native
Windows run passed 465 tests with 13 explicitly ignored fixtures/elevated tests,
plus the full IPC and host-security suites. A fresh, hash-verified Mac worktree
passed the canonical local gate with 483 Rust and 198 Swift tests. These results
cover the implementation bytes validated locally; publication additionally
requires both protected GitHub checks on the current PR and merged SHA.

Each later phase must repeat the documentation, knowledge-base, unit/E2E,
independent-review, and publication checklist in
[`development-agent-workflow.md`](development-agent-workflow.md#feature-and-phase-closeout).
Production adapter execution, active-effect recovery, installed policy binding,
signed service cutover, and owner UI proof remain separate uncompleted milestones.
