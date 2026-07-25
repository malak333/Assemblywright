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
  accurately. The repository kernel is default-inert; it exposes no HTTP/API,
  worker dispatcher, repository execution, review provider, publication
  coordinator, Mac queue UI, or autonomous activation.
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
cargo run -p jarvis-cli -- release signed-distribution-runbook
```

```sh
JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' \
JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' \
JARVIS_NOTARYTOOL_PROFILE='...' \
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
JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness
```

`production_ready` stays false until signed distribution, notarization and
stapling, and the final evidence bundle checks all validate. Set the external
evidence mode only after owner-recorded evidence has actually been collected.

## Release Notes

State the exact surface, the exact evidence, and the exact remaining gaps. Do
not carry forward claims from a previous cycle without re-verifying them.
