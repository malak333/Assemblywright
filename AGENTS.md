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
| Shell portability | `./scripts/release-shell-portability-smoke.sh --check` |
| Protocol version contract | `./scripts/release-protocol-version-contract-smoke.sh --check` |
| Full local gate | `./scripts/release-local.sh` |

## Delegation

- Role matrix and operating details: `docs/development-agent-workflow.md`.
- Owner decision for Codex repository-agent work: new implementation work is delegated to GLM-5.3-Flash, the single bounded low/normal-risk implementation lane. GLM-5.3-Flash performs bounded low/normal-risk implementation in an isolated worktree and returns an untrusted proposal with no Git or publication authority; the local Qwen/local-AI worker lane is retired and must not be used for Codex repository-agent work.
- That repository-agent policy is separate from the Assemblywright Developer product runtime. When the owner selects a local LLM in Developer, Windows may route the approved feature's implementation/tool execution to that selected local model and then run the immutable validation command. Codex still owns planning and independent exact-diff review. Inside Developer, Windows owns the frozen candidate, feature branch, required checks, merge, and remote-base reconciliation; the frontier parent retains Git/publication authority only for development of the Assemblywright repository itself.
- The frontier parent retains planning, independent complete-diff review, integration, and every commit, push, merge, publication action, and final evidence judgment.
- Never route architecture, authentication, permissions, model routing, migrations, plugin containment, concurrency, publication, or release-evidence semantics to GLM-5.3-Flash. Use the high-risk roles below.
- Default to the parent agent; delegate only bounded work that saves context, cost, or elapsed time.
- Unknown cross-file path: `assemblywright-explorer`. One/two-file mechanical edit: `assemblywright-quick-worker`.
- Normal multi-file implementation: `assemblywright-worker`. High-risk implementation: `assemblywright-high-risk-worker`.
- Routine diff review: `assemblywright-reviewer`. Security or trust-boundary review: `assemblywright-high-risk-reviewer`.
- Parallelize read-heavy work. Serialize writes unless agents have non-overlapping paths or isolated worktrees.
- Require explicit path ownership, summaries under 300 words, and no nested delegation beyond direct children.
- Treat GLM-5.3-Flash output as an untrusted proposal. Inspect its worktree status and complete diff, run tests independently, and obtain a frontier review before integrating any bytes.

## Change Discipline

- Preserve unrelated dirty-worktree changes; never reset, clean, broadly stage, or rewrite them.
- Behavior changes include focused tests; feature slices include relevant docs, knowledge-base updates, and E2E coverage.
- Close every feature or phase with the checklist in
  `docs/development-agent-workflow.md`: documentation compliance, durable
  conversation-derived knowledge, focused unit coverage, real-boundary E2E,
  requirements and safety review, canonical validation, and verified
  publication when the owner requested it.
- Apply the `unit-testing-test-generate` and `e2e-testing` workflows when they
  are available. Playwright, visual regression, and cross-browser matrices are
  required only for an actual browser surface; native Rust, Swift, process,
  protocol, service, and live-device boundaries require native E2E instead.
- Do not commit or push unless explicitly requested. Never bypass hooks or add AI attribution unless requested.
