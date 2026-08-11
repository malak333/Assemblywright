# Release Checklist

Use this checklist before tagging or publishing any Assemblywright release.
Keep the evidence local-first unless the owner explicitly approves hosted
infrastructure. This checklist separates repository validation from
owner-recorded external evidence; a green local gate is never a
production-readiness claim.

## Scope Check

Before starting a release pass, confirm the claim you intend to make.

- Name the exact surface being released and the exact evidence that supports
  it. Do not describe autonomous dispatch, repository mutation, review-provider
  invocation, or GitHub publication as implemented.
- Confirm `docs/feature-conveyor-design.md` still marks the implemented slice
  accurately. The repository kernel is default-inert and exposes the
  owner-token-authenticated loopback read-only
  `GET /v1/feature-conveyor/status` observation seam. Its queue-, pause-, and
  optional feature-lifecycle-revision-bound owner-guidance labels remain
  display-only and do not establish claimability.
  The dedicated remote `GET /v1/distributed/feature-conveyor/status` is
  read-only, requires an accepted exporter-bound MacBridge session, denies
  other roles, and forwards no owner token. The Mac renders it as text, never a
  callable owner action. Separately confirm schema v8 designates exactly one
  current non-fixture owner-control MacBridge through the owner-token loopback
  route, and that only its accepted, revalidated session can enqueue one strict,
  already-approved, queue/designation/pause-revision-bound specification. The
  signed helper action requires `--confirm`, bounded stdin, a strict redacted
  receipt, and session close; the app remains read-only. Separately confirm the
  owner-token loopback repository-grant routes enforce contiguous compare-and-
  set and Emergency-Pause-revision binding, allow revocation while paused,
  expose only current digest metadata, and remain absent from enrolled-device
  mTLS. They perform no repository access. Separately confirm the owner-token
  loopback repository preflight is bound to the exact active registration
  grant and Emergency Pause revision, limits the base branch to one component, uses only bounded point-in-time
  filesystem identity inspection with no Git process or configuration load,
  returns and stores no path, and commits only redacted audit plus a path-free
  receipt. On Windows it must hold non-reparse identity-chain handles, reopen
  and compare the complete canonical pathname and identities immediately before
  the final atomic recheck, and retain the fresh handles through it. Confirm the five-second timeout claim is
  limited to each filesystem-observation await and does not cover authentication
  or database lock/audit latency. This does not create a cross-filesystem-and-
  SQLite snapshot. It does not prove clean-tree or content state and creates no reusable
  snapshot or claimability. A checkout containing valid `.git/worktrees`
  metadata is deliberately ineligible; do not prune or delete that metadata to
  force a pass. Use a dedicated standalone checkout for positive proof, obtain
  the path-free receipt, record the next revoked grant revision, confirm it is
  inactive, and remove only the disposable checkout created for the proof.
  Windows fixtures must use the final normalized DOS path from the held
  directory handle rather than a verbatim `\\?\` path from
  `std::fs::canonicalize`. The slice
  also exposes a separate owner-token loopback snapshot-claim POST. Confirm it
  is absent from the remote mTLS router; rejects stale head/dependency/lease,
  queue/pause/grant/provider bindings; reads raw objects without Git process,
  source config, hooks, attributes, credentials, global config, PATH,
  alternates, links/reparse entries, gitlinks, remotes, or hardlinks; returns no
  path; copies only the exact current commit/tree/blob graph with shallow
  metadata and no parent/deleted history; fail-fast serializes bounded snapshot
  work while retaining the reservation after HTTP timeout until the blocking
  task exits; and atomically binds one durable snapshot receipt to one lease and
  redacted audit. Prove failure/cancellation leaves no lease or referenced snapshot,
  and restart cleans unreferenced state and quarantines a finalized lease. The
  snapshot also contains one deterministic bounded transfer bundle whose raw
  object graph and file manifest are independently digest-bound.
  Schema v10 separately exposes one owner-token loopback-only metadata coding-
  dispatch POST. Confirm it is absent from the enrolled-device router, binds
  the exact feature/specification/lifecycle, feature lease, snapshot ID/digest,
  queue/pause revisions and current singleton `local.coding.v1` worker
  registration, and atomically commits its queued step, immutable binding,
  event, and redacted audit. Prove stale authority, cancellation, Emergency
  Pause, lifecycle departure, registration drift, and restart quarantine block
  lease or acknowledgement. Confirm protocol and native-agent tests accept no
  repository bytes/path, commands, credentials, or mutation claim. Then prove
  the separate default-off snapshot lane authorizes only exact active
  attempt/lease/cancellation/snapshot bindings, rechecks around every bounded
  read, rejects stale or out-of-order frames, and materializes through the
  authenticated Mac local socket into fresh private per-attempt state. Prove
  object/chunk/bundle/path/digest validation, no remote/hooks/links, and cleanup
  on completion, cancellation, failure, shutdown, and restart. This slice
  exposes no retained worker checkout, coding execution, mutation/result
  integration, review provider, publication coordinator, Mac control UI, queue
  advancement, or autonomous activation.
- Confirm `docs/architecture-map.md` matches the code for any surface that
  changed in this cycle.
- Confirm the version is consistent:

```sh
./scripts/release-version-consistency.sh --check
```

## Code Gate

Run the canonical local gate and treat any failure as blocking:

```sh
./scripts/release-local.sh
```

Nothing in this gate signs, notarizes, staples, installs, or validates on a
live device. It proves the workspace builds, tests pass including ignored
release proofs, the crates package, the unsigned distribution layout is valid
and launches in an isolated HOME with Developer Mode default-off, the release
runbooks render, the evidence preflights and self-tests pass, and the Swift
package builds and tests.

## Safety Gate

Re-read `docs/safety-rules.md` and confirm the change preserves:

- Fail-closed policy. Ambiguous repository, provider, external-effect, review,
  or publication boundaries quarantine and block rather than guessing.
- Planning and action separation. Models propose; the owner authorizes.
- Sensitivity classification and redaction. Audit and event surfaces carry
  metadata and digests, never raw payloads or credentials.
- Explicit cancellation, which dominates completion and suppresses late output.
- Emergency pause, which blocks new leases and publication.
- Durable audit evidence committed in the same transaction as the state
  transition it describes.
- Result acceptance bound to the exact leased attempt.

Confirm no new surface grants a worker or model repository-write, credential,
network, or publication authority it did not previously hold.

## Documentation Gate

```sh
./scripts/release-docs-drift-smoke.sh
```

Update in the same change as the code:

- `README.md` — what is implemented and what is explicitly not claimed.
- `DESIGN.md` — system-level design and non-goals.
- `docs/architecture-map.md` — current implementation and evidence boundary.
- `docs/build-test-commands.md` — canonical commands and proof boundaries.
- `docs/knowledge-base/assemblywright-project-facts.md` — durable facts.
- This checklist, when the release flow itself changes.

For every feature or phase, also complete the closeout contract in
`docs/development-agent-workflow.md`: conversation-derived knowledge review,
focused unit coverage, real-boundary E2E, explicit browser/Playwright
applicability, requirements and safety review, and exact publication evidence.

## Distribution

Build and validate the unsigned layout:

```sh
./scripts/package-distribution.sh --check
```

```sh
./scripts/package-distribution.sh --unsigned-launch-check
```

Then produce the signed artifacts. This step requires Developer ID credentials
and is not reproducible in CI:

```sh
cargo run -p assemblywright-cli -- release signed-distribution-runbook
```

```sh
ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' \
ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' \
ASSEMBLYWRIGHT_NOTARYTOOL_PROFILE='...' \
./scripts/package-distribution.sh
```

## Owner-Recorded External Evidence

These lanes cannot be proven by the repository. Each writes a JSON report that
`release evidence-status` validates structurally.

**Live-device QA.** On a clean release Mac, install from the signed installer
into `/Applications`, launch through Finder, exercise the installed app, and
restart it. Then:

```sh
./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env
```

```sh
set -a && source target/release-live-device-qa.env && set +a
./scripts/release-live-device-qa.sh --assert-complete
```

The report binds the installed app executable's SHA-256, code identifier,
TeamIdentifier, and CDHash to the exact signed provenance report. Owner
evidence notes must contain real observations, not placeholders.

**Final evidence bundle.** Only after signed distribution and live-device QA
reports exist:

```sh
./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env
```

```sh
set -a && source target/release-evidence-bundle.env && set +a
./scripts/release-evidence-bundle.sh --bundle
```

```sh
./scripts/release-evidence-doctor.sh --assert-complete
```

**External handoff.** To generate the operator packet:

```sh
./scripts/release-external-handoff.sh --write target/release-external-handoff
```

## Readiness Confirmation

```sh
ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release readiness
```

`production_ready` stays false until signed distribution, notarization and
stapling, and the final evidence bundle checks all validate. Set the external
evidence mode only after owner-recorded evidence has actually been collected.

## Release Notes

State the exact surface, the exact evidence, and the exact remaining gaps. Do
not carry forward claims from a previous cycle without re-verifying them.
