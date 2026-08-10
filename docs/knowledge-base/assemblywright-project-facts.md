# Assemblywright Project Facts

These notes capture durable facts for future agents working on this repository.

## Product Identity And Licensing

- The product name is **Assemblywright**. The primary positioning line is
  "Orchestrated intelligence. Verified software."
- The coordinated model system is **The Assembly**; the feature conveyor is
  **The Line**; approved specifications are **Blueprints**; local
  implementation agents are **Builders**; deterministic and independent-model
  validation are the **Proof Gate** and **Review Gate**; the owner interface is
  the **Control Room**.
- The repository license is Apache-2.0. The root `LICENSE` file and Cargo
  workspace metadata are authoritative.
- The Cargo crates are `assemblywright-protocol`, `assemblywright-master`,
  `assemblywright-agent`, `assemblywright-core`, and `assemblywright-cli`. Their
  binaries are `assemblywright`, `assemblywright-agent`, and
  `assemblywright-master`; the legacy `assemblywright*` binary aliases were removed.
- The rename from the former product name is total. No crate, binary, SwiftPM
  product or target, environment variable, Keychain service, state directory,
  Windows service, code-signing identifier, wire label, or certificate subject
  carries the old name. `./scripts/release-naming-contract-smoke.sh --check`
  scans every tracked path and file and fails if it reappears.
- Exactly one legacy reference survives: `LEGACY_MASTER_STATE_NAMESPACE` in
  `crates/assemblywright-master/src/main.rs`, a read-only path used once to adopt
  a pre-rename Windows master state directory. The gate pins it by name and caps
  how many lines of that file may mention the old namespace, so the exception
  cannot grow.
- `PROTOCOL_VERSION` is 2. It moved from 1 because the rename changed wire
  values: the TLS exporter label, the fixture capability provider and model, and
  the certificate subject and SAN URI. Two builds both claiming version 1 while
  disagreeing on those would be mutually incompatible, which is exactly what the
  version field exists to prevent. A pre-rename peer now fails on version.
- The current identity surface: code-signing identifier
  `com.nobiletechnology.assemblywright` and its `.core` suffix for the bundled
  CLI; bundle executable `AssemblywrightMacApp`; bundled CLI
  `assemblywright-cli`; Keychain service
  `com.nobiletechnology.assemblywright.developer-bridge` (plus a `.fixture`
  sibling); `~/Library/Application Support/Assemblywright`; Windows service
  `AssemblywrightMaster` with state in `%LOCALAPPDATA%\Assemblywright\master`;
  TLS exporter label `EXPORTER-Assemblywright-Developer-Mode-v1`; certificate SAN
  `urn:assemblywright:device:<uuid>`; fixture capability
  `assemblywright-fixture` / `assemblywright-fixture-v1`; environment variables
  `ASSEMBLYWRIGHT_*`.
- The SwiftPM products and targets now share names: `AssemblywrightMacCore`,
  `AssemblywrightMacApp`, and the `assemblywright-mac-bridge` executable from the
  `AssemblywrightMacBridgeCLI` target. The product-facing app and release
  artifact names are `Assemblywright.app` and `Assemblywright-<version>.*`.
- The canonical remote is `https://github.com/malak333/Assemblywright`. GitHub
  still redirects the former URL, so old clones keep working, but new references
  must use the current one. The local working directory is `Assemblywright`.

## Repository Housekeeping Already Completed

- Crates, binaries, SwiftPM products, the working directory, and the remote are
  all current. Do not re-plan any of these as outstanding work.
- The `.worktrees/` directory of finished-branch worktrees is gone, and
  `git worktree list` should show only the primary checkout. If stale worktrees
  reappear, prune them with `git worktree prune` before adding new ones.
- The `codex/*` branches were deleted locally and on `origin` after their
  squash-merge PRs landed. `origin` should carry `main` only. Their tip commits
  are recorded in `~/Antigravity/codex-branch-cleanup-manifest.txt`, which is
  outside the repository on purpose so a deleted branch stays recoverable with
  `git push origin <sha>:refs/heads/<branch>`. That manifest is the only record;
  do not delete it while any recovery might still be wanted.

## Migrating A Host Past The Rename

The rename crosses signed identity and installed state, so an already-enrolled
host needs an explicit migration. `docs/brand.md` has the full table of what
changed and what each change costs.

**Windows master host.** The executable filename changed, and
`windows_service_host::install` calls `create_service`, which cannot rewrite an
existing registration. A host that only pulls and rebuilds keeps running the
stale pre-rename binary and still reports healthy, because the old executable
survives until `cargo clean`. Run this elevated, after the rebuild:

```text
assemblywright-master.exe service stop --service-name JarvisMaster
assemblywright-master.exe service uninstall --service-name JarvisMaster --confirm
assemblywright-master.exe --data-dir "%LOCALAPPDATA%\Assemblywright\master" ^
    service install --service-name AssemblywrightMaster --bind 127.0.0.1:7791 ^
    --remote-bind <overlay-ip>:7792 --identity owner-account ^
    --credentials-stdin --confirm
assemblywright-master.exe service status --service-name AssemblywrightMaster
```

Stop and uninstall still use the *old* service name, because that is what is
registered; install creates the new one. The subcommand is `service <verb>`, not
`service-<verb>`; `service-run` is the hidden SCM entry point and is never
invoked by hand. `install` and `uninstall` both require `--confirm`.

Owner-account installation requires `--credentials-stdin` so the password never
reaches argv. Supplying it with `echo {...} | assemblywright-master.exe` defeats
that: the password lands in shell history and in the `echo` process's own command
line. Prompt for it instead, and hand the master a document built in memory:

```powershell
$secure = Read-Host -Prompt "Windows password for $account" -AsSecureString
$bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
try {
    $plain = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    (@{ account_name = $account; password = $plain } | ConvertTo-Json -Compress) |
        & $exe --data-dir $dataDir service install --service-name AssemblywrightMaster `
            --bind 127.0.0.1:7791 --remote-bind <overlay-ip>:7792 `
            --identity owner-account --credentials-stdin --confirm
} finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
}
```

`CreateService` does not verify the password, so `install` reports success even
when it is wrong. Only `service start` proves it: a wrong password fails there
with `os error 1069` (logon failure), and the fix is to uninstall and reinstall.
Always follow an install with a start before considering the host migrated.

**Master state.** On its first run the master will adopt a pre-rename
`%LOCALAPPDATA%\Jarvis\master` directory by moving it to
`%LOCALAPPDATA%\Assemblywright\master`, so `master.sqlite3`, the DPAPI-protected
enrollment authority, and the owner lock survive. It is a move, not a copy: two
directories both claiming to be the authority is the ambiguity the safety rules
say to refuse. If the new directory already exists the legacy one is left alone
and the current one wins, because guessing which is authoritative is not safe.
Uninstalling the service never touches either directory.

**Every enrolled device must re-enroll.** The certificate SAN moved from
`urn:jarvis:device:` to `urn:assemblywright:device:` and the enrollment CA
subject changed, so already-issued device certificates no longer verify. The
Keychain service also moved, so the Mac bridge cannot read its previous identity.
Re-enroll through the normal grant flow; device certificates are 30-day and
rotate anyway, so this is an ordinary operation rather than a special path.

**Release evidence must be regenerated.** The code-signing identifier, the bundle
executable name, and the bundled CLI filename all changed, and signed provenance
and live-device QA reports bind all three. Prior reports no longer describe the
shipped bundle. Re-sign, re-notarize, re-staple, redo live-device QA, and rebuild
the evidence bundle. Do not carry a pre-rename report forward.

**Owner shell profiles.** Every `JARVIS_*` variable is now `ASSEMBLYWRIGHT_*`.
Regenerate the QA and evidence templates with
`./scripts/release-live-device-qa.sh --write-template` and
`./scripts/release-evidence-bundle.sh --write-template` rather than hand-editing
an old one.

## The Pivot

- The repository began as a local-first macOS assistant. It is now a
  developer-agent system built around the Feature Conveyor, local model
  workers, and Codex-assisted planning and review.
- The assistant surface was removed, not deprecated: the conversation runtime,
  model providers and routing, the plugin host and wasm sandbox, the SQLite
  task/audit/memory/approval store, the scheduler and its notification outbox,
  trusted system-wake, workspace roots, the permission engine, voice input and
  speech output, and the assistant CLI and Mac tabs are all gone.
- Do not reintroduce those surfaces or cite them as existing capability. If a
  document, script, or comment still refers to them, that is drift to fix.
- The plugin-trust QA release lane was removed with the plugin system it
  audited. Signing, notarization, live-device QA, and the final evidence bundle
  remain.

## Current Crate Boundaries

- `assemblywright-protocol` — versioned, bounded wire contracts. No I/O, no state.
- `assemblywright-master` — the durable Windows authority: distributed device
  lifecycle, the default-inert Feature Conveyor repository kernel, enrollment
  identity and mTLS, and the Windows SCM service host. Does not depend on
  `assemblywright-core`.
- `assemblywright-agent` — the Mac worker. Depends on `assemblywright-protocol` for contracts
  and on `assemblywright-core` only for the peer-identity Unix-socket transport and its
  startup validation.
- `assemblywright-core` — the shared local foundation: `ipc_transport`, `startup`,
  `macos_code_identity`, and `release`. It holds no conversation, model, tool,
  memory, scheduler, plugin, or repository authority.
- `assemblywright-cli` — a read-only release and evidence client. Its only subcommand
  tree is `release`.
- `apps/mac` — the SwiftUI Developer Mode client plus the separately signed
  bridge helper CLI.

## Apple Peer Identity Boundary

- Default app-supervised IPC uses `unix_socket_peer_identity_v1`. Its strict
  startup transport contains the bounded absolute `socket_path`, nonempty
  maximum-4096-byte `peer_code_requirement`, and exact `peer_identity_profile`
  (`adhoc_exact` or `developer_id_hardened`). The bearer remains a separate
  startup-stdin value.
- Both Swift and Rust use `LOCAL_PEERTOKEN` and Security.framework dynamic-code
  validation before request framing, retain `getpeereid` current-EUID checks,
  and still require the per-launch bearer. PID/path lookup is not an identity
  authority. Missing tokens, malformed requirements, wrong code, mixed
  profiles, and unsigned peers fail closed.
- Package signing keeps the app identifier at the fixed
  `com.nobiletechnology.assemblywright` identifier and explicitly assigns the bundled
  CLI `com.nobiletechnology.assemblywright.core`; package bundle-ID overrides are
  rejected because they cannot satisfy the fixed identity policy. Never rely on
  codesign's hash-derived identifier for the bare Mach-O.
- An ad-hoc designated requirement is exact-build cdhash evidence without a
  TeamIdentifier; it does not prove publisher trust. The
  `developer_id_hardened` profile separately requires Apple-generic Developer
  ID Application leaf/intermediate certificate extensions, stable IDs, the same
  nonempty team, and hardened-runtime flags. Ordinary Apple Development
  signatures do not satisfy that profile.

## Proof Boundaries

- Repository validation is distinct from signing, notarization, live-device QA,
  and owner-recorded external evidence. Never conflate them.
- `release readiness` reports `production_ready: false` until signed
  distribution, notarization and stapling, and the final evidence bundle checks
  validate. `ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external` is set only after
  owner-recorded evidence exists.
- `release evidence-status` is file and report inventory plus structural
  validation. Presence never proves that an external check happened.
- The Feature Conveyor kernel includes one observation seam:
  owner-token-authenticated loopback-only
  `GET /v1/feature-conveyor/status`. Its pure-SELECT response is bounded to 100
  redacted current queue or retained-lease lifecycle entries, excludes terminal
  history, includes current aggregate lifecycle counts and an explicit
  visible-total/truncation signal. The exact same projection is available at
  `GET /v1/distributed/feature-conveyor/status` only after an accepted
  exporter-bound application session for an enrolled MacBridge; other roles
  are denied and no owner token is forwarded. Its fixed-enum `owner_guidance`
  object is snapshot-bound
  to queue, Emergency Pause, and optional feature lifecycle revisions and
  applies a strict precedence: Emergency
  Pause, retained-lease reconciliation, normal active work, empty queue,
  queue-head dependency blocking, then ready head. It contains no free text or
  dependency, repository, provider, grant, evidence, or audit identifiers.
  `queue_revision` and `emergency_pause_revision` always bind the snapshot. The
  labeled next action is display-only and is not claimability or authorization.
  The Swift helper fetches health then status on the same session, rejects
  nested duplicates, extra keys, enum drift, oversize and cross-field
  inconsistency, and publishes status only in authenticated snapshots. The app
  presents compact queue/guidance text; failure cancels the session and clears
  the sample. The observation routes grant no enqueue, mutation, worker, Codex,
  repository, Git, review, publication, Mac control UI, or autonomous activation
  authority.
- Schema v8 adds one nullable/default-deny owner-control bridge designation.
  Only the owner-token-authenticated Windows loopback
  `POST /v1/feature-conveyor/owner-control-bridge` may select or replace one
  exact current, enrolled, non-revoked, non-fixture MacBridge using compare-and-
  set revision and same-transaction redacted audit. Another valid MacBridge,
  including the fixture profile, has no owner-control authority.
- Only the exact designated bridge on a revalidated exporter-bound session may
  call `POST /v1/distributed/feature-conveyor/approved-features`. Its fixed
  schema binds an already-approved specification to current queue, designation,
  and Emergency Pause revisions and reuses canonical manifest digest, current
  independent grants, dependency, capacity, immutable-specification, and atomic
  audit checks. Success only inserts a queued feature; it never claims,
  dispatches, accesses a repository, invokes a provider/reviewer, uses Git, or
  publishes. The standard-profile signed helper exposes the action only as
  `feature-conveyor approve-and-enqueue --confirm` with bounded stdin, strict
  recursive duplicate rejection, and a redacted bound receipt. The app remains
  read-only.
- Repository-grant preparation is now supported only through the owner-token-
  authenticated Windows loopback `POST /v1/feature-conveyor/repository-grants`;
  current grant inspection uses
  `GET /v1/feature-conveyor/repositories/:repository_id/grants`. The fixed
  schema carries digest-only scope and approval bindings, the contiguous next
  revision, expected current revision, and Emergency Pause revision. Active
  grant creation fails while paused, revocation remains available, and the
  immutable revision plus redacted audit commit atomically. The projection is
  limited to the three current fixed grant kinds and their revision, digests,
  expiry, revoked, and computed active state. Neither route exists on enrolled-
  device mTLS, and recording a grant neither reads a repository nor exercises
  enqueue, worker, provider, Git, or publication authority.
- Repository-grant compare-and-set, Emergency Pause, revision, expiry, and audit
  invariants belong in the sole public mutation primitive, not only in an HTTP
  handler. A separate public raw-insert or test-seeding path can bypass the
  owner-control boundary. Tests therefore seed grants through the same checked
  entry point; any private insert helper runs only after validation and writes
  its redacted audit record in the same transaction.
- Windows is the sole canonical-manifest digest authority for approved-feature
  enqueue. The signed Swift helper validates exact nonzero digest shape but does
  not recompute canonical JSON because Foundation normalizes some numeric JSON
  forms differently from `serde_json` (for example `1.0` versus `1`). Rust
  recomputes the digest before any mutation, and cross-language regressions must
  include a numeric manifest plus a self-dependency rejection case.
- A Windows-only remote mTLS proof can run from a disposable source checkout in
  `%TEMP%` without changing the clean service checkout or the deployed service.
  Remove that checkout after the test, and still treat a schema migration,
  service restart, and live Mac bridge run as separate deployment evidence.
- Rust serializes absent Feature Conveyor guidance identity as explicit JSON
  `null`, while Swift's synthesized `JSONEncoder` normally omits nil optional
  properties. Any Swift snapshot that must preserve the strict Rust allowlist
  therefore uses explicit `encodeNil(forKey:)` calls and an actual
  encode-to-strict-decode round-trip regression. Fixtures built only through
  `JSONSerialization` can mask this cross-language wire mismatch.
- The Mac agent's fixture and MLX lanes are default-off, singleton, and
  no-retention. They add no remote planning, repository, tool, credential,
  network, Codex, Git, publication, or unattended authority.
- Live closeouts (`mac-windows-bridge-live-e2e.sh --run`, `--run-fixture`, and
  `--run-mlx`) are owner-controlled external evidence, not release evidence.
  The base `--run` lane is the native E2E for the read-only Feature Conveyor
  observer: it requires the separately Apple-signed helper, an authenticated
  MacBridge session to the live Windows service, repeated monitor and reconnect
  samples, and a production Swift lifecycle marker bound to
  `feature_conveyor_schema=8`. A successful source pull is not this proof: stop,
  rebuild, and restart the configured Windows release service, confirm its
  health reports the expected schema and a fresh process, then run the Mac lane.
  The owner-token file is in `%LOCALAPPDATA%\Assemblywright\master`; the legacy
  namespace is only a one-time adoption source and must not be used for current
  control-plane requests.
- When the Feature Conveyor response schema advances, update both the Swift
  decoder and every live-harness assertion, then rebuild the separately
  Apple-signed bridge before live proof. A stale signed helper fails closed as
  `invalid_feature_conveyor_status`; source tests or an updated lifecycle marker
  do not refresh that external signed binary.

## Workflow

- `AGENTS.md` holds the agent operating contract. `docs/development-agent-workflow.md`
  holds the role matrix.
- `./scripts/release-local.sh` is the canonical local gate and the default PR
  evidence for executable changes.
- `./scripts/release-docs-drift-smoke.sh` enforces the docs/code contract. When
  a documented command or boundary changes, update the doc and the smoke check
  in the same change.
- Behavior changes include focused tests. Feature slices include relevant docs,
  knowledge-base updates, and E2E coverage.
- Every feature or phase uses the closeout contract in
  `docs/development-agent-workflow.md`: re-check accepted documentation and
  safety rules; capture durable facts from the conversation; apply
  `unit-testing-test-generate` to meaningful unit boundaries; apply
  `e2e-testing` to the real product boundary; run focused, docs, diff, and full
  local gates; obtain risk-proportional independent review; and, when
  publication is requested, verify the pushed commit and hosted gates on
  `origin/main`.
- Playwright, screenshots, visual regression, and cross-browser testing are not
  generic E2E requirements. They apply only when the changed surface is a
  browser UI. Native Rust/Swift HTTP, process, protocol, service, packaged-app,
  and Mac/Windows flows use native E2E. For the Feature Conveyor status slice,
  `master_process_e2e` proves the authenticated loopback HTTP path and the
  Windows-only `remote_mtls_e2e` proves pre-handshake and non-MacBridge denial,
  exact MacBridge read-only success, and designated-owner-only enqueue on the
  enrolled-device router. This native Rust/Swift/process boundary does not use
  Playwright.
- Do not commit or push unless explicitly requested.

## Shell Portability

- Every `scripts/*.sh` runs under `set -euo pipefail` on the macOS system bash,
  which is 3.2. There, expanding an empty array as `"${name[@]}"` aborts with
  `name[@]: unbound variable` rather than expanding to nothing. Bash 4.4+ and
  zsh do not, so the defect never reproduces on a modern shell.
- Use `${name[@]+"${name[@]}"}` for any conditionally-populated array, or guard
  the expansion with an explicit `${#name[@]}` length check. Both forms are
  accepted; anything else fails the gate.
- `./scripts/release-shell-portability-smoke.sh --check` enforces this across
  every tracked `.sh` that opts into nounset, with an allowlist of reviewed
  expansions that carry a written justification. `--self-test` proves the bash
  3.2 behavior itself and that the scanner detects a deliberate violation.
- The contract script scans itself, and it has to contain the forbidden spelling
  in order to document and to test the rule: in the explanatory comment, in the
  `bash -c` string the self-test executes, and in the deliberate-violation
  fixtures. Those three array names are allowlisted individually rather than
  exempting the file, so a genuine unguarded expansion added there still fails.
  Until that was fixed the gate flagged its own source, which meant
  `release-local.sh` was red on `main` from the commit that introduced it.
- This class hides from ordinary testing. A conditionally-populated array breaks
  only the code path that leaves it empty, and a self-test that asserts a
  nonzero exit cannot distinguish an intended failure from an unbound-variable
  abort. Two instances shipped undetected: `--run` and `--run-outage` in
  `mac-windows-bridge-live-e2e.sh`, and `endpoint_args` in
  `release-evidence-doctor.sh --assert-complete`, which would have fired only
  after every evidence check passed.
- A script that fails in one mode but not others is this bug until proven
  otherwise. The first symptom read as the Windows master being unreachable.

## Cross-Language Protocol Version

- `PROTOCOL_VERSION` is declared four times and nothing derives one from
  another: `crates/assemblywright-protocol/src/lib.rs`,
  `apps/mac/Sources/AssemblywrightMacCore/DeveloperBridge.swift`, and
  `$protocolVersion` in both `scripts/windows-*-live-control.ps1`.
- Each language's tests only compare against its own declaration, so a partial
  bump passes every suite. Both halves of that have already shipped. Missing the
  Swift constant produced a live-device handshake rejection *after* mTLS had
  authenticated. Missing both PowerShell control planes made every fixture and
  MLX enqueue fail with `unsupported protocol version: expected 2, received 1`;
  those scripts have no test suite at all, so only an owner-driven live run
  could surface it.
- `./scripts/release-protocol-version-contract-smoke.sh --check` compares all
  four and rejects a hardcoded `protocol_version` literal in either PowerShell
  script, since a literal at a request site drifts independently of the
  declaration the gate reads. It also rejects numeric protocol-version prose in
  the README, architecture map, and release-readiness feature proof; those
  surfaces describe the current contract without duplicating its number.
  `--self-test` proves the comparator and prose scanner against fixtures for
  stale Swift, stale PowerShell, hardcoded request and prose literals, absent
  declarations, and one stale file beside an aligned one.
- The shell self-test is the unit boundary for the comparator and prose scanner.
  The Rust unit test
  `protocol_readiness_proof_is_version_independent` validates the feature
  metadata directly, while `release_readiness_e2e` executes the shipped CLI,
  parses `release readiness --json`, and validates the owner-visible proof.
  Playwright, screenshots, and cross-browser matrices do not apply because
  this surface is a native CLI and has no browser or DOM.
- Both modes run inside `release-local.sh`, so adding them also required
  updating `expected_local_gate_commands` in `release-ci-workflow-smoke.sh` and
  the command list in this repository's build documentation.

## Live Lane Enrollment Topology

- Live lane status as of 2026-07-25: `--run`, `--run-relay`, `--run-outage`,
  `--run-fixture`, and `--run-mlx` all pass against the Windows master.
- The registry now holds two active devices and two active certificates:
  `owner-mac-bridge` uses the standard Keychain profile with the exact singleton
  `mlx.reasoning` capability, and `owner-mac-fixture` retains the separate exact
  fixture profile. The rebind advanced the standard registration to revision 2;
  it did not mutate the fixture profile.
- `--run-mlx` needs the *standard* profile to carry `mlx.reasoning`. The shipped
  repair path is now an explicit owner-confirmed two-phase rebind, not rotation
  or destructive removal. Windows `rebind-pair` snapshots the exact stale
  fixture registration and exact singleton MLX target; schema v6 keeps the new
  certificate outside normal authentication until a staged-certificate
  acknowledgement signed by the exact replacement CSR key is separately
  confirmed. Activation atomically advances the
  registry revision, inserts the replacement certificate, revokes old serials,
  terminalizes the pending evidence, and emits a CA-signed receipt. Exact
  lost-output retries preserve the first activation timestamp; Emergency Pause
  blocks activation. All four rebind transitions commit immutable metadata-only
  audit evidence in the authority transaction. The Mac uses a separate replacement
  Secure Enclave key/certificate slot and changes the selected installed
  generation only after verifying the activation receipt against the staged
  pinned CA. Mac cancel can delete only prepare-only staging; after a signed
  acknowledgement it refuses and retains the replacement key, certificate,
  and receipt so an already-committed Windows activation can be retried and
  promoted. If abandoning after Windows issuance but before Mac staging, abort
  Windows first and then cancel locally. Fixture-profile rebind and general
  standard-profile removal remain forbidden.
- Repository tests alone do not establish live capability repair. The
  2026-07-25 owner/device closeout additionally proved the Xcode-provisioned
  helper staged and promoted a replacement Secure Enclave identity, that the
  replacement authenticated at the higher revision, and that `--run-mlx`
  completed one real local-inference success plus pause-dominated cancellation,
  seven-second late-output suppression, durable cursor restart, and a fresh
  authenticated connection. The exact terminal event sequences were success
  `560/562/563`, cancellation `577/579/580/581/582`, and restart `584`.
  Deterministic tests prove revoked-certificate rejection, but the retained old
  Secure Enclave identity was not directly reused for a negative live
  connection after activation; keep that narrower boundary explicit.
- `enrollment pair` reads the CSR to **EOF**, which an interactive terminal
  never sends. Send the CSR line, then a separate Ctrl-Z (`ASCII character 26`).
  Console line length is not the constraint — 541 characters round-trip intact.
- The fixture lane's separate `EnqueueCancellation` and `Pause` are an operator
  race the fixture job wins, because its synthetic delay is at most five
  seconds. The MLX lane has a combined `EnqueueCancellationAndPause` for exactly
  this reason. Chain both fixture actions into one PowerShell invocation.
- If `Pause` throws, emergency pause stays active and every later run fails with
  "timed out waiting for the exact fixture-profile connection". Run
  `-Action Resume` before retrying.
- An SSH login that presents
  `mike@MIKE-PC C:\Users\mike\Codex\Assemblywright>` is Windows `cmd.exe`, not
  PowerShell. Backticks do not continue a command there, and a literal
  `<printed-id>` is parsed as input redirection, producing "The system cannot
  find the file specified." Invoke the script on one line through
  `powershell -NoProfile -ExecutionPolicy Bypass -File
  .\scripts\windows-mlx-live-control.ps1 ...`, replacing the placeholder with
  the emitted device UUID. The same form with
  `-Action Resume -ConfirmAction` returns the strict
  `mlx_emergency_resumed` receipt.
- Take authenticated Windows health before and after a live fixture closeout.
  A reconnect may expire an abandoned queued fixture from an interrupted prior
  run; accept the closeout only when the final health is unpaused and reports
  zero queued steps, leased steps, and active attempts.
- The harness validates a control receipt's `succeeded_sequence` against the
  agent's own cursor, which is a fresh temporary directory each run. A receipt
  from an earlier run will therefore be accepted. Clear the Windows console
  before each command rather than grepping scrollback.
- Foundation `JSONSerialization` escapes forward slashes unless
  `.withoutEscapingSlashes` is requested, while Rust `serde_json` does not.
  Because Developer Mode hashes sorted JSON bytes, slash-bearing MLX model IDs
  can otherwise make the Mac reject a valid agent result before Windows sees
  it. Use sorted keys plus unescaped slashes for every Swift recomputation of a
  Rust-owned context or payload digest, and retain slash-bearing fixture and
  MLX regression cases.

## Safety Guardrails

- Fail closed. Ambiguity quarantines and blocks rather than guessing.
- Planning and action stay separate. Models propose; the owner authorizes.
- Redaction is structural: audit and event surfaces carry metadata and digests,
  never raw payloads or credentials.
- Cancellation dominates completion and suppresses late output. It also dominates
  cleanup: when a job already has a definite verdict, a slow or unprovable
  process-group reap must not relabel it. The MLX runtime latches
  `cleanup_unproven` instead and refuses new work with
  `mlx_cleanup_unproven` (HTTP 503) until the app-supervised agent restarts, so
  an unproven reap fails closed without turning a cancellation into an internal
  error. Reporting the cleanup failure *as* the job outcome was a real defect: it
  surfaced only under CPU load, as HTTP 500 in place of 409 `job_cancelled`.
- Emergency pause blocks new leases and publication.
- Audit evidence commits in the same transaction as the state transition it
  describes.
- Result acceptance is bound to the exact leased attempt.
- Automatic retry is allowed only when evidence proves repetition cannot
  duplicate an effect.
