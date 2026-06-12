# Production Readiness Sweep - 2026-06-12

This note records the autonomous production-readiness sweep state after PR
#247, PR #248, PR #249, PR #253, PR #254, PR #256, PR #257, and the release
handoff command-guidance slice merged. It is a release-governance artifact, not
production evidence by itself. Use the
checked-in code, PRs, and command output as proof.

## Live Readiness Snapshot

Commands:

```sh
cargo run -p jarvis-cli -- release readiness --format json
cargo run -p jarvis-cli -- release evidence-status --format json
```

Observed on 2026-06-12 UTC from `main` at `68e2c5a` after PR #257. The
evidence-status counts below came from the primary checkout after
`./scripts/release-local.sh` regenerated the local unsigned distribution
layout; fresh worktrees can report those generated local app paths as missing
until the distribution lane runs there.

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

## Merged PR State

- PR #247 makes the Swift Release readiness model fail closed when
  evidence-status refresh fails after a readiness payload would otherwise look
  production-ready.
- PR #248 binds plugin-trust QA reports to the current release version and keeps
  wrong-version plugin reports from clearing readiness.
- PR #249 hardens the signed-distribution runbook E2E contract by pinning JSON
  parity, the exact signed-artifact evidence key set, operator commands, and
  manual-check handoff text.
- PR #253 exposes the redacted memory retention plan in the Swift Memory tab
  without adding autonomous purge/rewrite behavior.
- PR #254 exposes the signed-distribution, live-device, and plugin-trust
  runbooks through read-only IPC endpoints and renders them in the Swift Release
  tab.
- PR #256 embeds release-core command evidence capture, the
  `task:<uuid>`/`audit:<uuid>` evidence-ID rule, and external evidence-mode
  evidence-status/readiness commands in the generated live-device QA env
  template.
- PR #257 mirrors that live-device evidence-capture and endpoint-aware
  external evidence-mode guidance in the CLI fallback runbook and IPC
  `/release/live-device-runbook` payload, with Rust and Swift coverage.
- The release handoff command-guidance slice mirrors that same live-device
  evidence-capture and endpoint-aware external evidence-mode sequence in
  `package-distribution.sh --check` and `release-evidence-doctor.sh --check`,
  with shell self-tests pinning the guidance.

These PRs were pushed from isolated worktrees, merged through GitHub, and
cleaned up after merge. There were no open PRs after PR #257 merged. The final
merged `main` checkout passed `./scripts/release-local.sh` locally for the
code-changing runbook guidance slice, and the public GitHub PR
`Release local gate` passed for PR #256 on run `27412085979` and PR #257 on run
`27413191322`.

## Current Architecture Phase

Jarvis remains a production-shaped local assistant foundation with conservative
release boundaries:

- Repo-owned gates cover Rust/CLI/Swift tests, local operator QA, unsigned
  distribution layout launch, release evidence inventory, evidence script
  self-tests, and release runbook rendering.
- The Swift Release tab now shows read-only signed-distribution, live-device,
  and plugin-trust runbooks from IPC when the running core exposes them.
- The generated live-device QA template and live-device runbook now both tell
  operators to capture a release-core `jarvis command "status check" --json`
  result, record the returned `task:<uuid>` or task-associated `audit:<uuid>`,
  and rerun evidence-status/readiness against the release endpoint with external
  evidence mode.
- Package preflight and evidence doctor next-step guidance now show that same
  release-core command evidence path before plugin-trust and final-bundle
  handoff, so all repo-owned release handoffs point operators at the same
  evidence sequence.
- The Swift Memory tab now shows the redacted memory retention-plan action queue
  without performing deletion, rewrite, or autonomous retention actions.
- Swift release readiness now fails closed when evidence-status refresh fails
  after a readiness payload would otherwise look production-ready.
- Plugin-trust QA evidence is now release-version bound, and stale or missing
  plugin-trust report versions cannot clear readiness.
- Signed-distribution runbook E2E pins JSON compatibility plus exact evidence,
  command, and manual-check handoffs.
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
