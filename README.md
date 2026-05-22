# Jarvis

Jarvis is a local-first macOS assistant foundation. The current repo implements
the Rust core described in [DESIGN.md](DESIGN.md): durable task/audit
primitives, policy-gated first-party plugin commands, bounded model-planned
first-party tool orchestration, strict provider response envelopes for
first-party tool requests, local-first model routing evidence, opt-in
Ollama-compatible local HTTP and ChatGPT/OpenAI-compatible provider boundaries,
structured provider-failure responses with route/audit evidence, plugin
contracts, metadata-only local plugin installation, local plugin
provenance snapshots, scheduler state, redacted diagnostics export, a loopback
IPC surface with compatibility policy plus feature proof/boundary metadata,
repository-backed activity summary and activity event stream, conservative
release-readiness inspection, and CLI smoke paths for the Swift shell scaffold
and future packaged app.
It also includes the first buildable Swift/SwiftUI Mac shell scaffold under
`apps/mac`, with a tested IPC client, command-console state model,
activity/audit panel with current progress summary, memory
create/update/review/delete and restore management, memory classification
summary, memory review counts in diagnostics and permission policy review, provenance-aware
permission/grant inspection, permission policy review items, redacted scheduler
attention summaries for app handoff, scheduler trigger policy-review items,
release-readiness blocker inspection,
adapter-backed scheduler notification controls, degraded-mode handling, and a
core supervisor abstraction.
Scheduler due execution records a redacted proactive policy audit before
submitting commands, using the same trigger classification as
`/permissions/policy-review`, so proactive routines stay inspectable without
exposing scheduler command text.
Operators can also run `jarvis scheduler recover-stale` to mark persisted
stale `Running` scheduler jobs failed with redacted audit evidence after a
crash or killed process leaves a job leased but unfinished.
`jarvis serve --scheduler-recover-stale-on-startup` offers the same recovery as
an explicit startup option and marks the audit payload with
`automatic_recovery: true` while keeping scheduler command text redacted.

## Current Scope

This repository is intentionally v1 foundation work, not a Marvel/JARVIS clone
and not an autonomous external-communication system. Risky side effects must be
blocked or require approval, and every meaningful decision should be auditable.
The current implementation should not be described as a finished production
assistant: distribution signing/notarization, live microphone validation,
marketplace plugin trust, OS-level plugin network sandboxing, live OS-level
scheduler notification validation, and manual release QA are still target
architecture. The
default command path still uses `FakeLocalModel`; set
`JARVIS_LOCAL_MODEL_PROVIDER=ollama`, `JARVIS_LOCAL_MODEL`, and optionally
`JARVIS_OLLAMA_BASE_URL`/`JARVIS_LOCAL_MODEL_TIMEOUT_MS` to exercise the local
HTTP provider. ChatGPT execution is disabled by default and requires explicit
typed env opt-in with `JARVIS_CHATGPT_ENABLED=true`,
`JARVIS_OPENAI_API_KEY`, and optional `JARVIS_CHATGPT_MODEL`,
`JARVIS_OPENAI_BASE_URL`, and `JARVIS_CHATGPT_TIMEOUT_MS`; route policy still
blocks restricted data and sends only redacted route context. Provider failures
return failed command responses with redacted diagnostics instead of becoming
IPC transport errors. Provider text responses may also use a strict JSON
envelope with `message`, `complete`, and `tool_requests`; accepted tool
requests still pass through the existing first-party schema, policy, approval,
and audit path. ChatGPT/OpenAI-compatible responses may also return native
OpenAI `tool_calls` for the advertised first-party tool definitions; those are
translated into the same bounded first-party path. Plain text remains
supported, and this is not installed-plugin orchestration or broad third-party
tool execution. Local plugin
installation stores validated manifest metadata disabled by default and captures
a local manifest/subprocess hash snapshot. Executable local subprocess plugins
require the snapshot to verify as unchanged plus an explicit
`subprocess_stdio` grant, or `subprocess_stdio_network` when an action declares
network access, and still run only through the constrained JSON stdin/stdout
boundary. They may emit bounded `jarvis_progress` JSON frames on stderr; Jarvis
exposes only parsed sequence/stage/message progress events in the run response
and audit log, while raw stderr stays redacted. Publisher-origin claims can be
operator-pinned only
after local provenance matches the install snapshot; this is an auditable
local review step. Manifests can also carry an Ed25519 publisher signature,
which Jarvis verifies only against an explicit trusted public key after local
provenance matches; this is not marketplace trust or malware analysis.
Network-capable plugin actions must declare exact allowed hosts in the manifest
and appear in policy review; Jarvis also fails closed unless executable
network-capable installed plugins are enabled with `subprocess_stdio_network`.
This is runtime grant gating and manifest governance, not an OS-level network
sandbox or host-level egress filter.

## Production Work Protocol

This public repository is being advanced through isolated worktrees, topic
branches, and reviewable PR slices. During the autonomous production sweep,
agents should stay inside their assigned ownership, preserve unrelated edits,
and treat cross-process E2E plus the local release gate as the evidence bar.
Passing the local gate supports only the implemented Rust/Swift foundation
claim; it is not proof of a finished packaged assistant.
`/release/readiness`, `jarvis release readiness`, and the Swift Release tab
summarize implemented feature proofs, pending feature boundaries, recommended
verification commands, and manual production blockers in read-only surfaces.
The CLI command prefers a running IPC server but falls back to the same
conservative local readiness summary when the server is unavailable, so release
triage still works before starting the supervised core.
Readiness feature metadata includes the repository-backed operator QA smoke as
implemented local evidence, with clean-profile installed-app and live-device
QA still listed as manual gates.
The response keeps `production_ready: false` until Developer ID
signing/notarization, clean-profile installer/Finder validation, live voice
device checks, and manual QA are actually completed.

Phase 3 landed through separate worktrees for model route persistence, plugin
subprocess sandboxing, voice input controls, packaged app release smoke,
permission grants UX, docs architecture alignment, distribution packaging, and
Keychain launch credential injection. Follow-on slices have added Swift memory
CRUD, local plugin provenance verification, scheduler attention handoff, and
permission policy review plus scheduler trigger review and notification
controls, including redacted proactive scheduler policy audit before due
command execution, explicit stale-running scheduler recovery, opt-in startup
stale-running recovery, and strict provider tool-request envelope coverage for
Ollama-compatible and ChatGPT/OpenAI-compatible text responses. Later slices
continue the same branch/PR discipline;
release language should describe only the merged repo-owned surfaces with
recorded focused E2E or integration proof.
The permission center now surfaces installed-plugin provenance status from
`/permissions/grants`, including unverified plugin counts and local integrity
state. `/permissions/policy-review` turns pending approvals and installed
plugin provenance/grant/network concerns plus unreviewed memory items and
deleted sensitive memory retained in local storage into explicit review items
without exposing memory values, but it still does not grant broader marketplace
trust, malware safety, autonomous memory rewriting, purge automation, or
OS-level network sandboxing.
Approval grant/deny decisions remain side-effect-free. Approved first-party
approval records require a separate one-shot `/approvals/:id/execute` or
`jarvis approvals execute <approval-id>` replay, which verifies the original
action and scopes before recording `approval_executed` audit evidence. The
Swift Approval Center exposes the same boundary by showing Run Approved only
for approved records that do not already have execution audit evidence.

## Build

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
./scripts/storage-migration-backup-smoke.sh
./scripts/release-operator-qa-smoke.sh
swift test --package-path apps/mac
swift build --package-path apps/mac
```

For the current IPC smoke path, start the local server and run CLI commands
from a second terminal:

```sh
cargo run -p jarvis-cli -- serve
cargo run -p jarvis-cli -- health
```

Use `cargo run -p jarvis-cli -- serve --db-path /tmp/jarvis.sqlite` when you
want manual IPC commands to persist task and audit state locally.

For branches that touch SQLite migrations or file-backed repository startup,
run the focused storage recovery proof:

```sh
./scripts/storage-migration-backup-smoke.sh
```

That script proves legacy file-backed DB migration creates a preflight backup,
failed migration-open restores the backup, and newer schema versions fail with
an explicit upgrade diagnostic. It does not replace installer upgrade QA.

For operator-facing release QA over a repository-backed local core, run:

```sh
./scripts/release-operator-qa-smoke.sh
```

That script starts a loopback core with an isolated SQLite database, exercises
command, audit, routes, memory create/update/review/delete/restore, scheduler
attention/run-due, activity, permission review, diagnostics, emergency pause,
release readiness, and restart recovery. It is local CLI QA evidence, not a
clean-profile installed-app or live-device validation pass.

For branches that touch Swift supervision or core binary discovery, run the
focused packaged-supervision proof:

```sh
./scripts/packaged-supervision-proof.sh
```

That script builds `jarvis-cli`, places it in a temporary
`Jarvis.app/Contents/Resources/bin/` layout, runs the Swift coverage against
the configured packaged-style executable, and runs `jarvis smoke`.
It is branch evidence for app-supervised core discovery, not a signed packaged
app release smoke.

For local packaged app smoke evidence, run:

```sh
./scripts/packaged-app-release-smoke.sh
```

That script assembles a temporary `Jarvis.app`, verifies microphone/Speech usage
strings, ad-hoc signs with the packaged audio-input entitlement when `codesign`
is available, and launches the app against an isolated temp profile. It is
still not Developer ID signing, notarization, installer validation, or live
voice-device validation.

For distribution packaging work, run:

```sh
./scripts/package-distribution.sh --check
./scripts/package-distribution.sh --unsigned-structure-check
./scripts/package-distribution.sh --unsigned-launch-check
```

The unsigned structure check builds release Rust and Swift artifacts, assembles
the distribution-shaped `Jarvis.app`, optionally ad-hoc signs it when
`codesign` is available, creates an unsigned `/Applications` installer package,
and inspects the package payload for the app executable, bundled core, and
`Info.plist`.
The unsigned launch check additionally launches the release-built app
executable with an isolated temporary HOME and verifies app-supervised core
health, command, audit, diagnostics, pause/block/resume behavior, and SQLite
state through the bundled core. It is also part of the default
`./scripts/release-local.sh` local release gate so distribution-layout launch
regressions fail the standard proof path.
The full `package-distribution.sh` lane owns Developer ID signing,
notarization, stapling, microphone entitlement packaging, and signed installer
package creation when Apple credentials are provided. It still does not replace
clean-profile installer run, Finder launch, App Store validation, or live
microphone/Speech/audio-output validation.

With a repository-backed server running, `jarvis release readiness`,
`jarvis tasks`, `jarvis memory`, `jarvis activity summary`,
`jarvis activity watch`, `jarvis scheduler`, `jarvis diagnostics`, and
`jarvis plugins` expose the current readiness evidence, durable state, status
counts, recent task/audit progress, bounded activity events, redacted scheduler
attention handoff, scheduler trigger policy review, redacted diagnostics,
first-party plugin manifests, and disabled installed-plugin registry metadata
over IPC.

## Docs

- [Architecture map](docs/architecture-map.md)
- [Plugin contract](docs/plugin-contract.md)
- [Safety rules](docs/safety-rules.md)
- [Build and test commands](docs/build-test-commands.md)
- [Release checklist](docs/release-checklist.md)
- [Knowledge-base facts](docs/knowledge-base/jarvis-project-facts.md)

The architecture map includes both the implemented current-state diagram and
the end-goal production diagram, plus a phase table that separates verified
foundation work from future production assistant requirements.
