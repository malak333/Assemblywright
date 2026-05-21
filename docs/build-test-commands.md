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
state when configured with a repository-backed IPC state. It also has
deterministic coverage for bounded model-planned first-party tool calls.
Approval-required command scaffolds such as `plugin approval echo ...` fail
closed by returning `waiting_for_approval`, persisting an inspectable pending
approval when repository backing is enabled, and requiring a separate CLI/IPC
grant or denial. Granting an approval records the decision but does not execute
the side effect. `jarvis permissions grants` reads the combined local grant
surface: approval counts/history, high-risk pending count, installed-plugin
`metadata_only` grant records, and the invariant that side effects still
require approval. It does not yet implement installed plugin sandboxing,
installed plugin execution, real voice capture, or signed packaged-app smoke.
Swift approval decision controls are covered by the Swift contract/model tests.
Local plugin install is metadata-only:
`jarvis plugins install /absolute/path/to/jarvis-plugin.json` validates and
stores a disabled registry record when repository backing is enabled.
Background scheduler execution is opt-in on `jarvis serve`; it does not start
for default smoke or manual inspection sessions unless `--scheduler-background`
is passed.

When the server is started with `--db-path`, these inspection commands are also
available:

```sh
cargo run -p jarvis-cli -- tasks list
cargo run -p jarvis-cli -- tasks audit
cargo run -p jarvis-cli -- approvals list --status pending
cargo run -p jarvis-cli -- approvals approve <approval-id> --decided-by cli --reason "reviewed"
cargo run -p jarvis-cli -- approvals deny <approval-id> --decided-by cli --reason "not safe"
cargo run -p jarvis-cli -- memory list
cargo run -p jarvis-cli -- memory create workflow release-gate "run local gate before PR" --provenance "manual note" --sensitivity workspace
cargo run -p jarvis-cli -- diagnostics export
```

## Useful Focused Commands

```sh
cargo test -p jarvis-core
cargo test -p jarvis-core --test e2e_scaffold
cargo test -p jarvis-cli
cargo test -p jarvis-cli --test local_ipc_e2e
cargo test -p jarvis-cli --test local_ipc_e2e -- --ignored
./scripts/packaged-supervision-proof.sh
```

The non-ignored `local_ipc_e2e` test is the current cross-process E2E
expectation for Rust/CLI changes. The ignored variant includes the opt-in
release-proof smoke command and is run by `./scripts/release-local.sh`.
`./scripts/packaged-supervision-proof.sh` is the focused Swift/Rust bridge
proof for supervision changes: it builds `jarvis-cli`, copies it into a
temporary `Jarvis.app/Contents/Resources/bin/` layout, runs the Swift supervisor
coverage against that configured executable, starts the copied binary with a
repository-backed database, verifies packaged-layout health, command, audit,
diagnostics, emergency pause, blocked command, pause status, and resume
surfaces, and then runs the CLI smoke command.
Docs-only branches should at least run a render/lint-oriented documentation
check when available, plus `cargo fmt --check` if the branch also touches Rust
examples or scripts. Record any skipped full-gate stage as a blocker, not as
implicit coverage.

For documentation-only production-sweep slices, use focused repository checks
that prove the required docs and diagrams are still present:

```sh
rg -n "Current Implementation Diagram|End-Goal Production Architecture" docs/architecture-map.md
rg -n "six-agent|worktree|E2E|production-readiness|proof" docs/knowledge-base/jarvis-project-facts.md docs/release-checklist.md docs/build-test-commands.md
git diff --check
```

These checks do not replace `./scripts/release-local.sh` for executable
changes. They only support docs-only PR evidence and should be reported as such.

## Release Evidence Boundary

Passing `./scripts/release-local.sh` proves the current Rust workspace builds,
passes standard and ignored release-proof tests, runs the CLI smoke command,
packages the Rust crates, and passes the Swift package build/test gate. The
cross-process IPC E2E test proves the local server and CLI can exchange JSON for
health, runtime-backed command execution, deterministic first-party plugin
execution, route/policy/plugin audit evidence, approval-required persistence and
grant/deny decisions, scheduler schedule/cancel and persistence, redacted
diagnostics export, memory create/update/review/delete and persistence, plugin
manifests, scheduler due-job execution/reschedule audit evidence, scheduler
fail-closed emergency pause on non-accepted due jobs, and emergency-pause
blocking/resume surfaces.
Runtime unit tests additionally prove bounded fake-model first-party tool-call
orchestration, including policy checks, approval stops, validation failures, and
tool-result feedback into later model steps. Focused provider tests prove typed
Ollama-compatible request/error behavior without requiring a live model during
the default release gate. They do not prove signed app packaging, ChatGPT
execution, installed plugin sandboxing/execution, memory UX beyond the
scaffold, Swift approval decision UI, voice loop, or packaged Mac release smoke
test until those surfaces exist and are covered. The current Swift gate proves
the Mac shell scaffold builds, decodes IPC contracts, exposes management models
for approval evidence, runs/audit, scheduler, diagnostics, and text-only voice
transcript handoff state, and can supervise a configured local core process
abstraction. It also covers Swift approval decision calls against the Rust IPC
approval endpoints. The packaged supervision proof additionally checks the
expected `Resources/bin/jarvis-cli` bundle layout with a locally built core
binary and exercises repository-backed command, audit, diagnostics, and
emergency-pause IPC through that copied binary. These gates still do not prove
a signed packaged app release, notarization, clean-profile launch, microphone
permissions, live speech-to-text, or text-to-speech.

The public-repo production workflow expects isolated worktrees, topic branches,
reviewable PRs, and clear ownership. A six-agent autonomous sweep can reduce
elapsed time, but readiness claims still depend on checked-in implementation
and the verification commands above. Each feature phase should name the E2E or
focused integration coverage it relies on; when a phase changes behavior and no
such coverage exists, adding coverage is part of the phase rather than a
follow-up readiness claim.
