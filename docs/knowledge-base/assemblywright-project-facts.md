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
  compatibility contracts, because they bind installed state or signed
  artifacts: environment variable names, Keychain and Application Support
  namespaces, the Windows service name, the `com.nobiletechnology.jarvis`
  code-signing identity and its `.core` suffix, the `JarvisMacApp` executable
  name inside the bundle, and the bundled `jarvis-cli` filename. Renaming any of
  these changes code-signing identity or orphans installed state, so they are
  not cosmetic.
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
