# Build And Test Commands

Run commands from the repository root unless noted otherwise.

## Required Local Gate

Run the full local release gate with:

```sh
./scripts/release-local.sh
```

The script is a wrapper around the commands below and intentionally stays
local-only. Use this gate as the default PR evidence for current foundation
work unless a narrower docs-only change justifies a focused documentation
check.

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace -- --ignored
./scripts/storage-migration-backup-smoke.sh
cargo build --workspace
cargo run -p jarvis-cli -- smoke
cargo package --workspace --allow-dirty
swift test --package-path apps/mac
swift build --package-path apps/mac
```

## Current Health Check

`jarvis smoke` starts an ephemeral loopback server and verifies the currently
implemented foundation surfaces: health, command execution, pause blocking,
resume, plugin manifest listing, and repository-backed task plus memory
inspection paths:

```sh
cargo run -p jarvis-cli -- smoke
```

For manual inspection, `jarvis-cli health` calls a loopback HTTP server, so
start the server first.

Terminal 1:

```sh
cargo run -p jarvis-cli -- serve
```

The default command runtime uses `FakeLocalModel` so local smoke tests stay
deterministic. To exercise the production-shaped local HTTP provider boundary
against an Ollama-compatible server:

```sh
JARVIS_LOCAL_MODEL_PROVIDER=ollama \
JARVIS_LOCAL_MODEL=llama3.2 \
JARVIS_OLLAMA_BASE_URL=http://127.0.0.1:11434 \
JARVIS_LOCAL_MODEL_TIMEOUT_MS=15000 \
cargo run -p jarvis-cli -- serve
```

To exercise the opt-in ChatGPT/OpenAI-compatible provider boundary, disable
the local provider and provide an API key. The key is never serialized in
provider config, provider status, route evidence, diagnostics, or structured
provider errors:

```sh
JARVIS_LOCAL_MODEL_ENABLED=false \
JARVIS_CHATGPT_ENABLED=true \
JARVIS_OPENAI_API_KEY=... \
JARVIS_CHATGPT_MODEL=gpt-4.1-mini \
cargo run -p jarvis-cli -- serve
```

For durable local task and audit state during manual inspection, pass a SQLite
path:

```sh
cargo run -p jarvis-cli -- serve --db-path /tmp/jarvis.sqlite
```

To exercise the bounded background scheduler loop, opt in explicitly on the
server. Each tick calls the same audited run-due path and clamps the per-tick
limit to the core scheduler maximum:

```sh
cargo run -p jarvis-cli -- serve \
  --db-path /tmp/jarvis.sqlite \
  --scheduler-background \
  --scheduler-interval-ms 30000 \
  --scheduler-limit 16
```

Terminal 2:

```sh
cargo run -p jarvis-cli -- health
```

Expected response body includes:

```text
"status":"ok"
```

Stop the server with `Ctrl-C` after the smoke checks.

## IPC Smoke Commands

Run these while `cargo run -p jarvis-cli -- serve` is active:

```sh
cargo run -p jarvis-cli -- command --dry-run "status check"
cargo run -p jarvis-cli -- plugins list
cargo run -p jarvis-cli -- diagnostics export
cargo run -p jarvis-cli -- permissions grants
cargo run -p jarvis-cli -- scheduler list
cargo run -p jarvis-cli -- scheduler schedule "manual check" "status check"
cargo run -p jarvis-cli -- scheduler schedule "approval fail closed" "plugin approval echo scheduler pause"
cargo run -p jarvis-cli -- scheduler run-due --limit 1
cargo run -p jarvis-cli -- plugins installed
cargo run -p jarvis-cli -- pause --reason "manual smoke"
cargo run -p jarvis-cli -- pause-status
cargo run -p jarvis-cli -- resume
```

Current boundary: the command endpoint runs `ConversationRuntime` with
`FakeLocalModel` by default, an opt-in Ollama-compatible local HTTP provider, or
an opt-in ChatGPT/OpenAI-compatible HTTP provider from typed env config. It
records local-first `ModelRouter` audit evidence, sends ChatGPT only minimized
redacted route context after policy selection, can execute deterministic
first-party plugin commands such as `plugin echo ...` through the policy
engine, honors `--dry-run` for plugin execution, and can persist task/audit
state plus redacted model-route records when configured with a
repository-backed IPC state. It also has deterministic coverage for bounded
model-planned first-party tool calls.
Approval-required command scaffolds such as `plugin approval echo ...` fail
closed by returning `waiting_for_approval`, persisting an inspectable pending
approval when repository backing is enabled, and requiring a separate CLI/IPC
grant or denial. Granting an approval records the decision but does not execute
the side effect. `jarvis permissions grants` reads the combined local grant
surface: approval counts/history, high-risk pending count, installed-plugin
`metadata_only` grant records, and the invariant that side effects still
require approval. Installed `local_subprocess` plugins remain disabled by
default and execute only after an explicit `subprocess_stdio` grant through the
constrained JSON stdin/stdout runner. The Swift app now exposes the
Speech/AVFoundation input adapter controls and AVFoundation speech-output
preview controls, but release claims for real voice still require entitlement
packaging, live microphone checks, live audio-output checks, and manual device
validation. Swift approval and voice controls are covered by the Swift
contract/model tests.
Local plugin install is metadata-only:
`jarvis plugins install /absolute/path/to/jarvis-plugin.json` validates and
stores a disabled registry record with local provenance hashes when repository
backing is enabled. Use `jarvis plugins verify-installed <id>` before enabling
local subprocess execution; enablement fails closed unless the manifest and
subprocess command still match the install snapshot.
Background scheduler execution is opt-in on `jarvis serve`; it does not start
for default smoke or manual inspection sessions unless `--scheduler-background`
is passed.

When the server is started with `--db-path`, these inspection commands are also
available:

```sh
cargo run -p jarvis-cli -- tasks list
cargo run -p jarvis-cli -- tasks audit
cargo run -p jarvis-cli -- routes list
cargo run -p jarvis-cli -- routes list --task-id <task-id>
cargo run -p jarvis-cli -- routes get <route-id>
cargo run -p jarvis-cli -- approvals list --status pending
cargo run -p jarvis-cli -- approvals approve <approval-id> --decided-by cli --reason "reviewed"
cargo run -p jarvis-cli -- approvals deny <approval-id> --decided-by cli --reason "not safe"
cargo run -p jarvis-cli -- memory list
cargo run -p jarvis-cli -- memory classification --include-deleted
cargo run -p jarvis-cli -- memory create workflow release-gate "run local gate before PR" --provenance "manual note" --sensitivity workspace
cargo run -p jarvis-cli -- memory restore <memory-id>
cargo run -p jarvis-cli -- diagnostics export
```

## Useful Focused Commands

```sh
cargo test -p jarvis-core
cargo test -p jarvis-core --test e2e_scaffold
cargo test -p jarvis-cli
cargo test -p jarvis-cli --test local_ipc_e2e
cargo test -p jarvis-cli --test local_ipc_e2e -- --ignored
./scripts/storage-migration-backup-smoke.sh
./scripts/packaged-supervision-proof.sh
./scripts/packaged-app-release-smoke.sh
./scripts/package-distribution.sh --check
```

The non-ignored `local_ipc_e2e` test is the current cross-process E2E
expectation for Rust/CLI changes. The ignored variant includes the opt-in
release-proof smoke command and is run by `./scripts/release-local.sh`.
`./scripts/storage-migration-backup-smoke.sh` is the focused storage recovery
proof for migration changes: it runs Rust tests that create a legacy
file-backed DB, verify preflight backup creation, corrupt the DB after backup
to prove restore on migration-open failure, and verify newer schema versions
fail with an explicit upgrade diagnostic.
`./scripts/packaged-supervision-proof.sh` is the focused Swift/Rust bridge
proof for supervision changes: it builds `jarvis-cli`, copies it into a
temporary `Jarvis.app/Contents/Resources/bin/` layout, runs the Swift supervisor
coverage against that configured executable, starts the copied binary with a
repository-backed database, verifies packaged-layout health, command, audit,
diagnostics, emergency pause, blocked command, pause status, and resume
surfaces, and then runs the CLI smoke command.
`./scripts/packaged-app-release-smoke.sh` is the stronger local packaged app
proof for packaging/release changes: it builds the Swift app executable,
assembles a deterministic `Jarvis.app`, writes `Info.plist`, bundles
`jarvis-cli` in `Contents/Resources/bin/`, ad-hoc signs with `codesign -` when
available, launches the app executable under a temporary HOME/profile with an
isolated endpoint and database path, and verifies app-supervised core health,
command, audit, diagnostics, emergency pause, blocked command, pause status,
resume, and temp-profile SQLite state. It is still local evidence only, not
Developer ID signing, notarization, installer validation, entitlement
validation, App Store distribution, Finder/LaunchServices validation, or real
microphone/Speech/live audio-output coverage.
`./scripts/package-distribution.sh` is the stricter distribution packaging
lane. Its `--check` mode validates local tool availability and the entitlements
template without Apple credentials. Full mode requires
`JARVIS_DEVELOPER_ID_APPLICATION` plus notarytool credentials, signs the release
bundle with hardened runtime and microphone entitlements, submits it for
notarization, and staples the ticket. Passing that script still does not
replace clean-profile Finder launch, live microphone/Speech validation,
installer validation, App Store review, or live audio-output validation.
Docs-only branches should at least run a render/lint-oriented documentation
check when available, plus `cargo fmt --check` if the branch also touches Rust
examples or scripts. Record any skipped full-gate stage as a blocker, not as
implicit coverage.

For documentation-only production-sweep slices, use focused repository checks
that prove the required docs and diagrams are still present:

```sh
rg -n "Current Implementation Diagram|End-Goal Production Architecture" docs/architecture-map.md
rg -n "phase-3|phase3|six-agent|worktree|E2E|production-readiness|proof" docs/knowledge-base/jarvis-project-facts.md docs/release-checklist.md docs/build-test-commands.md README.md
git diff --check
```

These checks do not replace `./scripts/release-local.sh` for executable
changes. They only support docs-only PR evidence and should be reported as such.
For the phase-3 docs architecture slice, the expected verification is the two
`rg` checks above plus `git diff --check`; code gates belong to the executable
phase-3 slices unless this branch starts changing code.

## Release Evidence Boundary

Passing `./scripts/release-local.sh` proves the current Rust workspace builds,
passes standard and ignored release-proof tests, runs the CLI smoke command,
packages the Rust crates, and passes the Swift package build/test gate. The
cross-process IPC E2E test proves the local server and CLI can exchange JSON for
health, runtime-backed command execution, deterministic first-party plugin
execution, route/policy/plugin audit evidence, approval-required persistence and
grant/deny decisions, scheduler schedule/cancel and persistence, redacted
diagnostics export, memory classification summary fields,
memory create/update/review/delete/restore and persistence, plugin
manifests, installed-plugin provenance verification, permission-grant
provenance summary fields, fail-closed subprocess enablement, scheduler due-job
execution/reschedule audit evidence, scheduler fail-closed emergency pause on
non-accepted due jobs, and emergency-pause blocking/resume surfaces.
Runtime unit tests additionally prove bounded fake-model first-party tool-call
orchestration, including policy checks, approval stops, validation failures, and
tool-result feedback into later model steps. Focused provider tests prove typed
Ollama-compatible request/error behavior without requiring a live model during
the default release gate. They do not prove live ChatGPT service execution,
advanced memory classification policy beyond the current summary surface, live
microphone capture, or live audio output until those surfaces are manually
validated. The current Swift gate proves the
Mac shell scaffold builds, decodes IPC contracts, exposes management models for
approval evidence, memory classification summary,
memory create/update/review/delete/restore state, runs/audit,
scheduler, diagnostics, text-transcript voice handoff state, adapter-backed
voice input controls, adapter-backed speech-output preview controls, and
Keychain-backed supervised-core credential injection
without requiring live microphone access, live audio output, or real credentials
in tests. It can supervise a configured local core process
abstraction. It also covers Swift approval decision calls against the Rust IPC
approval endpoints. The packaged supervision proof additionally checks the
expected `Resources/bin/jarvis-cli` bundle layout with a locally built core
binary and exercises repository-backed command, audit, diagnostics, and
emergency-pause IPC through that copied binary. The packaged app release smoke
assembles and ad-hoc signs a local `Jarvis.app`, launches it with isolated
profile state, and verifies app-supervised core IPC through the bundled
`jarvis-cli`. These gates still do not prove Developer ID signing,
notarization, installer behavior, entitlement validation, Finder/LaunchServices
launch, microphone permissions, live speech-to-text, or live audio-output
behavior unless the stricter distribution lane and manual checks named in the
release checklist are also completed.

The public-repo production workflow expects isolated worktrees, topic branches,
reviewable PRs, and clear ownership. A six-agent autonomous sweep can reduce
elapsed time, but readiness claims still depend on checked-in implementation
and the verification commands above. Each feature phase should name the E2E or
focused integration coverage it relies on; when a phase changes behavior and no
such coverage exists, adding coverage is part of the phase rather than a
follow-up readiness claim.
