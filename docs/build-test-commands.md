# Build And Test Commands

Run commands from the repository root unless noted otherwise.

## Required Local Gate

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace -- --ignored
cargo build --workspace
cargo run -p jarvis-cli -- smoke
cargo package --workspace
swift test --package-path apps/mac
swift build --package-path apps/mac
```

## Current Health Check

`jarvis smoke` starts an ephemeral loopback server and verifies health, command,
pause blocking, and resume:

```sh
cargo run -p jarvis-cli -- smoke
```

For manual inspection, `jarvis-cli health` calls a loopback HTTP server, so
start the server first.

Terminal 1:

```sh
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
cargo run -p jarvis-cli -- scheduler list
cargo run -p jarvis-cli -- scheduler schedule "manual check" "status check"
cargo run -p jarvis-cli -- pause --reason "manual smoke"
cargo run -p jarvis-cli -- pause-status
cargo run -p jarvis-cli -- resume
```

Current boundary: the command endpoint runs `ConversationRuntime` with
`FakeLocalModel`, records local-first `ModelRouter` audit evidence, can execute
deterministic first-party plugin commands such as `plugin echo ...` through the
policy engine, honors `--dry-run` for plugin execution, and can persist
task/audit state when configured with a repository-backed IPC state. It does
not yet implement autonomous model-generated tool calls, real model providers,
or approval UI.

## Useful Focused Commands

```sh
cargo test -p jarvis-core
cargo test -p jarvis-core --test e2e_scaffold
cargo test -p jarvis-cli
```

## Release Evidence Boundary

Passing these commands proves the current Rust workspace builds and its tests
pass. The smoke commands prove the local server and CLI can exchange JSON for
health, runtime-backed command execution, route/plugin audit evidence,
scheduler, and emergency-pause surfaces. They do not prove app packaging, real
local model provider integration, autonomous model-generated tool calls, memory
UX, approval UI, voice loop, or packaged Mac release smoke test until those
surfaces exist and are covered. The current Swift gate proves the Mac shell
scaffold builds and its IPC contract decoding tests pass.
