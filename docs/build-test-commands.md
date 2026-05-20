# Build And Test Commands

Run commands from the repository root unless noted otherwise.

## Required Local Gate

Run the full local release gate with:

```sh
./scripts/release-local.sh
```

The script is a wrapper around the commands below and intentionally stays
local-only.

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

For durable local task and audit state during manual inspection, pass a SQLite
path:

```sh
cargo run -p jarvis-cli -- serve --db-path /tmp/jarvis.sqlite
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
cargo run -p jarvis-cli -- scheduler list
cargo run -p jarvis-cli -- scheduler schedule "manual check" "status check"
cargo run -p jarvis-cli -- pause --reason "manual smoke"
cargo run -p jarvis-cli -- pause-status
cargo run -p jarvis-cli -- resume
```

Current boundary: the command endpoint runs `ConversationRuntime` with
`FakeLocalModel` by default or an opt-in Ollama-compatible local HTTP provider
from typed env config. It records local-first `ModelRouter` audit evidence, can
execute deterministic first-party plugin commands such as `plugin echo ...`
through the policy engine, honors `--dry-run` for plugin execution, and can
persist task/audit state when configured with a repository-backed IPC state. It
also has deterministic coverage for bounded model-planned first-party tool
calls. It does not yet implement ChatGPT execution, installed plugin
sandboxing, or approval UI.

When the server is started with `--db-path`, these inspection commands are also
available:

```sh
cargo run -p jarvis-cli -- tasks list
cargo run -p jarvis-cli -- tasks audit
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
```

## Release Evidence Boundary

Passing `./scripts/release-local.sh` proves the current Rust workspace builds,
passes standard and ignored release-proof tests, runs the CLI smoke command,
packages the Rust crates, and passes the Swift package build/test gate. The
cross-process IPC E2E test proves the local server and CLI can exchange JSON for
health, runtime-backed command execution, deterministic first-party plugin
execution, route/policy/plugin audit evidence, scheduler schedule/cancel and
persistence, redacted diagnostics export, memory create/update/review/delete
and persistence, plugin manifests, and emergency-pause blocking/resume surfaces.
Runtime unit tests additionally prove bounded fake-model first-party tool-call
orchestration, including policy checks, approval stops, validation failures, and
tool-result feedback into later model steps. Focused provider tests prove typed
Ollama-compatible request/error behavior without requiring a live model during
the default release gate. They do not prove signed app packaging, ChatGPT
execution, installed plugin sandboxing, memory UX beyond the scaffold, approval
UI, voice loop, or packaged Mac release smoke test until those surfaces exist
and are covered. The current Swift gate proves the Mac shell scaffold builds,
decodes IPC contracts, exposes management models, and can supervise a
configured local core process abstraction; it does not prove a signed packaged
app bundles and launches the Rust core.
