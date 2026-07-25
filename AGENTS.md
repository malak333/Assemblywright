# Assemblywright Agent Instructions

## Architecture And Safety

- Read `DESIGN.md` and `docs/safety-rules.md` before architectural or behavior changes.
- Windows `assemblywright-master` owns durable authority: the queue, device lifecycle, identity, policy, and audit. `assemblywright-protocol` owns the wire contracts. `assemblywright-agent` is the bounded Mac worker. `assemblywright-core` is the local transport plus release evidence only. Swift owns the macOS Developer Mode UX.
- The pre-pivot assistant surface is removed. Do not reintroduce a conversation runtime, model routing, plugins, personal memory, a scheduler, voice, or trusted wake.
- Preserve fail-closed policy, planning/action separation, redaction, cancellation, emergency pause, and audit evidence.
- Keep repository validation distinct from signing, notarization, live-device QA, and owner-recorded external evidence.

## Toolchains

- Rust workspace: Cargo with the pinned `rust-toolchain.toml`.
- macOS app: Swift Package Manager under `apps/mac`.
- Canonical commands and proof boundaries: `docs/build-test-commands.md`.

## Focused Commands

| Task | Command |
| --- | --- |
| Codex workflow | `./scripts/validate-codex-workflow.sh` |
| Rust format | `cargo fmt --check` |
| Core test | `cargo test -p assemblywright-core <filter> -- --nocapture` |
| Conveyor kernel | `cargo test -p assemblywright-master --test feature_conveyor_kernel` |
| Agent relay E2E | `cargo test -p assemblywright-agent --test local_relay_e2e` |
| Swift test | `swift test --disable-sandbox --package-path apps/mac --filter <test>` |
| Docs contract | `./scripts/release-docs-drift-smoke.sh` |
| Naming contract | `./scripts/release-naming-contract-smoke.sh --check` |
| Full local gate | `./scripts/release-local.sh` |

## Delegation

- Role matrix and operating details: `docs/development-agent-workflow.md`.
- Default to the parent agent; delegate only bounded work that saves context, cost, or elapsed time.
- Unknown cross-file path: `assemblywright-explorer`. One/two-file mechanical edit: `assemblywright-quick-worker`.
- Normal multi-file implementation: `assemblywright-worker`. High-risk implementation: `assemblywright-high-risk-worker`.
- Routine diff review: `assemblywright-reviewer`. Security or trust-boundary review: `assemblywright-high-risk-reviewer`.
- Parallelize read-heavy work. Serialize writes unless agents have non-overlapping paths or isolated worktrees.
- Require explicit path ownership, summaries under 300 words, and no nested delegation beyond direct children.

## Change Discipline

- Preserve unrelated dirty-worktree changes; never reset, clean, broadly stage, or rewrite them.
- Behavior changes include focused tests; feature slices include relevant docs, knowledge-base updates, and E2E coverage.
- Do not commit or push unless explicitly requested. Never bypass hooks or add AI attribution unless requested.
