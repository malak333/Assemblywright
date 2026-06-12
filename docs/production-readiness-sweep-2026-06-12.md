# Production Readiness Sweep - 2026-06-12

This note records the autonomous production-readiness sweep state after PR #247
merged and while PR #248 and PR #249 were still awaiting hosted release-gate
completion. It is a release-governance artifact, not production evidence by
itself. Use the checked-in code, PRs, and command output as proof.

## Live Readiness Snapshot

Commands:

```sh
cargo run -p jarvis-cli -- release readiness --format json
cargo run -p jarvis-cli -- release evidence-status --format json
```

Observed on 2026-06-12 UTC from `main` at `d87ba20` after PR #247:

- `production_ready: false`
- `verified_feature_count: 16`
- `pending_feature_count: 1`
- Remaining pending feature: `live_voice_loop`
- `evidence-status complete: false`
- `satisfied_count: 3`
- `missing_count: 6`
- `invalid_count: 0`

The missing production evidence items are still external/manual gates: signed
app zip, signed installer package, signed-distribution provenance report,
live-device QA report, plugin-trust QA report, and final release evidence
bundle. The locally present app bundle, app executable, and bundled core are
presence/metadata evidence only; they do not prove Developer ID signing,
notarization, stapling, clean-profile install, Finder launch, live-device QA,
plugin-trust QA, or final evidence bundling.

## Open PR State

- PR #248 binds plugin-trust QA reports to the current release version and keeps
  wrong-version plugin reports from clearing readiness.
- PR #249 hardens the signed-distribution runbook E2E contract by pinning JSON
  parity, the exact signed-artifact evidence key set, operator commands, and
  manual-check handoff text.

Both PRs were pushed from isolated worktrees and were waiting on the hosted
`Release local gate` at the time this note was written. The main checkout passed
`./scripts/release-local.sh` locally after PR #247.

## Current Architecture Phase

Jarvis remains a production-shaped local assistant foundation with conservative
release boundaries:

- Repo-owned gates cover Rust/CLI/Swift tests, local operator QA, unsigned
  distribution layout launch, release evidence inventory, evidence script
  self-tests, and release runbook rendering.
- Swift release readiness now fails closed when evidence-status refresh fails
  after a readiness payload would otherwise look production-ready.
- Production-ready language remains blocked until owner-recorded external
  evidence exists for signed/notarized distribution, clean-profile install,
  live-device QA, plugin-trust QA, and final evidence bundling.

## End-Goal Production Phase

The target production state requires a signed and notarized packaged app whose
installed clean-profile behavior is manually validated and whose external
release evidence is archived:

- Developer ID signed and notarized app zip plus `/Applications` installer.
- Clean-profile install, Finder/LaunchServices launch, supervised bundled core,
  command/audit/memory/scheduler/plugin/pause/diagnostics/restart QA.
- Live microphone, Speech permission, spoken transcript handoff, live audio
  output, and notification validation on a real Mac.
- Plugin marketplace review, malware scan, signed publisher policy, OS sandbox,
  and host-level egress evidence.
- Final evidence bundle generated after all child reports and validated by
  `release-evidence-doctor.sh --assert-complete`.
