# Assemblywright Project Facts

These notes capture durable facts for future agents working on this repository.

## Owner Connection Setup

- Normal Mac product startup uses `developer-bridge-configuration-v1.json` under
  owner-private Application Support instead of requiring hidden launch variables. The
  file contains only the separately signed helper path and independently supplied Apple
  team identifier. Environment configuration remains an explicit development/test seam.
- The setup file is bounded, duplicate-free, no-follow, owner-only, and atomically
  published with file and directory fsync. Unsafe stored state blocks supervision and
  never falls back to ambient environment configuration.
- Every supervision, status, enrollment, and rotation command revalidates the helper's
  Apple signature, fixed identifier, entitlements, expected team, CDHash, and running
  process. The helper remains separately Xcode-provisioned and is not bundled.
- The Mac UI keeps invitation, CSR reply, and certificate receipt text in memory only.
  The raw enrollment or rotation grant stays in the stopped Windows pairing process;
  helper setup, installed identity, authenticated connection, and Windows-authoritative
  projections are separate readiness states.
- Once an enrollment or rotation install command receives its public receipt on stdin,
  cancellation, nonzero exit, timeout, or malformed output is effect-ambiguous and must
  surface forward recovery rather than a fresh-install retry. Pre-stdin helper identity
  rejection remains a specific pre-effect failure.
- After a successful rotation install, retain the validated public `grant_id` in memory
  until the UI renders the exact Windows `rotate-recover-acknowledge` command. Do not
  clear the receipt and replace that binding with a placeholder.
- A configured/paired/connected card is not execution, release, installation, or
  production-readiness evidence. Live pairing and certificate rotation remain separate
  two-device proof boundaries.

## Local Model Selection

- The only owner-selectable model is the designated standard-profile Mac MLX
  model. Local coding and production review are fixed; Windows RTX is not yet
  provisioned.
- Selection changes only the MLX model ID. It does not rotate identity, alter
  provider or bounds, grant capability, or rebind another device.
- Local executable/model paths remain Mac-only in
  `local-model-selection-v1.json` at mode 0600. Windows request, projection,
  receipt, status, and audit are path-free.
- Model IDs are printable non-whitespace ASCII only. Pending persistence fsyncs
  both file and parent directory. The store recursively rejects duplicate or
  unknown nested fields and requires the pending model identity to match the
  canonical frozen request bytes; invalid store state blocks supervision.
- Only strict known pre-mutation HTTP error pairs clear pending as terminal.
  Unknown responses, including every 5xx, remain reconciliation cases, and a
  selected relay validates before active state is persisted.
- A pending selection intentionally stops ordinary supervision. Use the
  separately confirmed exact Resume action; never delete or hand-edit the
  pending file to infer success.
- If a target-profile session is accepted after an ambiguous POST, never probe
  the stale old profile. Emergency Pause keeps pending and local identity inert;
  after unpause, only exact target registry/device/name/model evidence with
  monotonic designation and pause revisions may complete reconciliation.
- The production TLS factory ordinarily accepts only the installed Keychain
  profile. Reconciliation uses a distinct internal factory operation that
  rechecks the frozen old profile/material and permits only its exact derived
  revision-plus-one/requested-model target; never add a broad transition-mode
  flag. Windows must still accept the target application handshake and exact
  projection.
- Repository tests prove strict contracts and native process/mTLS behavior;
  they do not prove signed distribution, notarization, clean-profile install,
  or live-device model execution.

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
- `PROTOCOL_VERSION` is 5. It moved from 1 to 2 because the rename changed wire
  values: the TLS exporter label, the fixture capability provider and model, and
  the certificate subject and SAN URI. Two builds both claiming version 1 while
  disagreeing on those would be mutually incompatible, which is exactly what the
  version field exists to prevent. A pre-rename peer now fails on version. It
  moved from 2 to 3 because `LocalCodingJobResult` replaced the snapshot-only
  receipt with wire-incompatible contained-coding evidence: fixed allowed and
  changed path-set digests, admission and patch digests, changed-file count,
  truthful test status, mutation, workspace-retention, and ambiguity fields.
  It moved from 3 to 4 because the local-coding terminal response is now a
  strict result/artifact pair and the result carries immutable artifact ID,
  exact-byte SHA-256, and size. Version-3 peers must reject this shape. It moved
  from 4 to 5 because the immutable coding packet now carries bounded
  deterministic multi-file write/delete operations, successful workspaces are
  retained with exact expiry evidence, and the canonical artifact/result wire
  contract changed. Version-4 peers must reject this shape.
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

## Approved Full-Machine Assembly Line Target

Full-machine target phase: planning/creation containment has bounded native Windows LocalSystem/AppContainer service proof; execution admission is implemented and effects remain unavailable.

- The approved simple entry points are New Project, New Feature, and Assembly Line.
  The owner-facing repository identity is a canonical GitHub URL; Windows retains an
  immutable internal identity mapped to it.
- New Project defaults to Public, permits Private before approval, and creates the
  GitHub repository only after schema-bound brainstorming and owner review/approval.
  New Feature uses the same planning-only workflow and owner approval before enqueue.
- Orchestrators are explicitly provisioned provider/model pairs, defaulting to
  `openai.codex` / `gpt-5.6-sol`, with no fallback. They plan only and receive no
  execution authority or restricted machine data. This is a built-in typed workflow,
  not restoration of the removed conversation or plugin runtime.
- Starting a nonempty Assembly Line is the execution approval. Auto-run defaults on,
  only one feature is active, and advancement requires exact validation, independent
  review, publication, and final-main verification success.
- Full-machine effects across the registered Windows and Mac hosts are functional,
  brokered authority, not a raw administrator/root token. The protected Assemblywright
  control plane—master/broker identities and code, IPC/configuration, authority data
  and backups, audit, tokens/keys, trust/update roots, enforcement state, and reserved
  enforcement resources—is the sole scope exception and rejects mutation before effect.
- Stop checkpoint-then-terminates tracked executor processes. Emergency Pause
  terminates them immediately. Both preserve the last durable checkpoint, and neither
  claims rollback of completed durable services or external effects. Effect ambiguity
  requires exact adapter reconciliation or owner resolution; it is never blindly retried.
- This is approved target architecture with a reviewed planning/creation source slice
  that remains effect-free on Windows until capability separation is proved. The
  current protocol-v5/schema-v22 master preserves schema-v19 restricted-worker grants,
  queues, and activation meanings until native hostile-boundary tests, deployment, and
  owner-recorded two-host evidence complete an explicit execution cutover.

## Schema-v20 Assembly Line Planning Foundation

- `assemblywright-protocol` keeps protocol version 5 and adds independent strict
  schema-v1 types for canonical GitHub URL identity, orchestrator-catalog binding,
  brainstorming drafts/frozen specifications/owner approval, repository lifecycle,
  FIFO projection, default-on auto-run, sessions/child epochs, control receipts, and
  termination evidence. A valid type is not proof that its runtime component exists.
- The Windows master migrates backup-first from schema 19 to schema 20 and preserves
  every legacy Feature Conveyor table and meaning. New Assembly Line tables and
  authenticated owner/designated-Mac routes persist planning state only; they do not
  write legacy dispatch/effect tables.
- New Project defaults to Public, permits a frozen Private choice, and uses a strict
  Public-only cloud-disclosure wrapper. Source implements the fixed provider, frozen
  approval, intent-before-effect, and exact GitHub reconciliation. Windows loading is
  fail-closed pending distinct restricted planning and GitHub identities/ACLs, so the
  deployed target still performs no GitHub effect.
- New Feature authoring and queue insertion require the exact repository lifecycle to
  be `created` and creation evidence to be present. A test-only database transition is
  used to exercise that downstream gate; it is not a production GitHub completion
  writer. Accepted features append FIFO and do not dispatch work.
- Auto-run defaults on. The only implemented mutation is exact replay-safe CAS against
  the state revision. Start, Stop, and Emergency Pause routes are deliberately absent,
  so the setting cannot create a session, child epoch, or executor effect.
- The owner projection reports brainstorming provider, GitHub creation, Windows
  executor, Mac executor, and protected brokers as `unavailable`. The Mac has strict
  projection/mutation transport and a simplified New Project/New Feature/Assembly Line
  view. The queue and auto-run setting are authoritative; frozen planning approval and
  ambiguous outcomes use durable exact signed-helper reconciliation. Execution actions
  remain disabled.
- The master library has a Windows-catalog-bound, idempotent planning/GitHub
  coordinator, fixed tool-free Codex adapter, exact GitHub adapter, cancellation/pause
  fencing, and atomic frozen-specification/provenance storage. Unit tests and a real
  adapter process E2E cover the source path. Windows runtime loading returns
  unavailable until its capability separation is provisioned and proved.
- The brainstorming Codex adapter remains planning-only with all tool surfaces
  disabled. On Windows its inner sandbox is `danger-full-access` because the actual
  isolation boundary is Assemblywright's restricted-token AppContainer, exact ACL
  tree, closed environment, pinned executable, kill-on-close Job, and a per-call
  create-only private window station/desktop admitting only LocalSystem, the
  provisioning owner, and the exact profile. The master does not widen `Winsta0` and
  restores its original station before child creation. Windows Codex receives
  `LOCALAPPDATA` and `TEMP`/`TMP` only as separate validated provider-private writable
  roots; ambient owner paths are not forwarded. Non-Windows
  planning keeps inner `read-only`; the Windows label grants no host or implementation
  authority.
- Windows planning schema v4 separates the master-private `DataDir` locator/config from
  the effect packages. Provider, GitHub, and staged-check files are rooted only below
  canonical Common Application Data at
  `Assemblywright/planning-runtime/<runtime_instance>`. Provisioning preserves unrelated
  shared-parent ACL entries while merging exact non-inheriting traverse ACEs for the two
  profiles on canonical Common Application Data itself, the vendor directory, and the
  planning namespace. The known-folder ACE is required because a LocalSystem-derived
  restricted AppContainer does not inherit the ordinary Users traverse grant; without
  it, `CreateProcessAsUserW` fails closed with file-not-found before opening the provider.
  AppContainer profile registration is also scoped to the current Windows identity:
  registering the fixed profile while provisioning as the owner does not register it
  for the LocalSystem service. The master therefore verifies the deterministic expected
  SID before idempotently registering the fixed profile in its own identity namespace;
  SID mismatch rejects before profile creation. Missing LocalSystem registration is an
  independent file-not-found cause at `CreateProcessAsUserW`.
  Load and every call reject path, identity, reparse, missing-ACE, or broader-rights
  drift. Schema v3 is not accepted.
- A Windows provider launch that sets `STARTF_USESTDHANDLES` must supply valid inherited
  stdin, stdout, and stderr handles. The adapter drains stderr concurrently to avoid a
  blocked child but discards its bytes within the fixed cap; provider stderr content is
  never returned, audited, or included in diagnostics. Windows extended-length paths
  remain authoritative for held-path verification. Immediately before the shared
  `CreateProcessAsUserW` call, only an exact extended-length local-drive working
  directory is copied into ordinary drive form for the child; the canonical invocation
  path is not replaced, and UNC or volume-GUID paths are unchanged. Environment values
  consumed by Codex use the same bounded local-drive normalization.
- The native Windows planning service E2E installs a disposable LocalSystem master,
  launches the synthetic Codex fixture inside the fixed provider AppContainer, drains
  32 KiB of fixture stderr, verifies the planning response, and removes only its exact
  disposable service and data roots. It keeps the production service untouched. This
  is stronger than source-contract or direct-loopback evidence but does not prove real
  Codex authentication, network/model behavior, production deployment, or GitHub effects.
- The real-Codex planning diagnostic is deliberately outside the serving product path.
  An elevated owner-confirmed script stops the exact installed service; the master
  independently verifies the stopped SCM state, installed executable/data binding,
  and current schema-v4 runtime before invoking exact `codex login status`. Only a
  successful login permits one fixed Public structured request through the unchanged
  provider restricted-token AppContainer/private-desktop/Job boundary and exact five-
  variable Codex environment. Its metadata-only receipt carries fixed categories,
  numeric codes, hashes, and binding identities, never process text, paths, prompts,
  tokens, credentials, or specification content. The script restores and health-checks
  a service that it stopped. This is exact-run diagnostic evidence, not provider
  availability, repository creation, publication, release, or usability proof.
- Codex `login status` can exit successfully while writing the account result to stderr
  and leaving stdout empty. The native boundary therefore permits empty stdout only for
  the exact pinned login-status argv after an exit-code-zero process result; structured
  execution output remains nonempty and strict. Stderr stays bounded, is reduced to a
  closed numeric classification when needed, and is zeroized rather than retained.
- Continuous probe authority must distinguish immutable authority from provider-owned
  writable runtime state. Rewalking the complete profile every 25 ms raced legitimate
  Codex writes and could cancel a valid call. The proven boundary performs full profile
  revalidation before launch and after process completion, while the in-flight poll
  checks only the exact stopped SCM configuration/current identity and the durable
  unpaused revision. Any of those authority changes still terminates the Job and
  suppresses output.
- The 2026-09-02 owner flow is live external-effect evidence for one bounded candidate,
  not deployment evidence. The native real-Codex receipt succeeded with output digest
  `69d4374ec77d76b92afec52fdbe05ab06b3bee8085aa4ba308ea6d2775701417`.
  A separately owner-approved Assemblywright request then created the Public repository
  `malak333/final-fantasy-tactics-site`; its frozen specification digest was
  `4399b09c1852315edaffbe2321c0ecd5fbcae572f281246945e7847257ad817c` and its
  creation-evidence digest was
  `27471316866f06f81c77d2bf9a63b2253ee622ef188539ef6aa2a9f6bf2f6515`.
  The exact prior production executable was restored and health-checked afterward.
  This proves that single brainstorming/approval/creation run only; it does not prove
  that the candidate is deployed, that arbitrary repositories are supported, or that
  execution, signing, installation, and two-host release evidence exist.
- Windows AppContainer write access also depends on mandatory integrity control, not
  only the DACL. The provider's four writable roots and descendants therefore require
  exact Low/No-Write-Up labels, while immutable assets and the provider root must remain
  unlabeled. Provisioning resumes only absent/exact partial migration, holds identity-
  bound no-delete handles during label mutation, and revalidates the profile after each
  contained process before accepting output.
- The native loopback HTTP E2E drives authenticated Public-only provider disclosure,
  exact replay, durable intent, GitHub create/reconciliation, and fixed redacted error
  boundaries with synthetic process adapters. The Windows-only mTLS E2E separately
  proves that only the
  designated Mac bridge can reach the distributed planning surface.
- Repository tests do not prove live Windows migration/deployment, provider calls,
  GitHub creation through a production adapter, live Windows execution of the Mac
  owner action, signed/notarized Mac behavior,
  executors, privileged brokers, termination, full-machine effects, or two-host proof.

## Schema-v22 Fail-Closed Execution Activation Controller

- The Windows master migrates backup-first from schema 20 or 21 to schema 22 and preserves
  planning and legacy Feature Conveyor meanings. It durably records provisioned
  execution capabilities, Start sessions, one FIFO child epoch, authority revisions,
  action/checkpoint evidence, Stop/Emergency Pause intents, termination evidence, and
  replay-safe control responses. Schema 22 adds `starting`, immutable once-only effect
  dispatch claims, and capability-pinned signed activation receipts.
- Start requires a nonempty queue plus exact state, queue, emergency-pause, owner
  approval, and healthy capability bindings. It remains `starting` until exact Windows
  and macOS Ed25519 activation receipts verify. Stop and Emergency Pause durably revoke
  authority before claiming and dispatching termination. Restart never retries an
  ambiguous effect: effect-possible work is quarantined for reconciliation.
- Authenticated local and distributed Start, Stop, and Emergency Pause routes now call
  an explicit effect-dispatch interface. Production installs an unavailable dispatcher,
  so those routes return not-found without decoding or mutating a request until a real
  authenticated host runtime is provisioned.
- The strict Swift owner projection recognizes both queue and Assembly Line `starting`
  states only when Windows executor, Mac executor, and protected-broker availability are
  all authoritative and available. It rejects `starting` when any execution component is
  unavailable, while the product controls remain visibly disabled until that runtime exists.
- The broker and executor binaries now implement bounded inherited length-framed stdio
  runtimes with held no-follow configuration, exact binary/config digests, signed
  authority/session/replay validation, and fail-closed quarantine. They expose no
  ambient listener. Broker effects and executor activation receipts remain unavailable,
  and no four-host installation exists; therefore a genuine two-host Start and the Mac
  execution buttons remain unavailable.

## Current Crate Boundaries

- `assemblywright-protocol` — versioned, bounded wire contracts. No I/O, no state.
- `assemblywright-executor` — inert-by-default unprivileged full-machine containment
  source. It verifies exact signed action/identity/digest/deadline bindings, rejects
  protected paths and replay/gaps, and validates held macOS executable/cwd plus signed
  parent/object identities. Mac spawn is disabled because process groups are escapable;
  Windows source retains no-reparse path handles, verifies the suspended image, assigns
  a kill-on-close Job, and rechecks authority before resume. Its inherited-pipe runtime
  accepts exact signed authority/control frames, supports one active action, and signs
  termination receipts. It has no installed service/client and emits no activation
  receipt, so product execution cannot be activated.
- `assemblywright-broker` — inert-by-default privileged-broker policy and closed adapter
  source. It binds all named protected-control-plane categories into one digest and
  rejects protected descendants, case/link/reparse ambiguity, drift, replay, and
  unknown operations. Its inherited-pipe runtime authenticates exact signed FIFO control
  frames and quarantines ambiguity while keeping production dispatch effect-disabled.
  A dedicated native proof seam implements only Windows `CreateDirectory`: it retains
  every canonical ancestor without delete sharing, creates one absent leaf relative to
  the held parent, returns path-free applied evidence, and quarantines a typed
  reconciliation-required outcome after any uncertainty once the native create call is
  entered; even a negative native status is not treated as proof that no effect occurred. Replace/remove/
  service adapters remain closed. No root/SYSTEM broker service or master IPC client is
  installed. The required Windows hosted gate lints and tests both broker and executor
  packages, so the native create-directory and Job Object E2E suites run for every pull
  request and `main` push; this is exact-commit hosted Windows evidence, not production
  identity, installation, deployment, or live two-host proof.
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
  recursive duplicate rejection, and a redacted bound receipt. Feature 8 adds a
  typed SwiftUI authoring form rather than another authority path: it builds the
  fixed review-safe manifest and exact request from owner-entered fields,
  inserts the 13-command validation gate, binds the current authenticated
  queue/designation/pause snapshot, then stops observation and revalidates the
  independently signed helper before the explicit one-shot action. Swift emits
  deterministic string-only manifest JSON for usability, but Windows still
  recomputes the canonical digest and alone accepts the enqueue. The form
  creates no grants or proof and has no claim, dispatch, repository, provider,
  Git, publication, or activation authority. The first production form pins
  `openai.codex` / `gpt-5.6-sol`, rejects embedded secret-shaped strings, and
  displays the frozen approval bindings. If the helper loses a response, the
  app blocks new submissions and retains identical request bytes only in memory
  for another explicit confirmation. The Windows kernel replays the original
  receipt without mutation or a second audit only under exact original
  queue/specification/dependency/lifecycle/grant/designation/pause/audit state;
  any drift rejects.
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
- The Windows-local repository preflight is an owner-token-authenticated,
  point-in-time admission check, not repository registration persistence or a
  reusable execution snapshot. Its strict scope document must canonically hash
  to the exact current active registration grant and bind the current grant and
  Emergency Pause revisions, absolute repository path, single-component base
  branch, and HEAD.
  Inspection executes no Git process and loads no repository configuration or
  attributes. It reads only a canonical fixed-local-volume standard `.git`
  shape, symbolic HEAD, and exact loose branch ref, and rejects UNC, device,
  mapped/non-fixed-volume, reparse, worktree, submodule, detached, mismatched,
  symlinked, non-Git, stale, revoked, expired, or paused state. Windows holds
  non-reparse handles for the complete fixed-volume identity chain, reopens and
  compares the canonical pathname and identities immediately before the final
  grant, pause, and audit transaction recheck, and retains the fresh handles
  through that transaction. This is still point-in-time evidence rather than a
  cross-filesystem-and-SQLite snapshot.
  Its five-second timeout bounds each filesystem-observation await, not
  owner authentication or database lock/audit latency; a blocked filesystem
  thread cannot be forcibly cancelled.
  It does not
  prove clean-tree, index, object, or source-content state. The
  path and repository content are neither stored nor returned. Success rechecks
  authority atomically, appends only a structurally redacted audit, and returns
  a path-free digest receipt. A later claim must inspect again against a durable
  snapshot; this receipt grants no dispatch, mutation, review, or publication.
- A primary development checkout with a real `.git/worktrees` control directory
  is intentionally ineligible for repository preflight even when its current
  branch and HEAD are otherwise exact. Do not prune or delete valid worktree
  metadata merely to obtain a receipt. Use a dedicated standalone checkout with
  independent `.git` metadata for a positive preflight, then record the next
  revoked registration-grant revision before removing any disposable proof
  checkout. The revoked projection must report `active:false`.
- On Windows, `std::fs::canonicalize` may return a verbatim `\\?\` path that the
  public scope contract correctly rejects. Native fixtures and operator tooling
  that need the accepted canonical DOS spelling should derive it from the held
  directory handle with `GetFinalPathNameByHandleW` and strip only the verbatim
  DOS prefix; do not weaken UNC or device-path rejection. Rust's Windows rename
  path may also fall back to POSIX rename semantics, so share modes are not an
  immutability claim. The safety invariant is the final reopen and full identity-
  vector comparison while both original and fresh handle sets remain held.
- Windows is the sole canonical-manifest digest authority for approved-feature
  enqueue. The signed Swift helper validates exact nonzero digest shape but does
  not recompute canonical JSON because Foundation normalizes some numeric JSON
  forms differently from `serde_json` (for example `1.0` versus `1`). Rust
  recomputes the digest before any mutation, and cross-language regressions must
  include a numeric manifest plus a self-dependency rejection case.
- A human can onboard an existing standalone Windows repository with
  `windows-repository-onboarding.ps1`: `Plan` returns a path-free plan ID,
  `Approve` requires three explicit grant confirmations and returns a compact
  `repository_onboarding_ready` receipt, and `Check` reports whether the exact
  revision-1 grant set and receipt remain current. Pasting that receipt into the
  Mac approved-feature form fills repository identity and grant revisions only.
  This removes manual transcription but does not create the repository or grant
  enqueue, dispatch, mutation, validation, publication, or activation. A new
  repository must first have a committed clean `main`; general arbitrary-project
  dispatch remains unimplemented.
- Plan expiry narrows repository onboarding rather than making current state
  unreadable. `Check` still reports the exact grant/receipt state. A fully
  confirmed expired `Approve` may only finish an exact plan-bound partial or
  complete revision-1 grant set through preflight and receipt publication, or
  replay the exact stored receipt while all three exact grants remain current;
  it cannot start from zero grants. Foreign, drifted, inactive, or revoked state
  always fails closed.
- `windows_repository_onboarding_e2e.rs` is the Windows-native real-boundary
  regression for onboarding. It starts a disposable real master, invokes the
  PowerShell client against an ordinary temporary Git `main`, proves that fewer
  than three confirmations and post-plan drift create no grants or audit,
  validates expired read-only inspection, fresh-expiry rejection without audit,
  exact partial and complete-without-receipt forward recovery, the exact grant/preflight/path-free receipt
  bindings, and exact expired replay without duplicate audit. It does not prove Mac receipt import,
  feature enqueue, dispatch, source mutation, or a deployed Windows service.
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
  `feature_conveyor_schema=9`. A successful source pull is not this proof: stop,
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
- The remote health `schema_version` is the durable Windows master database
  schema, while the nested Feature Conveyor `schema_version` is the independent
  observer projection contract. Validate both strictly but never require them
  to be equal: the schema-v11 service intentionally serves the unchanged
  schema-v8 Conveyor projection. The live signed-helper monitor is the required
  regression for this 11/8 boundary.

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

- `PROTOCOL_VERSION` is declared five times and nothing derives one from
  another: `crates/assemblywright-protocol/src/lib.rs`,
  `apps/mac/Sources/AssemblywrightMacCore/DeveloperBridge.swift`, and
  `$protocolVersion` in the three `scripts/windows-*-live-control.ps1`
  controllers.
- Each language's tests only compare against its own declaration, so a partial
  bump passes every suite. Both halves of that have already shipped. Missing the
  Swift constant produced a live-device handshake rejection *after* mTLS had
  authenticated. Missing the PowerShell control planes made every fixture and
  MLX enqueue fail with `unsupported protocol version: expected 2, received 1`;
  those scripts have no test suite at all, so only an owner-driven live run
  could surface it.
- The 2-to-3 bump is required by the wire-incompatible `LocalCodingJobResult`
  expansion for the fixed contained-coding fixture. Version 3 peers must reject
  version 2 at the handshake rather than interpreting a snapshot-only receipt
  as mutation evidence.
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
- An expired standard MacBridge certificate is repaired with the secret-free
  same-device `enrollment rotate-pair` ceremony, not destructive Keychain
  deletion or a new enrollment. Windows keeps the raw rotation grant only in
  the stopped pairing process; Mac `enrollment rotate prepare|install
  --confirm` reuses the currently selected non-exportable key and accepts only
  the exact installed device, registration, capabilities, endpoint, pinned CA,
  and `rotate` receipt. Successful issuance revokes old serials while preserving
  the device ID, registry revision, capabilities, and owner-control designation.
  Windows precommits the exact public receipt to an explicitly protected
  owner-and-LocalSystem journal. If output is lost, use confirmed
  `rotate-recover --grant-id UUID` repeatedly until Mac installation and
  authenticated reconnect succeed; then use confirmed
  `rotate-recover-acknowledge --grant-id UUID` to remove only that validated
  journal. Do not generate a second grant to recover ambiguous output. Mac
  installation is forward-resumable across Keychain mutation boundaries.
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
- Windows PowerShell 5 promotes a native process's stderr records under
  `$ErrorActionPreference = "Stop"`, while `git clone` writes ordinary progress
  to stderr even when it succeeds. The local-coding live controller temporarily
  captures clone stderr under `Continue`, restores the fail-closed preference,
  and treats the exact native exit code as the sole clone verdict. Do not weaken
  error handling for the later HTTP or filesystem authority checks.
- A local `git clone --no-hardlinks` can copy `.git/objects/info/commit-graphs`
  from the source checkout. The Rust snapshot reader intentionally rejects that
  nested object-store directory. The live controller therefore uses
  `git clone --no-local`; recovery of an already marker-bound disposable clone
  removes only a strictly named, regular-file, non-reparse commit-graph cache
  (admitting Git's 40-hex SHA-1 and 64-hex SHA-256 graph names) and verifies
  the remaining flat object-store shape before claiming. It never rewrites the
  source checkout or committed proof tree.
- The production authenticated bridge rejects overlapping requests. Local-coding
  snapshot transfer and cancellation polling are concurrent tasks, so both must
  use the relay's FIFO session permit. Fake-session coverage must deliberately
  suspend and reject overlap; an actor fake with no suspension can otherwise
  serialize accidentally and miss the production `requestInFlight` failure.
- Cancellation of a queued local-coding request does not consume the session
  permit or reach the authenticated channel. When that waiter reaches the FIFO
  head, it observes cancellation and hands the permit directly to the next
  waiter. Unit coverage must retain a suspended first request, a cancelled
  second waiter, and a successful third request so this handoff cannot strand
  the relay.
- Foundation may serialize UUID values with uppercase hexadecimal while Rust
  and Windows receipts use lowercase canonical UUID text. Compare parsed UUIDs
  whenever possible; shell proof harnesses must lowercase both sides before a
  textual identity comparison. Case normalization changes presentation only,
  never the UUID bytes used by protocol digest transcripts.
- A signed-helper snapshot is different from an internal parsed-UUID
  comparison: its JSON is a strict app wire boundary. Every nested
  owner-control type containing a UUID must custom-encode lowercase canonical
  text. Regression data must include alphabetic hexadecimal digits and pass the
  complete production `JSONEncoder` snapshot through
  `AssemblywrightDeveloperBridgeProcessLifecycle.status`; digit-only UUIDs can
  hide Foundation's uppercase re-encoding and previously produced
  `invalid_helper_snapshot` only after live evidence admission.
- Interactive SSH pseudo-terminals can impose canonical input-line limits on
  long pasted JSON or controller commands. Keep authority documents in bounded
  files or stdin and pass only their validated paths/IDs on the command line.
  If an exact long-line interactive transfer is unavoidable, disable canonical
  input and echo for that bounded transfer, then restore terminal settings;
  terminal scrollback is never proof evidence.
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
- Schema v9 is the first durable snapshot-bound activation slice. The owner
  loopback request binds the exact queue head/specification, canonical
  registration scope, branch/base commit, provider/model, all three current
  grant revisions, queue revision, and Emergency Pause revision. SQLite is not
  held during snapshot construction; the finalizer atomically repeats every
  authority check, stores only snapshot ID/digest/base commit and revisions,
  inserts the singleton lease, advances lifecycle/queue state, and appends
  redacted audit.
- Snapshot construction uses libgit2 raw object-database reads without a Git
  subprocess or source repository/config checkout. It rejects alternates,
  symlink/reparse entries, gitlinks, unsafe/case-colliding Windows paths, and
  invalid or oversized object graphs; materializes raw committed blobs into an
  independent repository with no remote, hardlinks, source path, credentials,
  or active hooks; and binds a separate SHA-256 content digest. Only the exact
  base commit plus its current tree/blob graph is copied. The base is marked
  shallow, so parent commits and deleted historical objects are absent rather
  than exposed through the destination ODB. Each commit/tree/blob header is
  type- and declared-size-checked and charged once to the aggregate budget
  before its body is decompressed or allocated; materialization rechecks the
  header without double-charging already copied objects. A prepared snapshot is
  built directly in one private random-UUID final directory and remains
  unreferenced until the later SQLite finalizer commits its digest binding.
  This avoids a Windows directory-rename failure caused by libgit2/antivirus
  handle sharing; RAII and startup cleanup remove every unreferenced partial or
  completed UUID directory.
  Windows also requires a writable file handle for `sync_all`/
  `FlushFileBuffers`. Close the destination libgit2 ODB/repository handles
  before opening the generated object files for that flush. Libgit2 marks loose
  objects read-only, so the Windows flush helper temporarily clears that bit,
  flushes through a writable handle, and restores it; otherwise Win32 returns
  access denied even though the snapshot builder owns the files. Fail-closed
  RAII and startup cleanup likewise clear the bit only on ordinary
  non-reparse files immediately before deletion and best-effort restore it if
  deletion fails.
- Snapshot claims are process-wide singleton work. Concurrent requests fail
  immediately. The raw-object task is limited to 50,000 objects, 256 MiB total,
  32 MiB per blob, and a 30-second HTTP wait. Timeout detaches rather than
  force-cancels the blocking thread; that thread keeps the reservation and
  RAII cleanup ownership until it exits.
  This does not
  grant worker dispatch, provider/review invocation, GitHub publication, Mac
  mutation, or autonomous activation.
- Windows validation intentionally uses package-scoped clippy for
  `assemblywright-protocol` and `assemblywright-master`, matching
  `.github/workflows/windows-protocol.yml`. A workspace-wide Windows clippy run
  also selects Unix-only local transport code and can fail before exercising
  the authoritative Windows crates; keep workspace-wide clippy in the canonical
  macOS/Linux release gate instead.
- A prepared snapshot is RAII-owned until the database finalizer commits. HTTP
  cancellation, finalizer rejection, or any other precommit failure removes
  it and leaves no lease. Startup removes unreferenced UUID
  directories; a finalized `implementing` lease still becomes quarantined with
  no automatic retry. The schema-v8 observer projection remains wire-compatible
  even though the durable database is schema v11.
- Schema v10 is the first metadata-only coding-dispatch admission slice. The
  explicit owner-token loopback route accepts no repository path, bytes,
  command, prompt, credential, patch, or commit. It binds one work-packet
  digest and bounded ordinal/acceptance-count metadata to the exact active
  feature/specification/lifecycle revision, feature lease, snapshot ID/digest,
  queue and Emergency Pause revisions, and a current non-revoked
  `InferenceWorker` registration whose only capability is `local.coding.v1`.
  One distributed queued step, immutable dispatch row, event, and redacted
  Feature Conveyor audit commit atomically.
- A queued coding dispatch is not reusable authority. Lease and acknowledgement
  acceptance recheck the feature lifecycle and lease, snapshot, queue/pause,
  device registration, and capability. Explicit cancellation, Emergency Pause,
  lifecycle departure, registration drift or revocation, and restart quarantine
  make it ineligible; a pre-pause or out-of-phase acknowledgement must never be
  accepted after state returns to a superficially compatible condition.
- The default-off local-coding snapshot lane begins only after the exact leased
  path-free metadata envelope. Its distributed request and response bind the
  job, attempt, lease, cancellation, snapshot ID/digest, sequential offset,
  bounded total, chunk digest, and completion state. The Windows master
  reauthorizes before and after every snapshot-bundle filesystem read; stale
  authority never becomes reusable byte access.
- The signed Mac bridge selects this lane only through the exact
  `ASSEMBLYWRIGHT_MAC_DEVELOPER_LOCAL_CODING_SNAPSHOTS_ENABLED=true` opt-in with
  both agent executable and agent data-directory settings present. Fixture,
  MLX, and local-coding snapshot lanes are mutually exclusive.
- The production Swift supervisor must not request the MacBridge-only Feature
  Conveyor projection for the separate local-coding `InferenceWorker` identity.
  It validates the exact singleton `local.coding.v1` profile and required relay
  before connecting, then authenticates, health-checks, and leases without
  requesting the MacBridge-only event stream or Conveyor projection and emits
  neither owner observation. Partial, mixed, drifted, or relayless worker
  profiles fail before network use; standard and fixture MacBridge observation
  remains strict and unchanged.
- Production enrollment uses `--identity-profile local-coding`, a separate
  Secure Enclave/Keychain service, key tag, certificate label, and installed
  profile. It accepts only `inference_worker` with the exact singleton
  `local.coding.v1` descriptor. Standard/fixture identities remain
  `mac_bridge`-only, local-coding rebind is forbidden, and the process launcher
  selects this identity only for the local-coding snapshot opt-in.
- Protocol v5 raises that exact local-coding descriptor's context limit from
  the historical 8 KiB to 12 KiB. The production Swift enrollment and
  supervisor validators must match the Rust protocol descriptor exactly; an
  enrolled v4 worker is intentionally rejected and must be revoked and enrolled
  again through the supported local-coding profile ceremony before live v5 work.
- The Windows live controller must send the complete v5 work packet, including
  sorted `allowed_paths` and exact tagged write/delete arguments, and bind
  `work_packet_sha256` to its recursively sorted compact JSON bytes. The former
  v4 three-field packet and domain-string digest are rejected after a snapshot
  claim and must not be retained as a live-controller fallback.
- Historical protocol-v4/schema-v12 fixture behavior (superseded for new v5
  jobs by the general-worker rules below): the Mac bridge strictly validates
  every chunk and forwards it over the authenticated owner-only Unix socket. The Rust agent writes through one
  retained no-follow file descriptor, verifies the deterministic bundle and
  raw Git object IDs, rejects links, traversal, reserved/case-colliding or
  duplicate paths, reconstructs only safe regular/executable files with
  independent shallow Git metadata and no remote/hooks, and recomputes the
  original snapshot SHA-256 while enforcing an aggregate materialized-output
  byte budget. It then forks one fixed child from the
  already-running process with no `exec` or
  remote input. Before `fork`, the parent pre-opens the workspace, blocks
  signals, and captures the descriptor-table bound and effective UID.
  Swift launches the agent with an empty environment and the agent refuses this
  lane if it observes a nonempty parent environment. The child scans every
  descriptor slot with `F_GETFD`, closes every open descriptor except the
  workspace and group gate, and waits for the parent-established process group.
  Its fixed path opens and validates only relative `README.md`, then truncates,
  seeks, writes fixed bytes, syncs, closes, and exits. It does not inspect errno
  or mutable global state, use environment APIs, or call `geteuid`,
  `getdtablesize`, or `setpgid` post-fork. The parent rejects every other
  changed path or unexpected output.
- In that historical fixture, `contained_coding_completed` was a bounded
  path-free result: it binds work packet, admission, snapshot, fixed
  allowed/changed path set, and patch
  digests; reports exactly one changed file, `test_status:not_run`, mutation
  performed, workspace not retained, and ambiguity false. Both the materialized
  repository and transfer staging directory are removed before this result is
  returned. The accepted distributed state retains the payload digest rather
  than paths, source, output, or raw errors.
- The admission digest is not an arbitrary nonzero marker. Its protocol-owned
  transcript is the domain `assemblywright.local-coding-admission.v1\0`, protocol
  version as big-endian `u16`, the raw 32-byte context digest, raw network-order
  task/step/attempt/lease/cancellation UUID bytes, then connection epoch,
  sequence, lease duration, and deadline as big-endian `u64`. The Rust
  producer/master and Swift relay recompute that exact transcript.
- Cancellation during native verification or child execution sets an atomic
  stop request, kills and reaps the child when present, waits for bounded
  checkpoints and cleanup, and emits `cancelled` only after the agent removes
  active attempt state. The production Swift-to-real-agent negative E2E delivers
  authoritative cancellation during final verification, sends local
  cancellation before cancelling the blocked Unix request, observes an empty
  attempt root before forwarding the acknowledgement, posts no result, and
  requires acknowledgement strictly inside two seconds. The relay records the
  cancellation-in-progress state before invoking the agent: if the cancelled
  final-chunk request returns its expected rejection just before the agent's
  acknowledgement, the relay waits for and validates that acknowledgement;
  the same rejection without an in-progress cancellation still fails closed.
  Deadline/lease loss,
  Emergency Pause through durable
  cancellation, failure, shutdown, and startup likewise suppress late results
  and leave no partial attempt state. Master bundle reads retain
  component-by-component no-follow/no-reparse directory handles plus the
  single-link bundle handle until the bounded read completes, preventing an
  intermediate path replacement from redirecting authority.
- This is ephemeral Windows-to-Mac snapshot replication plus one deterministic
  contained-coding fixture. Dispatch approval cannot select a command, tool,
  executable, path, provider, or test, and grants no credential or network
  access. The forked-child boundary does not prove an OS sandbox or
  host-level egress enforcement.
  Repository, protocol, Windows master-route/mTLS, agent-library, and real UDS
  process seams are covered, and the canonical Mac native E2E now drives the
  production Swift relay and code-identity launcher against the real supervised
  Rust agent. It is still not one live two-device transfer, a retained coding
  workspace, arbitrary implementation, test execution, patch/result
  integration, review, publication, or autonomous activation.
- The 2026-08-11 contained-coding source closeout was validated by the full Mac
  release gate, independent high-risk review, native Rust UDS/process E2E, and
  the production Swift-to-real-agent cancellation E2E. A disposable Windows
  worktree based on `e0ba81f` also passed protocol/master Clippy, protocol
  contracts, the Feature Conveyor kernel, master-process E2E, remote-mTLS
  coding dispatch, and the master build. That worktree and its transfer patch
  were removed after proof; the installed Windows service was deliberately
  left clean on schema 10/protocol 2 at `e0ba81f`. Protocol 3 deployment must
  therefore be coordinated with rebuilding the Mac helper rather than
  upgrading only one peer and breaking version negotiation.
- Schema v11 now exposes owner-token, loopback-only, strict bounded
  `cancel-active-feature` and `abandon-and-advance` routes. Both compare-and-set
  exact feature, lifecycle, queue, and Emergency Pause revisions inside the
  authoritative transaction and are absent from enrolled-device mTLS.
  Cancellation cancels exact coding dispatches but retains the feature lease
  and cannot advance. Abandonment accepts only cancelled or quarantined state,
  requires a nonzero safe-reconciliation digest, derives merge necessity from
  immutable active-origin transition evidence, and requires verified healthy-
  main evidence when that origin is `verifying_main`, then releases the lease
  and increments the queue revision. The backup-first v10 migration backfills
  a missing retained-lease origin receipt only from one exact lifecycle-bound
  append-only cancellation or startup-quarantine audit event; missing,
  ambiguous, malformed, or non-active-origin evidence fails closed and restores
  the verified v10 database. Their owner-control documents remain schema 1,
  independent of the schema-v8 Feature Conveyor observation projection and the
  schema-v11 master database. Their receipts are fixed and path-free. A live closeout must use
  these routes and prove the feature queue, lease, distributed attempt, transfer
  staging, and Mac workspace are empty; direct SQLite mutation or deleting
  state as proof remains forbidden.
- The 2026-08-11 owner-coordinated two-device contained-coding proof completed
  against the deployed Windows schema-11/protocol-3 service. The marker-bound
  snapshot source was `80fed217`; the production Apple-development-signed Mac
  helper was built from `01afff03` and had binary SHA-256
  `0cbb9e85e445936b5b276c59210719f0ee5f5bc7e805be65fb2bc2d446d77aa5`.
  After fail-closed recovery exposed the copied commit-graph cache and the
  overlapping-request bug, the real `local-coding` identity and supervised
  Rust agent completed task `738cc025-df36-4bbb-94e6-092e3d4f0cf5`, step
  `eb694c9d-a837-4e91-9723-41c0bfbddd18`, with final lease/success sequences
  3824/3825 on connection 1837. Windows terminal steps advanced 15 to 16 with
  zero queued, leased, or active work; both Mac snapshot directories and
  Windows staging were empty. Owner cancellation advanced lifecycle 2 to 3
  while retaining the lease; abandonment advanced 3 to 4, released it, and
  produced safe-reconciliation digest
  `9a48d01079760733a3924bdbf3bf944cc7ba6676e353497d74774ff9df778a44`;
  cleanup revoked all three grants and removed the exact disposable checkout.
  This is functional live proof, not Developer ID distribution, notarization,
  clean-profile installation, Finder launch, or arbitrary coding authority.
- Validate bundle manifest paths from the raw UTF-8 bytes before constructing a
  platform path. `PathBuf` may normalize repeated or boundary separators, so
  traversal, empty segments, `.git`, Windows-reserved names, invalid UTF-8,
  trailing dot/space, overlength names, and case collisions need direct unit
  coverage rather than relying on later filesystem calls to fail.
- The 2026-08-11 snapshot-replication implementation reached `main` and the
  live Windows `AssemblywrightMaster` service at `94a98a0`. The Windows native
  gate proved exact local-coding enrollment and bounded snapshot transfer over
  mTLS; the Mac native seams proved deterministic Git materialization, strict
  identity and digest binding, cancellation cleanup, startup recovery, and
  production Swift-to-real-agent supervision. This remains repository and
  deployment proof, not a signed/notarized clean-profile release or owner-run
  live two-device snapshot transfer.
- The 2026-08-11 schema-v10 closeout reached `main` and the live Windows service
  at `4c891bd`. Native Windows mTLS/process E2E proved exact worker enrollment,
  exporter-bound dispatch and lease, wrong-digest denial, valid metadata-only
  acknowledgement, remote owner-route absence, and pause-dominant late-result
  denial. The deployed database and pre-v10 schema-v9 backup both passed
  `PRAGMA integrity_check`; the new dispatch table remained empty by default,
  and live owner-route probes returned 401 without authentication and 422 for
  an authenticated malformed request. Focused unit coverage includes exact
  capability-field drift, zero and maximum authority bindings, malformed or
  mutation-claiming results, and native-agent envelope drift. This is
  historical admission and
  deployment proof, not snapshot transport or coding-execution proof.
- Windows snapshot-claim fixtures must derive the final DOS path before
  computing the scope digest and recording grants. Using the raw temporary path
  can make the real filesystem admission correctly return 409 even when the Git
  fixture itself is valid; reuse the same `GetFinalPathNameByHandleW`
  normalization as production rather than weakening path validation.
- Raw TCP HTTP E2E clients should stop after reading the complete declared `Content-Length` body.
  Waiting for EOF with `read_to_string` can turn a complete
  `Connection: close` response into a macOS-only connection-reset failure when
  the server closes with unread request bytes. Truncated or malformed response
  bodies must still fail the harness.
- Schema v12 admits only one protocol-owned canonical `README.md` replacement
  artifact for the fixed contained-coding fixture. The Rust agent constructs it
  in memory, hashes exact bytes, cleans workspace and transfer state, then
  returns a strict result/artifact pair. Swift validates the canonical bytes,
  result binding, and receipt while uploading over the existing FIFO mTLS
  cancellation race; it posts only the metadata result afterward.
- Result-artifact bytes live under private Windows master state, never SQLite or
  audit. SQLite stores immutable artifact ID, digest, size, and exact device,
  registration, session, epoch, task, step, attempt, lease, cancellation,
  feature, snapshot, and work-packet bindings with a same-transaction redacted
  audit row. Exact retry is idempotent. Mismatch, replay, stale identity,
  cancellation, pause, expiry, lifecycle drift, missing admission, and result
  metadata drift fail closed.
- Startup removes unreferenced artifact directories. Referenced ambiguous state
  is retained as evidence, and active-feature startup quarantine remains the
  dominant recovery rule. Artifact admission is not apply/integration: it adds
  no canonical repository mutation, lifecycle advancement, test execution,
  provider/review, Git/publication, or autonomous-dispatch authority.
- The artifact store uses owner-private fixed-shape paths, stable no-follow or
  Windows by-handle validation, hardlink/reparse rejection, coordinated exact
  retry guards, startup verification of every referenced file, and immediate
  pre-result re-hashing while handles remain live through the transaction.
  File flush plus same-volume rename is implemented; portable Windows directory
  flush, service-account ACL, and crash durability remain live release proof.
- The 2026-08-11 protocol-v4/schema-v12 result-artifact closeout reached
  `origin/main` at `ea6a543`. Both exact-SHA hosted gates passed, the Windows
  service migrated with a verified schema-11 backup and reported healthy on
  schema 12/protocol 4, and the signed Mac-to-Windows `--run-local-coding` lane
  completed through the production Swift relay and real Rust agent. Terminal
  success therefore included exact artifact admission before result
  acceptance; owner cancellation and abandonment then left the queue, feature
  lease, distributed attempt, Windows transfer staging, and Mac workspace
  empty, revoked all three temporary grants, and removed the disposable
  checkout. This is functional development evidence, not artifact application,
  integration, Developer ID distribution, notarization, or clean-profile QA.
- The live local-coding preparation receipt contains a bounded but relatively
  long canonical base64 request. Deliver each sanitized controller receipt to
  the harness as one unchanged JSON line on stdin; terminal tooling that
  truncates or reserializes the line invalidates the approved-request digest.
  A pipe or FIFO is acceptable operator coordination, but it grants no product
  authority and must not bypass the harness's strict digest and binding checks.

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

- Protocol v5/schema v13 replaces the fixed README fixture while retaining the
  accepted schema-v10 through schema-v12 contracts. It replaces only that fixed
  execution boundary with immutable
  digest-bound general coding packets: at most 64 sorted normalized relative
  paths and exact deterministic `file.write.v1` or `file.delete.v1` operations.
  Rust and production Swift share 16 KiB complete-job, 12 KiB context, and 4 KiB
  replacement limits. The agent holds owner-private no-follow descriptors for all
  parents and uses exclusive atomic create, atomic-swap replacement plus displaced-
  inode verification, or identity-checked `unlinkat`; same-UID pathname substitution
  cannot redirect mutation. It executes no child or shell, emits a canonical multi-
  file artifact, and seals the attempt workspace for at most one hour without
  adding recovery metadata inside that attested tree.
  A separate bounded private recovery record binds the exact job/attempt, sealed
  name, post-edit tree digest, and expiry. Restart re-hashes one exact pair,
  reconstructs cancellation, blocks new admission while unresolved, removes an
  expired pair, and rejects tamper/orphans/ambiguity. The record never enters
  logs, audit, remote payloads, or SQLite. This is not canonical-repository
  application, test, integration, review, publication, lifecycle advancement, a
  host sandbox, or host egress proof.
  Delete capture uses a same-parent atomic swap: only a verified displaced inode
  is removed, while mismatch atomically restores the substituted leaf. Windows
  independently validates canonical artifact operations and packet digest against
  the immutable job and matches stored artifact/retention/expiry metadata to the
  terminal result. Swift is defense in depth, not the Windows authority source.
- Backup-first schema-v12 startup preserves an existing protocol-v4 result only
  when it is the exact canonical README artifact and its immutable stored digest
  and size match. The compatibility validator is migration-only: new prepare,
  handle revalidation, result admission, and terminal acceptance reject it and
  remain strictly protocol-v5.
- A protocol-v5 live success must not reuse the protocol-v4 assertion that the
  Mac snapshot root is empty. The production agent deliberately leaves exactly
  one owner-private `<attempt>.sealed` directory and one matching
  `<attempt>.retention.json` recovery record. A live harness may validate and
  remove that exact pair only because it owns the disposable temporary root; it
  must label this as harness cleanup, not product cancellation. Native relay
  coverage remains authoritative for restart reconstruction, exact cancel,
  expiry, tamper/orphan rejection, and cleanup.
- Work-packet `expected_before_sha256` binds the immutable Git blob bytes, not
  the platform-normalized working-tree file. Windows checkouts can expose CRLF
  bytes while `git cat-file blob <commit>:README.md` returns the committed LF
  bytes. The Windows live controller self-check creates both forms and requires
  their hashes to differ while the packet keeps the immutable blob digest; it
  also rejects case-drifted blob paths.
- Schema v14 deliberately does not widen the protocol-v5 Mac worker contract.
  Candidate creation is a separate Windows owner-loopback action: reopen and
  independently validate the complete terminal accepted artifact set, derive
  deterministic application order from immutable dispatch ordinal and packet
  ID, apply only to a private no-remote repository created from the immutable
  claimed snapshot, and freeze one exact commit/tree. The registered source
  checkout is never the integration target.
- Integration ID is the durable idempotency key. An exact retry returns the
  original candidate receipt only after reopening and revalidating that candidate;
  changing its feature, artifact set, authority
  revisions, snapshot, grants, or base commit is a conflict rather than a new
  attempt. A lost HTTP response therefore does not authorize another Git effect.
- Artifact admission is still evidence-only. Only schema-v14 integration may
  consume it, and only after the artifact set exactly equals every terminal
  accepted artifact-backed dispatch for the active feature. Duplicate packet
  ordinals, overlapping paths, file-content compare-and-set drift, or tree-shape
  conflicts leave no partial candidate and no lifecycle advance. The resulting
  `validating` state is a candidate boundary, not test, review, publication, or
  registered-source mutation proof.
- Integration brackets the immutable source snapshot and frozen candidate with
  stable no-follow handle inventories, retains candidate handles through the
  SQLite commit, rejects alternate object stores and component-wise
  file/directory collisions, and keeps its singleton reservation alive when an
  HTTP client disconnects.
- The schema-v14 `--run-local-coding` live lane is now an integration proof, not
  merely an artifact-admission proof. Before owner cancellation and safe
  abandonment, the Windows-local controller freezes the exact candidate,
  verifies its detached/clean/no-remote/fsck shape and exact-retry idempotence,
  and confirms the registered source checkout is unchanged. Test execution,
  review, publication, and registered-source mutation remain later boundaries.
- Schema v15 adds the strict, owner-token-authenticated loopback-only
  test/evidence-gate contract and immutable persistence; it does not add an
  enrolled-device mutation. The approved manifest must contain the exact
  ordered 13-command protocol plan: requirements binding, coverage, focused
  unit tests, native E2E, documentation, knowledge base, formatting, lint,
  build, safety, changed paths, secret scan, and repository validation.
- A validation request is bound to its canonical plan/request digests and the
  exact feature, specification and `validating` lifecycle revision, retained
  lease, snapshot, integration and artifact set, candidate commit/tree/base
  commit, queue and Emergency Pause revisions, and all three grant revisions.
  It cannot supply executable names, argv, shell text, paths, raw output, or
  caller-asserted evidence.
- Schema-v15 validation persistence is immutable and digest-only: each command
  records only its identifier, pass bit, nonzero result digest, duration, and
  truncation bit, plus one aggregate evidence-manifest digest. Only all 13
  ordered, passing, non-truncated results atomically advance
  `validating -> reviewing` with redacted audit and transition evidence. A
  recorded failure stays `validating`; incomplete or malformed evidence does
  not complete.
- Exact retry is outcome-preserving. A passed retry reopens and revalidates the
  frozen candidate before returning the original receipt; a failed retry
  returns the same fixed failure; a different validation ID or request binding
  rejects. Startup quarantines active `validating` state as effect-possible and
  never retries it automatically.
- The production validation runner is connected only when fixed Windows
  toolchain and credential-free dependency-cache inputs validate at startup.
  Missing provisioning returns `validation_runner_unavailable` before an
  attempt or audit row is created.
- The Windows validation-containment module derives exact path/acceptance scope
  from immutable work packets, binds it to the approved design digest, verifies
  toolchain/cache plus authoritative and disposable candidates before recording
  a start, runs deterministic master checks and fixed offline Cargo commands
  including a protocol-owned 70% minimum line-coverage threshold, and
  owns executable/argv/environment selection. Its native fixture
  harness uses a restricted token, standard zero-capability AppContainer, exact
  minimal environment and inherited-handle list, bounded output, Job Object
  memory/process/tree limits, temporary root ACL, and profile cleanup. Hostile
  fixtures prove active cancellation/tree reaping, denial of one outside-root
  file, and nondelivery to loopback TCP/UDP probes. They do not prove installed-
  service identity, a real populated production toolchain/cache, credential-
  store isolation, actual above/below-threshold llvm-cov behavior, signed Mac
  E2E, or OS-wide outbound-egress enforcement.
- `cargo llvm-cov --no-report` returns before report-threshold evaluation, so
  the fixed coverage command must emit `--summary-only` and
  `--fail-under-lines 70`; the portable runner contract must reject any return
  of `--no-report`. The deterministic requirements check binds the approved
  design digest, acceptance-count presence, and exact approved changed paths;
  it is `requirements_binding`, not a claim that each criterion has been
  semantically mapped to an individual test.
- Schema v16 adds the owner-token loopback-only Independent Review Gateway.
  The request is path-free and cannot carry review content; Windows rebuilds a
  canonical packet from the immutable approved specification, exact verified
  candidate patch, and ordered validation-result digests. Raw brainstorming,
  implementation transcripts, canonical memory, paths, raw command output, and
  secrets are excluded by approval-time sensitive-context admission and a
  strict deny-unknown-fields review-safe manifest DTO, plus a review-time recheck
  for migrated legacy rows. Patch
  extraction retains add/delete/context polarity rather than hashing unlabeled
  line content.
- Review-provider admission is mechanical and occurs before durable mutation:
  exact bound provider/model, at least 256 KiB and 64,000 input tokens, at least
  64 KiB structured output, strict decoding, and no fallback. A configured
  adapter is a canonical executable launched as a new cleared-environment
  process for every call with bounded pipes, timeout, and complete process-tree
  termination before pipe joins. On Windows the master holds a verified no-
  write/no-delete image handle while a trusted gate-blocked launcher enters the
  kill-on-close Job Object before provider spawn; its only
  argument is the fixed `--count-tokens` adapter preflight. Missing config
  is unavailable. Native process coverage is repository proof only, not live
  selected-provider or reviewer-quality proof.
- Review calls, transport outcomes, and decisions are immutable. Outage,
  malformed output, and incomplete transport record fixed evidence plus 1-,
  5-, or 15-minute backoff, consume no repair cycle, and are capped at three
  calls per candidate and twelve per feature. Rejection keeps the feature and
  lease in `reviewing`; only exact approval atomically advances
  `reviewing -> publishing`.
  The master requires exact ordered coverage of identifiers from the approved
  manifest's required top-level `acceptance` array and evidence references drawn
  only from the packet. Interrupted calls, including post-response authority
  drift, terminalize immutably and quarantine without automatic retry.
- The schema-v17 Publication Coordinator is an owner-token loopback-only,
  path-free durable authority boundary absent from mTLS. It binds the approved
  review decision, candidate/spec/evidence/provider/grants/queue/pause, remote
  base, and branch policy; persists an intent before each of seven external
  action classes; and accepts evidence only from a fixed internal adapter.
  Stage evidence self-binds remote base/head, PR identity, complete named checks,
  branch-protection/no-bypass state, merge strategy/resulting main, and fixed
  post-merge gate. Adapter calls receive a deadline and pollable current-
  authority/cancellation control; late results are suppressed and ambiguity
  quarantines.
  Merge advances only to `verifying_main`; exact remote-main equality plus the
  fixed `release-local` post-merge gate atomically marks success, releases the
  lease, and advances the queue. Ambiguity or restart quarantines. The
  production GitHub adapter is default-unavailable, so native bare-Git E2E is
  not live credential, hosted-check, merge-API, or branch-protection evidence.
- A durable schema-v17 merge intent is sufficient to make merge possible for
  owner resolution. Even if quarantine origin is `publishing`, abandonment
  requires merged healthy-main evidence and rejects `merged:false`. Pre-v17
  resolution remains governed by its historical immutable transition origin.
- Schema v18 adds a backup-first, path-free immutable orchestration checkpoint
  ledger and deterministic internal coordinator. It has no activation writer or
  HTTP/device route, so it is durable but default-inert pending capability 7 and
  owner-recorded live evidence. Decisions derive only from lifecycle and exact
  admitted validation/review/publication evidence; caller commands, paths,
  provider/adapter output, credentials, and retry classification are excluded.
- The schema-v18 active clock is capped at 24 hours and excludes provider,
  worker, maintenance, and owner pauses. A complete provider-backoff pause is
  restart-safe; Emergency Pause charges through the pause instant and requires
  a fresh coordination checkpoint to restart the clock after resume. Unresolved
  provider calls, repository effects, and publication intents quarantine. The
  initial candidate is free, the repair ceiling is three replacements, and
  completed immutable calls at either review ceiling require owner attention.
- Current protocol-v5 dispatch and artifact admission cannot safely bind a
  replacement candidate to a prior substantive failure. The coordinator
  therefore records `repairing` and then `attention_required` without charging
  repair or claiming completion. Only existing exact healthy-main success may
  automatically release the active lease. Coordination rejects cancellation,
  attention, and failure without mutation; the owner may explicitly reconcile
  and abandon those states through exact lifecycle/queue/pause CAS and durable
  transition-origin evidence. Quarantine and abandonment never auto-advance.
- The 2026-08-13 Automatic Orchestration and Repair kernel closeout reached
  exact feature commit `542d3577e9e674c21a7378a36d9879f20b1fc7d6`.
  Independent high-risk review rejected terminal-state lease traps, Emergency
  Pause wall-time charging after resume, and unbounded review transport retry;
  the corrected kernel preserves explicit owner resolution, suspends and
  restarts its active clock at verified checkpoints, and moves exhausted
  review budgets to owner attention. The requirements-derived unit matrix and
  native Rust, SQLite, process, Swift, signed-helper, and live Mac/Windows E2E
  passed. Playwright, visual regression, and cross-browser testing were not
  applicable because the phase has no browser surface. Documentation and
  safety verdict: the phase followed `DESIGN.md`, `docs/safety-rules.md`, the
  feature-conveyor design, and the feature-closeout workflow; the documentation
  drift contract and canonical local gate passed.
- Both hosted gates passed for that exact feature commit: Windows Distributed
  run `31749837880` and Release Local run `31749837805`. The authoritative
  Windows checkout was fast-forwarded to the same SHA, `AssemblywrightMaster`
  rebuilt, and the database migrated backup-first from schema 17 to schema 18.
  The live database and retained 1,880,064-byte backup
  `master.pre-v18.680a6a2b-766f-461d-affe-629122f6098b.sqlite3` passed
  `PRAGMA integrity_check`; the backup SHA-256 was
  `b4a52f0288a18a83fb5cf5351dc8312366a1f57eef19cd93da470b1f20f92d9d`.
  Final health was protocol 5/schema 18, running and unpaused with maintenance
  clear and no active, connected, queued, or leased work.
- After a strict Swift Feature Conveyor schema change, a previously signed
  helper can still authenticate its initial connection yet fail closed while
  decoding monitor samples. Rebuild and independently sign the production
  helper before live proof; never treat a source-only Swift build as deployed
  helper parity. For schema v9, the refreshed Apple-signed helper completed the
  live monitor and reconnect lane against `100.64.23.14:7792`, with two bounded
  authenticated samples on one connection and reconnect epochs `6763` to
  `6764` under team `H686S3N4V9`.
- The 2026-08-13 Publication Coordinator closeout reached exact published
  commit `d18d15b1b988e471ce69c7e5287c10f1506f0c42`. Independent high-risk
  review first rejected ambiguous-merge abandonment, generic adapter evidence,
  and missing in-flight cancellation; the corrected tree requires healthy-main
  reconciliation after any durable merge intent, stage-specific self-bound
  evidence, and cooperative deadline/current-authority/cancellation polling.
  Native proof comprised three strict protocol tests, eleven publication
  kernel/migration/concurrency cases, a controlled bare-Git remote, the real
  owner-loopback master process with remote-mTLS absence, and the canonical
  local gate. Playwright, visual regression, and cross-browser testing were not
  applicable because the slice has no browser surface. Documentation and
  safety verdict: the phase followed `DESIGN.md`, `docs/safety-rules.md`, and
  the feature-conveyor/release workflow, preserved fail-closed authority and
  evidence boundaries, and passed the documentation drift contract.
  `unit-testing-test-generate` verdict: followed with a requirements-derived
  matrix spanning success, idempotence, malformed/stale evidence, every
  capability and token boundary, cancellation, authority revocation, deadline,
  restart, migration, and ambiguous effects. `e2e-testing` verdict: followed
  with native Rust, SQLite, process, mTLS-route, and controlled-Git boundaries.
- Both hosted gates passed for that exact commit: Windows Distributed run
  `31737160863` and Release Local run `31737160996`. The authoritative Windows
  checkout was fast-forwarded to the same SHA; `AssemblywrightMaster` rebuilt,
  migrated backup-first from schema 16 to schema 17, and returned healthy at
  protocol 5/schema 17 with Emergency Pause and maintenance clear and no
  active, queued, or leased work. The verified migration artifact was
  `master.pre-v17.6fa7d45a-b179-4065-b094-1c1b4f81e7b8.sqlite3` (1,826,816
  bytes), SHA-256
  `03baecfc0ecac6828d4471853ebce2dce208548a59a61e8db678037b953bcc43`.
  This is source, CI, migration, and deployed-service proof only: the production
  GitHub adapter remains default-unavailable, and live GitHub credential/API,
  branch-protection, PR/check/merge, signing/notarization, clean-profile, and
  owner-recorded live-device evidence remain separate activation boundaries.
- A Rust integration test's `current_exe()` is the test harness, not Cargo's
  product binary. The Windows review-provider Job/launcher E2E must inject
  `CARGO_BIN_EXE_assemblywright-master` through the Windows debug-only test
  constructor. Production loading must continue to capture only its own
  `current_exe()` and must expose no runtime launcher override; otherwise the
  test seam would become a production executable-selection authority.
- The 2026-08-13 Independent Review Gateway closeout reached exact published
  commit `b7e8fc76efe9076aca3ba3dcd26221231b9ac6a8`. Native Windows proof covered
  the strict protocol, nine review-kernel cases, the configured adapter's
  verified-image/gated-Job boundary, owner-loopback routing, and remote mTLS
  exclusion. The deployed `AssemblywrightMaster` then migrated backup-first
  from schema 15 to schema 16; both the retained backup and live database
  passed `PRAGMA integrity_check`, source parity matched `origin/main`, and the
  empty queue remained unpaused. The production adapter directory remained
  absent, which is the intended default-unavailable state rather than live
  selected-provider or reviewer-quality evidence.
- Schema v19 is the first authoritative Feature Conveyor owner-activation
  surface. Windows alone admits six fixed, category/origin-bound digest
  references over the owner-token loopback boundary. The exact designated,
  current, non-fixture, exporter-bound MacBridge may inspect their redacted
  projection and bind only those exact current references into one immutable
  global activation; it cannot admit evidence. Activation creates no lease or
  queue work and remains blocked by incomplete evidence or Emergency Pause.
- Feature 1 adds the separate Mac/local repository-gate proof controller at
  `scripts/repository-gate-proof-controller.sh`. Its owner-run `--run` accepts
  no repository or command argument, requires exact clean `main` with `HEAD ==
  refs/remotes/origin/main`, compares branch/HEAD/tree/status/origin and the
  committed `release-local.sh` definition before and after executing only that
  exact committed blob through argument-free Bash stdin, and fails closed on drift, failure, or handled
  cancellation. Success atomically publishes an owner-only, path-free bounded
  schema-v1 receipt and raw SHA-256 sidecar beneath the ignored
  `target/repository-gate-proof/` directory. The receipt binds category
  `repository_gate_proof`, origin `repository_gate_proof_controller`, exact
  HEAD/tree, gate-definition digest, fixed gate identity, observed time, pass
  status, and the narrow repository proof boundary. `--check` and the
  disposable native Git/process `--self-test` run inside `release-local.sh`;
  the canonical gate never invokes `--run`, avoiding recursion. The controller
  neither admits its digest nor activates, mutates source, or claims any live,
  provider, publication, signing, notarization, or production proof. Windows
  owner-token admission and its atomic redacted audit remain separate.
  `target` and the proof directory must be ordinary owner-matched directories;
  symlink/reparse-style ambiguity fails before cleanup. A new run then removes
  only the exact old receipt pair so a failure cannot leave stale output that
  appears current. Archive any receipt the owner intends to retain before
  deliberately rerunning the controller.
  All Git observations use fixed-root, environment-empty Git with replacement,
  config, index/work-tree, object-directory, and alternate-object redirections
  disabled. The committed gate's internal stdin/root marker is cleared from
  caller input and unset before its command list. `target` must not be group or
  world writable. The controller holds the output directory, binds and
  revalidates target/output device and inode across the gate and relative-name
  publication, and cleans only through that held directory. HUP/INT/TERM drains
  and reaps the gate's separate process group with bounded TERM-to-KILL; any
  surviving descendant suppresses proof.
  All tracked index entries must retain the normal `H` tag; assume-unchanged,
  skip-worktree, and every other hidden tracked-state tag reject. Pre/post
  stability detects concurrent same-UID changes to other gate-consumed files at
  the controller edges but is not a host sandbox or same-UID isolation claim.
  The disposable controller suite also fixes the public CLI contract (default
  check, unknown-mode rejection, and extra-argument rejection) and proves that
  a rejected rerun invalidates an earlier receipt pair rather than exposing it
  as evidence for the failed attempt.
  Gate failure terminates its complete process group immediately. A successful
  gate leader may briefly outlive one naturally exiting group member, so only
  that success path performs a bounded natural drain before deciding the group
  survived. A member still present at that deadline is terminated, reaped, and
  makes the proof attempt fail.
- Feature 2 adds `scripts/restricted-worker-proof-controller.sh` as a separate
  owner-supervised live-proof wrapper around the existing real Mac/Windows local-
  coding boundary. It accepts no repository, remote, worker, executable, work
  packet, or alternate harness; requires exact clean published `main`; runs only
  the committed `mac-windows-bridge-live-e2e.sh` blob through Bash stdin; and
  reads the sanitized Windows-local phase receipts from a different inherited
  descriptor. Caller helper, agent, Tailscale, Git, and internal-marker
  overrides are cleared. The live lane uses the signed Swift helper, separate
  singleton `local.coding.v1` Keychain identity, real Rust agent, protocol v5,
  schema-v19 master, and schema-v9 conveyor projection. One bounded worker
  attempt must reach strictly ordered queued/leased/succeeded events, Windows
  artifact admission and candidate verification, private Mac retained-pair
  observation, explicit cancellation, safe abandonment, grant revocation, and
  complete disposable state cleanup. Exactly six unique ordered Windows-action
  markers followed by one exact terminal record are accepted. The complete
  owner-private temporary transcript is hashed and removed. The fixed `0600` receipt pair under ignored
  `target/restricted-worker-live-proof` contains only category
  `restricted_worker_live`, origin `restricted_worker_proof_controller`, exact
  commit/tree, committed Mac/Windows controller digests, transcript digest,
  fixed proof identity, observed time, pass status, and the narrow boundary.
  New runs invalidate the prior pair only after validating the ordinary owner-
  matched output chain; failure, drift, malformed/duplicate success, surviving
  descendants, or handled cancellation leaves no receipt. The controller does
  not admit evidence or activate, and this same-owner live proof is not a host-
  sandbox, OS-wide egress, review-provider, GitHub, restart-recovery, control-
  streaming, signing/notarization, clean-profile, or production-readiness claim.
- Feature 2 Prepare recovery is marker-bound rather than a second enqueue API.
  The Windows disposable marker is flushed, byte-validated, and atomically
  renamed into place before grants; it persists the generated repository/feature IDs
  and queue, Emergency Pause, owner-designation, source, and commit baseline.
  Retry creates only missing exact revision-1 grants and reconstructs the same
  approved request. A lost signed-helper enqueue receipt is recoverable only
  when the status surface contains the sole exact queued lifecycle-revision-1
  feature and baseline-plus-one queue revision; any other state fails closed.
- An owner-supervised shell controller must not leave its background live
  process reading the shared interactive TTY: job control can stop it with
  `SIGTTIN`. Feature 2 keeps the controller in the foreground for bounded owner
  input, relays each sanitized receipt through a private FIFO to descriptor 3,
  and keeps the committed Bash-stdin script bytes on a separate channel.
- macOS interactive PTYs report `MAX_CANON=1024`, while the sanitized Feature 2
  Prepare receipt can exceed two KiB. The controller therefore switches stdin
  to no-echo, noncanonical mode only for each bounded receipt read and restores
  the exact saved terminal state on success, failure, or handled signal. Its self-test
  sends 2,182-byte and 9,000-byte lines through real PTYs, verifies digest
  equality for the accepted receipt, caps stored input at 8,192 bytes while
  draining and rejecting the oversized line, and separately proves TERM
  restoration.
- Windows PowerShell adapts JSON objects and ordered dictionaries differently:
  `PSObject.Properties.Name` enumerates JSON-object keys but not the entry keys
  of an `OrderedDictionary`. Exact-key guards that accept both shapes must use
  `IDictionary.Keys` for dictionaries and compare counts plus ordinally sorted
  keys without delimiter serialization. The Windows-executed `Check` lane
  exercises the same atomic marker writer used by live `Prepare` and rejects
  wrong-case and composite-key collision fixtures.
- The 2026-08-15 Feature 2 Restricted-worker Live Proof Controller ran from
  exact clean published `main` at implementation commit
  `8a9e373ea1e61e9504ec0630971f7aeebc2bec1f`. Independent high-risk review
  approved the final fail-closed design after its findings on Windows ordered-
  dictionary key validation and bounded interactive-terminal receipt handling
  were corrected. The requirements-derived unit matrix covers fixed CLI and
  committed-byte execution, clean-main and definition drift, exact action and
  terminal transcript shapes, hostile output paths, prior-receipt
  invalidation, environment isolation, atomic publication failure, process-
  group cancellation and descendant rejection, plus real-PTY acceptance of a
  2,182-byte receipt, drain-and-rejection of a 9,000-byte line, and exact
  terminal restoration on success and TERM.
  The native cross-device E2E used the Apple Development-signed Swift relay
  under team `H686S3N4V9`, the separately enrolled `local.coding.v1` identity,
  and the real Rust agent against the live Windows service at protocol 5 and
  schema 19. The exact snapshot-bound README-only attempt reached event
  sequences 13987/13989/13990 for queued/leased/succeeded, froze detached
  candidate `b0157ee223ef4f41a57346ab492ef23fbf680293`, then proved explicit
  owner cancellation, safe abandonment, empty queue and lease state, three
  grant revocations, empty transfer staging, retained-pair cleanup, and removal
  of the disposable checkout. Final Windows health was running, unpaused, and
  maintenance-clear with no active attempts, connections, queued steps, or
  leased steps.
  The 0600 receipt was observed at `2026-08-15T14:26:43Z`; it binds tree
  `ce527610ae10a57b3a5d027057e301c5f5f0442d`, transcript SHA-256
  `48d81091ed50b4064af5abf9c98328d28f534b73ae70ebf7d8c2942063ac4e99`,
  and receipt SHA-256
  `670778420b977356a1a01b5895d76f02f69241482bdda43f1b47835b961bc83c`.
  Documentation and safety rules were followed; `unit-testing-test-generate`
  and `e2e-testing` were applied, with Playwright excluded because the proof is
  a native Bash/PowerShell/Rust/Swift/process/protocol/live-device boundary and
  has no browser surface. This receipt remains local and unadmitted: it does
  not activate the conveyor or prove selected-review-provider integration,
  GitHub publication, restart recovery, Mac/Windows control streaming,
  Developer ID distribution, notarization, clean-profile installation, or
  production readiness.
- Shell parameter expansion such as `${token#feature_id=}` returns the original
  token when the prefix is absent; it is extraction, not label validation.
  Exact live-proof records therefore match each complete `name=value` token
  before validating or extracting its UUID/digest, with missing-label and
  wrong-label negative fixtures.
- The 2026-08-14 Feature 1 Repository-gate Proof Controller implementation is
  published at exact commit `eb4cc0e9933601f9e141523dba4fa24772ffef75`.
  Independent high-risk review approved the final fail-closed design after its
  findings on committed-byte execution, Git environment hardening, output-path
  identity, index flags, and process-group lifecycle were corrected. The
  requirements-derived unit matrix covers the fixed CLI, success receipt and
  digest binding, dirty/wrong-branch/origin/status drift, assume-unchanged and
  skip-worktree rejection, prior-receipt invalidation, hostile/swap/writable
  target denial, environment isolation, atomic-move failures, cancellation,
  immediate failed-gate descendant suppression, success-only natural drain,
  and persistent-descendant rejection. The native Git/process/filesystem E2E
  ran the exact committed `release-local.sh` bytes from clean published `main`;
  the full canonical gate passed and emitted the fixed 0600 receipt pair at
  `2026-08-14T20:53:04Z`. The receipt binds tree
  `ab4829427e896132a41a80c7bb8251a30d9f8edb`, gate-definition SHA-256
  `93c2367b4fd588af058d0b6bff1f35d2ac25ac3c4d24bcd6be2754c5a39bff49`,
  and receipt SHA-256
  `0342cf1956682b4669f4c6d2802398a7158e9afea6ce00e467051072f3521f3e`.
  A diagnostic run with the noncanonical 45-second release-local heartbeat
  override left its current heartbeat sleep in the gate process group; the
  controller correctly rejected that attempt and emitted no receipt. Rerunning
  without the diagnostic override passed, preserving the rule that progress
  instrumentation is outside the proof environment. Documentation and safety
  rules were followed; `unit-testing-test-generate` and `e2e-testing` were
  applied, with Playwright excluded because this is a native shell/Git/process
  boundary rather than a browser surface. This local receipt has not been
  admitted by the Windows owner, does not advance the 0/6 activation count,
  and proves no restricted worker, review provider, GitHub publication,
  restart recovery, live Mac/Windows control, Windows deployment, signing,
  notarization, clean-profile install, live-device QA, or production readiness.
- The macOS app remains outside TLS-key and owner-token custody. For an explicit
  confirmed owner action it stops and reaps its monitor child, revalidates the
  same independently signed helper, launches one bounded `--confirm` command,
  validates the exact revision-bound receipt, and then restores monitoring.
  At most one helper child is owned at a time. Owner orchestration pause is not
  Emergency Pause; cancel retains the feature lease, while abandon-and-advance
  requires an explicit safe-reconciliation digest and healthy-main evidence
  whenever a merge may have occurred.
- Repository tests, controlled providers/remotes, and source builds do not
  satisfy schema-v19 live activation. The six admitted categories must refer to
  genuine repository-gate, restricted-worker, selected-review-provider,
  GitHub-publication, restart-recovery, and live Mac/Windows control plus event-
  streaming receipts. Signing, notarization, clean-profile install, and broader
  production readiness remain separate evidence boundaries.
- The 2026-08-13 Owner Control and Activation Surface closeout reached exact
  published source commit `aee87048b30c7b2601d2dd001e219938d82ff349`.
  Independent high-risk review first rejected non-owner pause decoding and
  action exposure, an unbounded helper teardown, weak durable evidence-role
  revalidation, and missing remote owner-mutation coverage; every finding was
  corrected and the final verdict was approval. Requirements-derived unit and
  native Rust, SQLite, process, Windows mTLS, Swift, signed-helper, and live
  Mac/Windows E2E passed. Playwright, visual regression, and cross-browser
  testing were not applicable because this is a native surface. Documentation
  and safety verdict: followed and implemented against `DESIGN.md`,
  `docs/safety-rules.md`, the Feature Conveyor design, and the phase-closeout
  contract. `unit-testing-test-generate` verdict: followed with a
  requirements-derived matrix covering success, all six missing-evidence
  positions, malformed/stale/category/origin input, redaction, idempotence,
  Emergency Pause, durable tamper/reopen, non-owner pauses, and helper teardown
  edges. `e2e-testing` verdict: followed with native Rust/SQLite/process,
  Windows mTLS/SCM, Swift signed-helper, and live Mac/Windows app-lifecycle
  boundaries. The documentation drift contract plus canonical local gate
  passed.
- Hosted Windows Distributed run `31758741258` and Release Local run
  `31758741133` passed for that exact SHA. The native Windows gate also exposed
  and closed a test-only exact-binding bug: a local-coding result and its
  artifact admission had independently sampled the workspace expiry clock and
  could differ by one millisecond. The fixture now captures one expiry for both
  documents; production equality remains strict.
- The authoritative Windows checkout was fast-forwarded to the exact source
  SHA, the native owner-control mTLS test passed on the live host, and
  `AssemblywrightMaster` rebuilt and migrated backup-first from schema 18 to
  schema 19. The live database passed `PRAGMA integrity_check`. The retained
  1,925,120-byte backup
  `master.pre-v19.8b726bff-e9b8-427a-a646-d5334e2570fa.sqlite3` has SHA-256
  `c29cfb868437eb95e0cffac5ff1e0f273b641a9be7510a7de4c6075ae09d17a8`.
  Final service health was protocol 5/schema 19, running and unpaused with
  maintenance clear and no active, connected, queued, or leased work.
- The refreshed Apple Development-signed production helper completed live
  authenticated monitoring, reconnect, and production-app supervision against
  `100.64.23.14:7792` under team `H686S3N4V9`. Its live redacted owner
  projection reported queue revision 24, designation revision 1, no active
  feature, and `activation_status=inactive` with `evidence_required`: all six
  evidence slots were empty. This proves the deployed control/inspection and
  fail-closed blocker path, not genuine provider/GitHub/restart activation,
  Developer ID distribution, notarization, or clean-profile installation.
- Deployment and live proof that occurs after a feature commit must not remain
  only in terminal output or conversation history. Publish a docs-only closeout
  commit, require the hosted gates for that exact final SHA, then fast-forward
  the authoritative Windows checkout and recheck service health. Feature 7 did
  this at `40456d366280c2525bc5968024f132aa6880396a`: Release Local run
  `31759496009` passed; Windows Distributed run `31759496029` transiently hit
  Win32 timeout `258` in the unchanged review-provider E2E, then passed attempt
  2 at the same SHA without weakening the test. Windows final source parity and
  the already-deployed protocol 5/schema 19 service health were reverified.
- Feature 3 adds a fixed selected review-provider integration and proof
  controller. Windows provisioning accepts only exact clean published `main`,
  `openai.codex`, `gpt-5.6-sol`, Codex `0.148.0`, the fixed owner-private Codex
  auth home, and committed adapter/output-schema assets. The master verifies
  and holds adapter/Codex/schema identities across each response-only Job Object
  call, requires owner/SYSTEM-only protected auth DACLs,
  clears the adapter environment to four exact bindings, and exposes no runtime
  path/provider/model selector. The adapter clears again, passes `CODEX_HOME`
  plus only OS-derived Windows `SystemRoot` and a system-directory-only `PATH`,
  uses strict-config ephemeral read-only Codex execution, disables shell,
  snapshot, agent, skill-install, image, and web surfaces, and rejects packet or
  output binding drift.
- The Mac approved-feature UI calls `openai.codex` / `gpt-5.6-sol` the
  **production review binding**, displays it as fixed by the Windows master, and
  states that repository-scoped Codex development reviewer agents are separate.
  The UI wording prevents the read-only production binding from being mistaken
  for `.codex/agents/assemblywright-reviewer.toml`; it adds no provider selector
  or provider-rebinding authority.
- `scripts/review-provider-proof-controller.sh --run` accepts no repository,
  provider, model, executable, schema, or harness argument. Exact committed
  harness bytes and the sanitized Windows receipt use separate descriptors.
  The live command requires one fixed valid candidate approval and one fixed
  invalid candidate rejection, then binds only the exact published commit/tree,
  five committed definition digests, bounded private-transcript digest,
  provider/model, time, pass status, and narrow boundary into the owner-private
  `target/review-provider-live-proof` receipt pair. It does not admit evidence,
  activate, enqueue, prove general reviewer competence, exercise a real feature
  gateway lifecycle, or prove GitHub/signing/notarization/production readiness.
- Feature 3 live closeout passed at published implementation commit
  `2291432c3b91307654dc327edf4db630bd7fb7f3`. Windows provisioned the fixed
  `openai.codex` / `gpt-5.6-sol` provider with Codex `0.148.0`, then the native
  proof exercised one semantic approval and one semantic rejection against the
  healthy protocol 5/schema 19 service while the queue was empty and unpaused.
  The service, adapter, Codex, and output-schema SHA-256 identities were
  `c5776702ec9276579fc97960e4a2154aabb956379eef887c5d11319af4eec444`,
  `67d5cfbe01036939ad1ebf35db51a687def36ec8e31d769c14ec4504b02237d8`,
  `2ad2cf8a732da68b8f141634f92db1a03016c5faf533a7225fbc0fb740130410`,
  and `62af1c4c71e1592f3b6b42fa72ea19ed56de6fe61034ae866d0032d212fa1fe9`.
  The approval packet/output digests were
  `0051e11bc685936f5493ba062692e78ba7269722501b22e1600ba2b587a90427` /
  `97722b79662e76772a7aff9f0c127e78bd999e96dc4e476eda9307c568fafbea`;
  the retained rerun's rejection pair was
  `ba25a3a99e5ea5cf24d1c805c3d5eca191eb5a234d715ad219c3a04068161b3e` /
  `db890d6f79fee1e8ec186b323c76d553cccda5e7e417a1285b25df50c7e4995f`.
  The retained owner-private Mac receipt SHA-256 is
  `5306041761b199da2503945d33b8d9f3b4df6fba862200aab7a9dd81505801fe`.
  Canonical local release verification passed, as did Release Local run
  `32272305779` and Windows Distributed run `32272306493` at that exact commit.
  No evidence was admitted, no activation was attempted, and the live proof
  does not change the six-slot activation projection.
- Feature 3 testing uses the native boundaries that actually carry authority:
  Rust unit tests cover closed environment selection, complete Codex tool
  disablement, and OS-derived Windows DNS environment; native Rust integration
  and subprocess tests cover canonical prompt/digest framing,
  environment-pollution denial, provider admission/cancellation, pinned assets,
  semantic approval/rejection, Windows verified-image and Job Object
  containment; controller fixtures plus the live Windows lane cover clean
  published-source binding, strict receipt validation, private output, and
  hostile-output denial. The requested browser Playwright, visual-regression,
  and cross-browser phases do not apply because Feature 3 has no browser
  surface. `cargo llvm-cov` was unavailable during the closeout audit, so no
  numeric coverage percentage is claimed.
- Owner-private proof receipts under ignored `target/` paths are not durable
  merely because a terminal reported success. After any later build, packaging,
  or release activity, closeout must revalidate the ordinary owner-owned mode-
  `0700` canonical non-symlink directory plus each file's type, owner, mode,
  link count, and receipt/sidecar digest equality. The Feature 3 pair was
  observed absent after later verification and was restored by rerunning the
  exact clean published controller. A missing pair must never be reconstructed
  from copied terminal output.
- Feature 4 connects the schema-v17 Publication Coordinator to one fixed,
  default-unavailable Windows GitHub adapter. The production selection is
  `malak333/Assemblywright`, base `main`, pinned master, GitHub CLI, and Git binaries,
  owner/SYSTEM-private GitHub CLI authentication state, normal protected merge,
  and exact mappings from protocol IDs `release-local` and
  `protocol-windows` to the GitHub Actions contexts `Release local gate` and
  `Protocol, master, identity, mTLS, and SCM`, their exact workflow IDs and
  paths, and trusted workflow-file digests. No caller-selected repository,
  remote, executable, credential, command, context, protection policy, or
  adapter evidence is accepted.
- GitHub `main` is protected with pull requests, strict up-to-date required
  checks, both hosted contexts, administrator enforcement, and conversation
  resolution; force pushes and deletion are disabled. Publication must observe
  that policy and use a normal merge. An admin merge or any bypass is forbidden
  evidence, not a fallback.
- The GitHub adapter keeps credentials inside GitHub CLI secure storage and
  polls cancellation, deadline, and current authority around every effect. It
  pushes only the frozen candidate, binds one exact pull request and reviewed
  head, reconciles exact remote `main`, and accepts the fixed post-merge gate
  only from the successful `Release local gate` check on that commit. Any
  missing, stale, failed, late, or ambiguous result after durable intent
  quarantines without automatic retry.
- Feature 4 Windows provisioning must not start Cargo's release output in place:
  Cargo may hard-link it to `target\release\deps` and leave checkout ACLs that
  the provisioned adapter rejects at service startup. After the exact service
  process reaches stopped/PID-zero state, copy the exact Cargo bytes into a
  fresh single-link file, apply the owner/SYSTEM-only ACL, verify the digest,
  and move that materialized file into the service path. Rollback must likewise
  recreate the prior image as a protected single-link file before restart. Set
  and verify the current-owner SID plus exactly owner and SYSTEM FullControl,
  and verify the recovery copy digest before the service stop.
- The Feature 4 live proof uses the production adapter with one disposable
  bounded metadata-only proof-marker commit. Its protected pull request
  advances `main` with that durable public marker and no product-code change,
  so the Mac controller must keep its source checkout
  unchanged while fetching and binding the exact reported merged
  `origin/main`. The path-free owner-private receipt proves the fixed GitHub
  credential, push, PR, hosted-check, protection/no-bypass, normal-merge, and
  reconciliation path only; it is not activation admission, a queued feature,
  general repository support, signing, notarization, clean-profile, or
  production-readiness evidence.
- `scripts/github-publication-proof-controller.sh --run` accepts no alternate
  repository, remote, branch, tool, credential, context, or harness. It keeps
  committed harness bytes separate from the sanitized Windows receipt, removes
  its private transcript, and publishes only the fixed ignored owner-private
  `target/github-publication-live-proof` receipt/digest pair. Its receipt read
  remains bounded to one hour so both required pull-request checks and the same
  two post-merge checks can finish before Windows emits the receipt; that wait
  does not weaken the receipt's one-hour freshness validation or exact binding.
- Feature 4's portable focused coverage is split deliberately: the adapter
  suite exercises default-unavailable provisioning, hostile configuration and
  plaintext-token rejection, pinned executable drift, exact branch policy,
  check/workflow provenance, explicit cancellation, deadlines, remote-base and
  cleanup drift, credential-process isolation, and strict live-receipt shape;
  the coordinator suites cover strict wire bindings, idempotence, durable
  intent ordering, pause/cancellation races, restart ambiguity, quarantine,
  migration, and a controlled bare-Git remote. The controller self-test adds
  maximum-size PTY input, hostile output, process-group reaping, dirty/wrong-
  branch/hidden-index rejection, remote advancement with unchanged checkout,
  merge reconciliation, and private receipt publication. The controller check
  statically enforces the Windows global-mutex, staging-ownership, and
  transactional rollback source contract; the publication-kernel tests execute
  concurrent cancellation/pause races, and only the live Windows lane executes
  the PowerShell provisioning/recovery path. `cargo llvm-cov` is not installed
  in the pinned workspace toolchain, so no numeric coverage percentage is
  claimed.
- The requested `e2e-testing` workflow maps Feature 4 to native Rust, Git,
  GitHub API, PowerShell, process-containment, Windows-service, and Mac/Windows
  proof boundaries. Playwright, screenshots, visual regression, mobile
  emulation, and cross-browser matrices are inapplicable because this feature
  exposes no browser product surface; installing a browser harness would not
  exercise the credential-owning publication boundary.
- Feature 5 is `scripts/restart-recovery-proof-controller.sh` plus the committed
  `restart-recovery-live-e2e.sh` and fixed Windows control. It has no selector
  for repository, service, data directory, executable, test, receipt, or
  harness and accepts Windows JSON only on the isolated descriptor. The output
  category/origin are exactly `restart_recovery_live` and
  `restart_recovery_proof_controller`.
- The committed Feature 5 harness runs the exact real-process Rust-agent test
  `authenticated_uds_local_coding_snapshot_admission_cancellation_and_restart_cleanup`.
  The Mac controller admits only pinned canonical system Git and owner-host
  Cargo/rustc ordinary single-link executables, validates the fixed Cargo home,
  rejects `config`/`config.toml` throughout the checkout ancestry and Cargo
  home, clears caller build/PATH overrides, and executes committed bytes with
  absolute system tools under a system-only PATH. Its self-test proves hostile
  Git, Cargo, Bash, Python, grep, Cargo-runner, and rustc-wrapper paths cannot
  forge the native result and exercises
  accepted, oversized, and signalled receipt handling through a real PTY with
  exact terminal restoration.
  That test proves retained workspace/record recovery, exact cancellation
  cleanup after restart, and partial-state restart cleanup. It is functional
  agent recovery, not host-wide crash containment.
- The Windows Feature 5 run is serialized and explicitly confirmed. It binds
  exact clean controller-reported source HEAD, fixed source/service/data paths,
  the exact local `MIKE-PC\mike` service-owner SID and release image, requires empty
  distributed and Feature Conveyor state with Emergency Pause clear. It stops
  the service, requires no SQLite sidecar, hashes/integrity-checks the complete
  frozen database and bounded backups, then offline-rebuilds exact HEAD with the
  fixed absolute Cargo/Rust/MSVC toolchain. The rebuilt image must be an owner/
  SYSTEM-only ordinary single-link file and byte-match its exact Cargo output. A
  bounded healthy start changes PID; a second stop must yield the identical full
  database SHA and logical/migration continuity. It then restores the original
  exact image and proves a distinct final protocol-5/schema-19 healthy PID.
  Failure attempts exact healthy restoration and emits no receipt; schema-v2
  sanitized JSON separately binds tool, original/restored image, transient
  exact-source rebuild, and frozen-database digests.
- Windows SCM may report the exact configured local account as `.\mike` even
  when the service identity and process argument use `MIKE-PC\mike`; compare the
  two allowed spellings through the fixed account SID. Installed service paths
  may likewise use the `\\?\` extended namespace. Accept it only when both the
  service executable and data-directory argument consistently resolve to the
  fixed ordinary paths; a display-string comparison alone creates false drift.
- Cargo's Windows release executable can be a hard link to the corresponding
  `target\release\deps` artifact and can inherit broader checkout ACLs. That is
  valid build output but not an eligible service-image trust boundary for the
  restart proof. Deployment must stop the service, preserve the exact digest,
  materialize the same bytes as a new single-link service-path file, protect it
  to the fixed owner and SYSTEM, verify unchanged bytes, and restore health.
  `Check` remains read-only and rejects an unprepared image.
- The same Cargo hard-link behavior occurs inside the Feature 5 isolated offline
  rebuild. Do not reject an otherwise exact Cargo output merely because it has
  the expected `deps` hard link, and do not install it directly. Hash the
  ordinary output, byte-copy it into a fresh recovery-root file, apply the exact
  owner/SYSTEM-only ACL, require one link and the unchanged digest, and only
  then compare/install that materialized image. A rejection at this boundary
  must restore the original healthy service, retain no Mac receipt, and leave
  the private recovery directory for explicit owner reconciliation.
- Repeated fixed-toolchain, exact-source Windows links can produce different PE
  byte digests; Feature 5 observed this directly and must not turn an
  unestablished reproducible-build property into a recovery prerequisite. Bind
  the verified materialized exact-source rebuild digest separately, require the
  service-path bytes to equal it during the transient start, then restore and
  re-hash the original installed image. This proves the exact-source image ran
  across idle authoritative recovery while preserving the original service; it
  does not prove that the original image came from that source or that Windows
  builds are reproducible.
- The fixed Windows service's loopback foundation health route reports
  `developer_foundation`; the enrolled mTLS route separately reports
  `developer_remote_master`. `developer_local_master` is not a supported mode
  and must not be accepted as an alias. The pinned Cargo and rustc executables
  can also inherit broad toolchain-directory ACLs even when their paths and
  digests are exact; restart-proof setup must preserve their bytes while making
  each ordinary single-link file owner/SYSTEM-only before the read-only `Check`.
- For the fixed MSVC environment capture, construct `call "<vcvars64.bat>"
  >nul && set` as one PowerShell string argument to `cmd.exe /d /s /c`.
  Wrapping that whole string in another escaped quote pair produces a Windows
  filename/directory syntax error before Cargo runs. That pre-build failure must
  restore the exact healthy service, retain no Mac receipt, and leave the private
  recovery directory for explicit owner reconciliation.
- Windows PowerShell 5.1 represents a native process's ordinary stderr as
  `NativeCommandError`; Cargo uses stderr for normal compile progress. Under a
  global `ErrorActionPreference=Stop`, a direct redirected Cargo call therefore
  aborts a successful build on its first progress line. Scope only that native
  call to `Continue`, redirect all streams to the private bounded log, capture
  `$LASTEXITCODE`, and restore the global fail-closed preference by leaving the
  child scope. Only the captured zero exit code can authorize the next step.
- Feature 5 removes its private transcript after hashing and atomically retains
  only owner-private path-free schema-v2 receipt/raw-digest files. Its self-test
  covers stale invalidation, dirty/wrong/stale/hidden Git state, hostile output,
  path redaction, and cancellation process-group cleanup. The exact master
  startup-quarantine and audit-rollback tests remain separate repository
  evidence. Neither controller nor tests prove active published-effect crash
  recovery, SCM retry policy, signed-helper behavior, Feature 6 streaming,
  admission/activation, signing/notarization, installation, or production.
- Feature 6 adds `scripts/mac-windows-control-streaming-proof-controller.sh` as
  a proof-only owner-run boundary. It accepts no selectors, starts from exact
  clean published `main`, validates normal tracked state and stable committed
  controller/harness definitions, clears caller tool/executable/PATH/exported-
  function/relay overrides into a minimal environment, and executes only the
  committed bridge harness through fixed Bash stdin as `--run-relay`. The
  native claim is the independently signed Swift
  helper's exporter-bound mTLS owner-control projection plus the exact signed
  Rust agent's durable same-stream event cursor advancing after helper/agent
  restart. One exact ordered terminal relay/bridge marker pair is accepted; the
  private transcript is bounded, hashed, and deleted, and its endpoint/stream
  identifiers never enter retained evidence. Digest-first/receipt-last
  publication leaves one owner-private,
  path-free schema-v1 pair bound to source/tree, definitions, stable executable
  identities, transcript digest, and `mac_windows_control_event_streaming_live`
  / `mac_windows_control_event_streaming_proof_controller`. It does not admit
  evidence, approve, activate, change protocol/schema/runtime authority, prove
  current-source binary linkage, Developer ID/notarization/installation,
  unattended operation, or production readiness. The disposable self-test is
  the unit/negative workflow; the committed signed Swift/process/mTLS/SQLite
  relay is native E2E. Playwright is not applicable.
- The committed relay harness emits its event-relay success marker immediately
  before the generic bridge success marker. A proof validator must bind that
  exact ordered pair, including their shared endpoint and advancing cursor/
  reconnect epochs; treating the relay marker as the final transcript line
  rejects every production-shaped run even when a simplified fixture passes.
  Clearing environment variables alone is also insufficient for a Bash proof
  boundary because exported `BASH_FUNC_*` definitions can replace bare-name
  tools. Feature 6 therefore removes imported functions and invokes the harness
  under `env -i` with only fixed owner identity, locale, and tool paths.
- A committed harness executed through Bash stdin has no script pathname in
  `BASH_SOURCE[0]`; a production harness that derives its repository root only
  from that value fails before reaching its live boundary even when simplified
  fixtures pass. Feature 6 uses a distinct fixed internal relay identity plus
  an exact canonical repository root in its minimal child environment. The
  harness accepts that pair only for stdin `--run-relay`, keeps it mutually
  exclusive with restricted-worker stdin mode, unsets it before execution, and
  the disposable fixture reproduces the missing-`BASH_SOURCE` shape. Because
  both proof controllers share the harness, each controller must also clear the
  other mode's caller-supplied internal markers before exporting its own trusted
  handoff; mutual exclusion alone would turn a stray marker into a cross-feature
  denial of every otherwise valid live proof.
- A live harness that falls back to `cargo build` can replace its agent after a
  controller's initial executable-identity snapshot. Even a fully successful
  native stream/reconnect run must then fail closed at the post-run identity
  check and retain no receipt. Feature 6 clears the caller's agent override but
  injects its own already captured exact agent path into the minimal child
  environment, forcing the shared harness to use those stable bytes. Its
  production-shaped disposable fixture requires that exact internal binding so
  unit coverage cannot silently exercise the harness's rebuild fallback.
- Feature 7 is `scripts/windows-owner-evidence-admission.ps1`, a PowerShell 5.1
  Windows-owner onboarding client for the existing schema-v19 admission
  transaction. `Status` and `Check` use the bounded owner-token loopback GET;
  `Admit` additionally requires one exact receipt/sidecar pair and explicit
  `-Confirm`. The token stays in protected master state and an in-process
  Authorization header, never argv, environment, output, receipt, or mTLS.
- The Feature 7 client accepts only the six fixed proof-controller contracts.
  It requires canonical filenames, ordinary non-reparse single-link files,
  protected current-owner/SYSTEM-only ACLs, held no-write/no-delete-share
  handles with pre/post canonical path and stable-identity revalidation,
  controller-specific byte bounds,
  a 64-lowercase-hex plus LF raw sidecar, exact raw receipt SHA-256, one UTF-8
  JSON line with exact controller field order and bytes, no duplicate/reordered/
  whitespace-rewritten keys, exact fixed proof boundary,
  category/origin/schema/controller identity, `passed`,
  a non-future millisecond time, nonzero lowercase commit/tree/digest fields,
  and the fixed review/helper identities. It sends only digest metadata.
- Feature 7 preflights the current per-category revision and Emergency Pause
  revision. An exact current digest returns `evidence_already_admitted` before
  pause or activation rejection; a new
  digest sends at most one contiguous CAS POST. Stale state, pause, activation,
  unsafe evidence, or an ambiguous response fails closed. A retry starts with
  a fresh GET and cannot activate. This adds no schema migration or protocol
  version change and is not approval, execution, release evidence, or external
  proof. Native Rust tests own API/CAS/pause/idempotence/remote-route coverage;
  the PowerShell self-test owns handle/pair/canonical-byte/digest negative coverage.
- For Feature 7, the `unit-testing-test-generate` workflow maps to the strict
  protocol contract, kernel projection/CAS/pause/idempotence/immutability tests,
  owner-token process authentication/redaction tests, and the disposable
  PowerShell 5.1 self-test. Those suites cover valid input, missing and malformed
  state, exact-size and shape bounds, duplicate/reordered/whitespace-rewritten
  JSON, wrong digest/pair/schema/status/fixed identity/boundary, held-handle
  write/delete denial, exact retry reconciliation, stale revisions, Emergency
  Pause, activation, remote-route absence, and redacted errors. Meaningless mocks
  and browser assertions are intentionally absent; no numeric coverage percentage
  is claimed because the pinned workspace does not provide a cross-language
  Rust/PowerShell coverage instrument.
- Feature 7's `e2e-testing` workflow follows the real native boundary rather
  than Playwright: the Rust process E2E starts the actual master binary and proves
  owner-token GET/POST authentication, strict bounded admission, exact retry,
  durable projection, and remote-route absence; the hosted Windows gate runs the
  actual PowerShell 5.1 handle path; and live Windows onboarding runs unchanged
  controller-produced receipt/sidecar bytes through `Check`, confirmed `Admit`,
  and `Status` against the installed schema-v19 service. There is no browser
  surface, so browser automation, screenshots, visual regression, mobile
  emulation, and cross-browser matrices are inapplicable.
- The Feature 7 live onboarding at implementation SHA
  `84f05dec7a48d6c4b3ce595902854df6f9819125` passed the PowerShell self-test and
  admitted five genuine revision-1 references: repository gate
  `a29153a67a27d55eb038d2d6f254300dc33dfe41456513dbc67497e65c5c4ba1`,
  restricted worker
  `c00398461382c21e25da8622507f5562a646ec0bd8c64f5165137fde2c03e963`,
  review provider
  `6cf78b8777d7b8fee5a64f9eba81e09fde2c51a5a97f9fec8f07751ba624d9c8`,
  restart recovery
  `704815efb50939ad123d9ad0d6985af2b0d8b6ac0ffbff068eb5b119921b5670`,
  and Mac/Windows control streaming
  `4a0152f0ec26fee7056622071485842ad9b66105b8ac4d428e9970be36fd1c71`.
  The authoritative projection remained inactive, unpaused, and 5/6; no
  activation was attempted. The sixth GitHub-publication proof remained absent
  because the otherwise valid Windows GitHub CLI account used plaintext fallback
  storage. The publication controller correctly requires OS credential-store
  protection and must not accept, migrate, print, or reuse that plaintext token;
  interactive secure reauthentication remains an owner identity action.
- Copying proof pairs onto Windows is not sufficient onboarding. Preserve the
  controller bytes exactly, then explicitly set each ordinary single-link file's
  owner to the current Windows owner and replace inherited access with exactly
  owner and SYSTEM FullControl before `Check`. An elevated process can otherwise
  leave Administrators as owner. Apply the same narrow reconciliation to a
  legacy `development.token` ACL without reading or printing its contents; do
  not broaden this into recursive data-directory permission rewriting.
- When validating native Windows tools, fully consume `gh version`, capture
  `$LASTEXITCODE` immediately after that native invocation, and only then inspect
  the first output line. Piping the native command through
  `Select-Object -First 1` can terminate the producer early and turn a valid
  executable into a false failure. The fixed version check changes no credential,
  publication, or repository authority.
- GitHub CLI `2.96.0` writes a successful `auth status` projection to native
  stderr. Windows PowerShell 5.1 converts that stream into
  `NativeCommandError`, so the publication control's global
  `ErrorActionPreference=Stop` must not wrap the call directly. Scope only the
  fixed `auth status` invocation to `SilentlyContinue`, discard all of its output,
  preserve whether global `$LASTEXITCODE` was initially absent or present, clear
  and capture it inside that child scope, reject a null sentinel when launch
  never occurred, and accept only zero. The Windows-hosted self-test starts with
  an absent global exit-code variable and covers successful stderr, nonzero exit,
  launch failure, and restoration of that absence. This does not relax the
  plaintext-token scan or any surrounding fail-closed check.
- `Stop-Service` returning is not sufficient authority to replace the Windows
  master executable. A GitHub-publication provision or rollback must poll the
  exact service identity for `State=Stopped` and `ProcessId=0` within the bounded
  window before Cargo or file restoration writes that executable. The stopped
  query may allow only the configured executable leaf to be missing so a verified
  backup remains recoverable; every parent and the service image path stay exact.
  Startup still requires the leaf and binds the healthy process ID to the exact
  running service before accepting the deployment. This prevents a transient
  executable handle from turning a safe provision into an access-denied rebuild
  or ambiguous rollback.
- `std::fs::canonicalize` returns `\\?\`-prefixed paths on Windows, while Git
  for Windows rejects that verbatim form when it is supplied to `--git-dir` or
  `--work-tree` (the live fetch exits 128 with “not a git repository”). Keep the
  canonical path for ancestry, containment, and executable-identity validation,
  then remove only a recognized local `\\?\` prefix or convert
  `\\?\UNC\server\share` to `\\server\share` at the fixed Git child-process
  argument boundary. Reject non-absolute results and do not broaden the
  normalization to caller-selected paths or stored authority.
- Native Mac relay tests must keep their client read timeout above the server's
  forty-five-second peer-code-identity validation bound. A shorter client timeout
  can surface Security.framework latency as `WouldBlock` before the server's
  bounded identity decision; the Rust test client derives forty-seven seconds from the
  shared server constant, and the production Swift authenticated-agent transport uses
  the same forty-seven-second total request bound, so the
  recovery assertion observes the actual transport contract. The agent SIGTERM
  E2E's backend-start marker must use the same derived bound: five seconds
  passes alone but races the valid identity decision under parallel workspace
  load, creating a false missing-marker failure before shutdown is exercised.
  After one full Security.framework success, the server caches only the exact
  eight-word `LOCAL_PEERTOKEN` (which includes PID generation) for its own lifetime;
  the fixed 64-token cap avoids unbounded identity state, and new or rejected tokens
  always retain the pre-frame validation boundary.
- Security.framework audit-token verification can exceed the production forty-five-second
  bound on a loaded Mac. The core ordering test therefore injects a deterministic rejecting
  verifier to prove that peer identity is checked before any frame read, while the separate
  local-peer test retains the real Security.framework audit-token, accepting-requirement,
  hardened-profile rejection boundary. This keeps the ordering test load-independent without
  widening the production timeout or accepting a timeout as identity evidence.
- A committed-byte proof is not contained merely because its primary executable
  is digest-pinned. Every interpreter and result parser must resolve through a
  fixed validated system identity, and build tools must reject configuration
  discovery that can redirect runners or wrappers. Feature 5 therefore treats
  hostile PATH entries and Cargo configuration ancestry as unit/self-test
  boundaries, not as operator assumptions.
- For Feature 5, the `unit-testing-test-generate` workflow maps to the controller
  self-test's disposable Git/process/PTY fixtures: success, absent prerequisites,
  strict CLI, malformed/duplicate/oversized evidence, boundary sizes, redaction,
  stale-output invalidation, hostile paths/configuration, cancellation, process-
  group cleanup, and terminal restoration. The `e2e-testing` workflow maps to
  the real Rust agent restart plus Windows SCM/SQLite recovery path. Playwright,
  visual regression, and cross-browser testing are inapplicable because no
  browser surface participates in this authority boundary.
- Feature 5 live closeout passed on exact published implementation merge
  `44e912ef8b6e4f499d89107cf6414887b8957f3d`. The owner-run controller proved
  the real Rust-agent retained-workspace restart path and the fixed Windows
  SCM/schema-v19 idle-authority recovery path. Windows changed from healthy PID
  `20896` to distinct restored healthy PID `38380`, preserved queue revision 27,
  inactive activation, and 14 migration backups, and revalidated protocol 5,
  schema 19, Emergency Pause clear, and no queued, leased, or active work. The
  original/restored service digest was
  `a9cc70507cf972dd2a62484ad2573e609bed1cef1692148bdf67fa07938ca36c`;
  the separately bound transient exact-source rebuild digest was
  `6ef271b79a6f1f6d3e61129845bc7015cba7000d45dd4c319878ac83ea05552c`.
  The frozen database, migration-backup set, and logical-continuity digests were
  `96c5adafdf82854dabcee2f78052de36b2d1e1da8feb9a61623ad1be98ec753b`,
  `67dd190058fa8bd537477004400ffafd6b4e5b8309faaf3a6a90e1b1dde66dce`,
  and `f5de3884b7a1266961b3550b49c34742fac674d421c7d11e763fec41c33f8a3c`.
  The retained owner-private receipt SHA-256 is
  `9ba39dc9b9663e0fd7783b46b760848b13572f5e19e7a85b47c7b8cbab7333b0`.
- Documentation, design, safety rules, the conversation-derived hardening facts
  above, focused unit/self-test coverage, native E2E coverage, canonical local
  release verification, and independent high-risk review were all completed.
  Exact published-merge hosted runs `32400583370` (Release Local) and
  `32400583357` (Windows Distributed) passed. No evidence was admitted, no
  activation was attempted, and Feature 5 does not prove reproducible Windows
  builds, installed-image source provenance, active-effect crash recovery, SCM
  retry policy, Feature 6 control streaming, signing, notarization, or
  production readiness.
