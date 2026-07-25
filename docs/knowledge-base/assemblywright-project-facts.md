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
  `assemblywright-master`; the legacy `jarvis*` binary aliases were removed.
- A specific set of `JARVIS_*` / `jarvis` identifiers survives deliberately as
  compatibility contracts, because they bind installed state, issued
  credentials, or signed artifacts: environment variable names, Keychain and
  Application Support namespaces, the Windows service name `JarvisMaster` and
  its `%LOCALAPPDATA%\Jarvis\master` state directory, the
  `com.nobiletechnology.jarvis` code-signing identity and its `.core` suffix,
  the `JarvisMacApp` executable name inside the bundle, the bundled
  `jarvis-cli` filename, the `EXPORTER-Jarvis-Developer-Mode-v1` TLS exporter
  label, the `urn:jarvis:device:<uuid>` certificate SAN URI baked into every
  issued device certificate, and the protocol-version-1 fixture capability
  provider and model (`jarvis-fixture`, `jarvis-fixture-v1`). Renaming any of
  these changes code-signing identity, voids issued certificates, breaks the
  wire contract without a protocol version bump, or orphans installed state, so
  they are not cosmetic.
- `./scripts/release-naming-contract-smoke.sh` is the gate for that list. It
  runs in `release-local.sh` as `--check` plus `--self-test` and fails in both
  directions: if a legacy `jarvis*` crate, binary alias, or SwiftPM product
  alias reappears, and if a preserved identifier is renamed or its documented
  reason is dropped from `docs/brand.md` or this file. A rename pass that trips
  it should change the contract and its migration, not the guard.
- Prose, error messages, and CLI runbook output are *not* on that list. The
  first rename pass left `cargo run -p jarvis-cli`, `` `jarvis-protocol` ``,
  `` `jarvis-master` ``, and `jarvis-agent` in emitted runbooks, error strings,
  and one `--bin jarvis-master` invocation in
  `.github/workflows/windows-protocol.yml` that no longer resolved. When
  renaming a crate, sweep emitted strings and CI invocations too, not just
  manifests.
- The SwiftPM product is `AssemblywrightMacApp`, and the built executable is
  named after the product, not the target. Packaging copies it into the bundle
  under the contract name via `SWIFT_APP_PRODUCT` in
  `scripts/package-distribution.sh`. Swift target names remain `Jarvis*`
  because module names are internal. The product-facing app and release artifact names are
  `Assemblywright.app` and `Assemblywright-<version>.*`.
- The GitHub repository was renamed from `Jarvis` to `Assemblywright`, so the
  canonical remote is `https://github.com/malak333/Assemblywright`. GitHub
  redirects the former `malak333/Jarvis` URL, so older clones keep fetching and
  pushing, but new references must use the current URL. The local working
  directory is `Assemblywright`.

## Repository Housekeeping Already Completed

- The rename is finished end to end: the crates are `assemblywright-*` with no
  legacy binary aliases, `apps/mac/Package.swift` exports only
  `Assemblywright*` products, the local working directory is
  `~/Antigravity/Assemblywright`, and the canonical remote is
  `malak333/Assemblywright`. Do not re-plan any of these as outstanding work.
- The `.worktrees/` directory of finished-branch worktrees is gone, and
  `git worktree list` should show only the primary checkout. If stale worktrees
  reappear, prune them with `git worktree prune` before adding new ones.
- The `codex/*` branches were deleted locally and on `origin` after their
  squash-merge PRs landed. `origin` should carry `main` only. Their tip commits
  are recorded in `~/Antigravity/codex-branch-cleanup-manifest.txt`, which is
  outside the repository on purpose so a deleted branch stays recoverable with
  `git push origin <sha>:refs/heads/<branch>`. That manifest is the only record;
  do not delete it while any recovery might still be wanted.

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
  `com.nobiletechnology.jarvis` identifier and explicitly assigns the bundled
  CLI `com.nobiletechnology.jarvis.core`; package bundle-ID overrides are
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
  validate. `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external` is set only after
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
