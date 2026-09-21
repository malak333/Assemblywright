# Developer Auto AI Repair Implementation Plan

Status: implementation completed in the repository working tree on
`codex/developer-auto-ai-repair`; final canonical, Windows, installed-app, visual,
hosted, and publication evidence remains separately reported.

The accepted requirements and Decision Log are in
[`developer-auto-ai-repair-design.md`](developer-auto-ai-repair-design.md). This
plan does not relax that contract.

## Delivery order

### 1. Persisted contract and migration

Owned paths:

- `crates/assemblywright-master/src/developer_main.rs`

Add default-off global enablement, the validated `1...100` maximum, per-feature
limit snapshot, automatic-repair lifecycle/reason/epoch, and bounded evidence
records. Advance the queue snapshot key with aliases and a backup-first migration.
Define the absolute shared escalation cap, 300 escalation-ledger slots, 104 review
slots, and atomic admission rules. Preserve old queue entries and manual proposal
evidence.

Unit-first proof covers defaults, migration, malformed and maximum values, stale
revision, exact replay, limit snapshots, counter preservation, evidence reservation,
and every lifecycle transition.

### 2. Revision-bound owner control and projection

Owned paths:

- `crates/assemblywright-master/src/developer_main.rs`
- `apps/mac/Sources/AssemblywrightMacApp/DeveloperRunnerView.swift`
- `apps/mac/Tests/AssemblywrightMacAppTests/DeveloperRunnerClientTests.swift`
- `apps/mac/Tests/AssemblywrightMacAppTests/DeveloperRunnerTests.swift`

Extend the authenticated status projection and add one atomic control carrying
`enabled`, `max_escalations`, and `expected_revision`. Accept success only when the
returned values match and the runner revision advances exactly once. Enabling an
eligible failed feature snapshots the limit and starts it; disabling persists the
new epoch and cancellation intent before signalling active work.

Tests bind strict fields, range checks, stale and conflicting replay, exact
acknowledgement, and cancellation ordering.

### 3. Windows-owned automatic repair state machine

Owned paths:

- `crates/assemblywright-master/src/developer_main.rs`
- narrowly required helpers under `crates/assemblywright-master/src/`

Reuse the existing three ordinary attempts without widening their test/configuration
authority. Add automatic escalation packets sourced from exact failure/review
evidence rather than project chat. Record `automatic_failure` versus `manual_chat`,
freeze and hash proposals, issue policy-bound authorization receipts, revalidate
exact bytes, and apply through the existing atomic path.

Continue only for validation failure, Codex rejection, no-op, or duplicate
candidates. Operational, quota, deadline, persistence, drift, path, publication,
and ambiguous failures enter a durable hold or quarantine. Gate every completion by
the active epoch. Keep Auto-run independent and preserve publication barriers.

Unit tests cover success, 100-attempt exhaustion, earlier manual counts, no-op and
duplicate continuation, provider/malformed stops, file-cap hold, test/config edits,
review rejection, toggle cancellation, late output, clean restart recovery, and
ambiguous application quarantine.

### 4. Native Swift owner interface

Owned paths:

- `apps/mac/Sources/AssemblywrightMacApp/DeveloperRunnerView.swift`
- `apps/mac/Tests/AssemblywrightMacAppTests/DeveloperRunnerTests.swift`
- `apps/mac/Tests/AssemblywrightMacAppTests/DeveloperRepairEscalationTests.swift`

Place the independent checkbox-style toggle and three-digit numeric control beside
Auto-run. Keep the maximum editable while disabled. Show pending mutation state,
feature snapshot, ordinary/escalation progress, elapsed step time, exact hold reason,
and admissible next action. Keep Stop, Emergency Pause, and disable available while
manual mutation controls are unavailable.

Verify decoding compatibility, input normalization, exact acknowledgement,
conflicts, active cancellation, accessibility labels/value announcements, and
plain-language limit/hold/quarantine messages.

### 5. Native process E2E

Owned paths:

- `scripts/developer-runner-auto-repair-e2e.py` (new)
- `scripts/release-local.sh`
- fixture helpers only when narrowly required

Use deterministic local model and Codex-review fixtures against the real runner
HTTP, filesystem, persistence, and validation-process boundaries. Cover ordinary
attempts followed by escalation, eventual success, cumulative counting, configured
and absolute maximums, source/test/configuration edits, unchanged command string,
review-rejection continuation, app-independent execution, toggle-off termination,
late-result rejection, clean restart, operational hold, and ambiguous quarantine.

This native Rust/Swift/process surface does not use Playwright or browser matrices.
Installed Windows and visual macOS proof remain separately reported boundaries.

### 6. Documentation and canonical validation

Owned paths:

- `DESIGN.md`
- `docs/safety-rules.md`
- `docs/developer-auto-ai-repair-design.md`
- `docs/developer-build.md`
- `docs/developer-build-testing.md`
- `docs/build-test-commands.md`
- `docs/knowledge-base/assemblywright-project-facts.md`
- `scripts/release-docs-drift-smoke.sh`

Replace target-only wording only after implementation evidence exists. Record the
weaker owner-account validation boundary, automatic policy authority, migration,
recovery, test commands, and evidence limitations.

Run focused Rust and Swift tests, the new native E2E on macOS and Windows where
available, documentation/naming/protocol/shell contracts, `cargo fmt --check`,
`git diff --check`, and `./scripts/release-local.sh`. Distinguish repository proof
from installation, native Windows execution, visual UI, signing, notarization,
hosted checks, and publication.

### 7. Independent review and closeout

Route the completed trust-boundary diff to `assemblywright-high-risk-reviewer`.
Resolve all blocking findings and rerun affected tests. Close out with explicit
verdicts for design/safety compliance, knowledge-base updates,
`unit-testing-test-generate`, native `e2e-testing`, canonical validation,
publication, and deployment. Do not commit or push unless the owner separately
requests publication.

## Sequencing constraints

- Serialize changes to `developer_main.rs`; it owns persistence, control, loop, and
  recovery invariants.
- Do not expose the Swift toggle until the backend projection and mutation contract
  are covered by focused tests.
- Do not enable automatic application until evidence reservation, epoch cancellation,
  and restart quarantine tests pass.
- Preserve unrelated working-tree files and never stage broadly.
- Stop implementation if the accepted design, safety rules, or observed runtime
  contract conflict; revise and re-review the design before continuing.
