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
invoked by hand. `install` and `uninstall` both require `--confirm`, and
owner-account installation requires `--credentials-stdin` because passwords must
never appear in argv.

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
- The Feature Conveyor kernel is persistence only. It has no HTTP/API, worker
  dispatcher, repository execution, review provider, publication coordinator,
  Mac queue UI, or autonomous activation.
- The Mac agent's fixture and MLX lanes are default-off, singleton, and
  no-retention. They add no remote planning, repository, tool, credential,
  network, Codex, Git, publication, or unattended authority.
- Live closeouts (`mac-windows-bridge-live-e2e.sh --run-fixture` and
  `--run-mlx`) are owner-controlled external evidence, not release evidence.

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
- Do not commit or push unless explicitly requested.

## Safety Guardrails

- Fail closed. Ambiguity quarantines and blocks rather than guessing.
- Planning and action stay separate. Models propose; the owner authorizes.
- Redaction is structural: audit and event surfaces carry metadata and digests,
  never raw payloads or credentials.
- Cancellation dominates completion and suppresses late output.
- Emergency pause blocks new leases and publication.
- Audit evidence commits in the same transaction as the state transition it
  describes.
- Result acceptance is bound to the exact leased attempt.
- Automatic retry is allowed only when evidence proves repetition cannot
  duplicate an effect.
