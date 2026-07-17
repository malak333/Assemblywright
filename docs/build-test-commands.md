# Build And Test Commands

Run commands from the repository root unless noted otherwise.

## Required Local Gate

Run the full local release gate with:

```sh
./scripts/release-local.sh
```

The script is a wrapper around the ordered command set in this section and
intentionally stays local-only. Use this gate as the default PR evidence for
current foundation work unless a narrower docs-only change justifies a focused
documentation check.
On GitHub, `.github/workflows/release-local.yml` runs the same gate on
`macos-15` with SHA-pinned checkout/toolchain actions and Rust `1.95.0` for
pull requests, pushes to `main`, and manual dispatch. CI sets
`JARVIS_RELEASE_LOCAL_HEARTBEAT_SECONDS=60` so long-running release-local
commands periodically print elapsed-time heartbeat lines without changing the
canonical command list or proof boundary. The workflow is configuration
evidence only; it still does not perform Developer ID signing, notarization,
clean-profile installation, Finder launch validation, live-device QA, or plugin
marketplace trust review.
`/contract` and release readiness expose this lane as `release_ci_gate` with
the same boundary.

The portable distributed-development foundation has a separate Windows gate:

For a fresh Windows checkout, install the MSVC Rust toolchain pinned by
`rust-toolchain.toml` after the Visual Studio C++ Build Tools and Windows SDK
are present:

```powershell
rustup toolchain install 1.95.0 --profile minimal --component clippy --component rustfmt
```

Restart PowerShell and Codex after installation so they inherit
`%USERPROFILE%\.cargo\bin` on `PATH`. If a long-running Codex process still has
the pre-install environment, use
`C:\Users\mike\.cargo\bin\cargo.exe` for the current validation run, then
restart Codex before subsequent development.

```sh
cargo fmt --all --check
cargo clippy -p jarvis-protocol --all-targets --locked -- -D warnings
cargo clippy -p jarvis-master --all-targets --locked -- -D warnings
cargo test -p jarvis-protocol --locked
cargo test -p jarvis-protocol --test distributed_protocol_contract_e2e --locked
cargo test -p jarvis-master --locked
cargo test -p jarvis-master --test master_lifecycle_e2e --locked
cargo test -p jarvis-master --test master_process_e2e --locked
```

`.github/workflows/windows-protocol.yml` runs formatting plus the protocol and
master-process lint/test commands on `windows-latest`. The existing
`jarvis-core` runtime is Unix/macOS-only and does not compile on Windows because
its current release path requires Unix-domain sockets and Unix filesystem APIs.
On a supported macOS host, the additional feature-gated integration check is:

```sh
cargo check -p jarvis-core --features distributed-development --locked
```

That check proves only that `jarvis-core` can consume the dormant contracts.
The Windows job deliberately does not build the current Unix/macOS runtime or a
Windows service installation, establish mTLS, operate an enrollment CA, run a
live model, mutate a repository, or exercise a Codex account. Those
capabilities remain gated target architecture in
`docs/distributed-developer-mode-design.md`.
Wire consumers must use each top-level message's `decode_frame` entry point so
the raw byte ceiling is checked before Serde decoding and semantic validation.
Protocol validation also rejects nil UUID identities before handshake or job
acceptance.
`distributed_protocol_contract_e2e` exercises the complete implemented seam:
a Mac worker advertises an MLX capability, a Windows master accepts it, issues
a digest-bound leased job, accepts the exact result, and rejects the same
result under a different lease. This is serialized contract E2E coverage, not
a network, process, mTLS, live-model, or cross-device runtime test.
`master_lifecycle_e2e` exercises the second implemented seam against a
file-backed SQLite database and in-process fake worker. It covers explicit
device registration, exact handshake acceptance, monotonic connection epochs
and sequence high-water, durable enqueue/lease/result, global and
capability-specific bounds, duplicate/wrong-lease/cancelled/expired/late result
denial, disconnect/restart abandonment, and reissue only after the earlier
attempt is durably non-accepting. It does not prove process exclusivity,
network transport, device authentication, worker cleanup, or unified migration
of the existing Mac-owned task, policy, audit, scheduler, and memory state.
`master_process_e2e` starts the actual `jarvis-master` binary and a separate
fixture-worker process. It proves setup receipts, exclusive database ownership,
bearer non-disclosure, unauthorized and oversized-body denial, authenticated
loopback health, one bounded enqueue/lease/result story, durable health counters,
and restart reconciliation. Its generated bearer is a local development
bootstrap secret, not mTLS, device enrollment, remote-worker trust, or a
production service credential.

Use these PowerShell commands for a manual Windows process smoke:

```powershell
$jarvisData = Join-Path $env:LOCALAPPDATA 'Jarvis\master'
cargo run -p jarvis-master -- --data-dir $jarvisData setup
cargo run -p jarvis-master -- --data-dir $jarvisData serve
```

Then, from a second PowerShell window:

```powershell
$jarvisData = Join-Path $env:LOCALAPPDATA 'Jarvis\master'
cargo run -p jarvis-master -- --data-dir $jarvisData health
cargo run -p jarvis-master -- --data-dir $jarvisData fixture-worker
```

The executable rejects non-loopback binds, requires its generated development
token on every route, and refuses a second process for the same data directory.
It is a foreground developer process; Windows service installation and remote
device authentication remain later slices.

```sh
./scripts/release-version-consistency.sh --check
./scripts/release-ci-workflow-smoke.sh
./scripts/release-docs-drift-smoke.sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace -- --ignored
./scripts/storage-migration-backup-smoke.sh
cargo build --workspace
cargo run -p jarvis-cli -- smoke
./scripts/release-operator-qa-smoke.sh
./scripts/release-cargo-package.sh
./scripts/package-distribution.sh --check
./scripts/package-distribution.sh --check-guidance-self-test
./scripts/package-distribution.sh --entitlements-policy-self-test
./scripts/package-distribution.sh --version-consistency-self-test
./scripts/package-distribution.sh --provenance-self-test
./scripts/package-distribution.sh --running-app-guard-self-test
./scripts/package-distribution.sh --running-app-guard-e2e
./scripts/package-distribution.sh --unsigned-launch-check
cargo run -p jarvis-cli -- release signed-distribution-runbook
cargo run -p jarvis-cli -- release live-device-runbook
cargo run -p jarvis-cli -- release plugin-trust-runbook
# Equivalent read-only IPC surfaces for app clients:
# GET /release/signed-distribution-runbook
# GET /release/live-device-runbook
# GET /release/plugin-trust-runbook
./scripts/release-live-device-qa.sh --check
./scripts/release-live-device-qa.sh --self-test
./scripts/release-plugin-trust-qa.sh --check
./scripts/release-plugin-trust-qa.sh --self-test
./scripts/release-evidence-bundle.sh --check
./scripts/release-evidence-bundle.sh --self-test
./scripts/release-evidence-doctor.sh --check
./scripts/release-evidence-doctor.sh --self-test
./scripts/release-external-handoff.sh --check
./scripts/release-external-handoff.sh --self-test
swift test --disable-sandbox --package-path apps/mac
swift build --disable-sandbox --package-path apps/mac
```

Focused workflow-shape check:

```sh
./scripts/release-ci-workflow-smoke.sh
```

This smoke verifies the pinned GitHub workflow still runs the canonical
`./scripts/release-local.sh` gate, enforces the CI heartbeat environment, and
runs `./scripts/release-local.sh --heartbeat-self-test` without recursively
running the full release gate. It also verifies the SHA-pinned
`windows-protocol.yml` workflow keeps the portable protocol and master-kernel
format, clippy, and test commands intact.

Focused release-doc drift check:

```sh
./scripts/release-docs-drift-smoke.sh
```

This smoke parses `scripts/release-local.sh`, verifies the release command
matrix is represented in `docs/build-test-commands.md` and
`docs/release-checklist.md`, and checks the architecture map plus KB for the
external evidence-mode, command-evidence, owner-evidence, and handoff-script
boundary phrases. It is documentation contract coverage only.

Focused installed-WASM confinement checks:

```sh
cargo test -p jarvis-core wasm -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e installed_wasm -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter pluginManagerModelDecodesWasmConfinement
cargo test -p jarvis-core installed_wasm_model_tools_require_opt_in_and_execute_only_on_local_route -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e model_planned_installed_wasm_requires_opt_in_and_revalidates_before_execution -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter commandConsoleInstalledWasmToolsAreExplicitOptIn
./scripts/release-docs-drift-smoke.sh
```

The Rust tests cover the `jarvis_json_v1` no-import ABI, `wasm_compute` grant,
exact-byte provenance, schema-v12 migration, byte/memory/table/fuel ceilings,
pause/cancel/timeout behavior, audit redaction, and cross-process restart. The
model-planning checks additionally cover default-off command behavior,
reactive-local-only advertisement, successful eligible execution, immediate
state/provenance revalidation, mutation denial, and the Swift per-command
opt-in. The Swift command-console test also proves execution remains a separate
default-off operator toggle: installed schemas may be planned while dry-run is
on, and guest entry is possible only when tool execution is explicitly enabled.
The Swift confinement test covers only backward-compatible decoding and
read-only redacted presentation. These checks prove Wasmi language-level confinement, not an OS
sandbox, same-user IPC, marketplace/publisher trust, malware analysis,
signing/notarization, or live-device behavior.

## Current Health Check

`jarvis health` is a strict IPC liveness check. Strict IPC commands such as
`jarvis command`, pause/resume, scheduler, task/audit/activity/route, memory,
approval, diagnostics, installed-plugin, and permission-center operations also
require a reachable repository-backed core. If the endpoint is down, these
commands exit non-zero with operator guidance to start `jarvis serve`, run the
offline ephemeral `jarvis smoke` check, or use read-only fallback inspection
commands such as `jarvis release readiness`, `jarvis plugins list`, and
`jarvis tools list`, instead of returning a raw connection-refused error.

The packaged app defaults to a generation-random Unix domain socket with a
fresh in-memory bearer. The strict startup document carries
`ipc_transport:{kind:"unix_socket_peer_identity_v1",socket_path:
"/absolute/path.sock",peer_code_requirement:"...",peer_identity_profile:
"adhoc_exact|developer_id_hardened"}` and the bearer. The runtime directory is
current-owner `0700` and the socket is `0600`. Both peers use
`LOCAL_PEERTOKEN` plus Security.framework dynamic-code validation against the
expected designated requirement before framing, and also require the connected
peer's `getpeereid` EUID to equal their current EUID. One four-byte big-endian length frames one strict bounded
JSON request, a required client write-half close, and one response per connection;
all existing routes still require the bearer. Frame/body/hard-deadline/concurrency
bounds and leaf-only cleanup fail
closed.

To inspect the app core from a bundled or development CLI, explicitly launch
the app with exact `JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true`. This replaces the
default UDS with the weaker authenticated loopback TCP/token compatibility
path. Once enabled, use the app-owned owner-only handoff file (the global
option may appear before or after the subcommand):

```sh
cargo run -p jarvis-cli -- --ipc-token-file \
  "$HOME/Library/Application Support/Jarvis/ipc-session-auth.json" \
  health --endpoint http://127.0.0.1:7787
```

The CLI opens the file no-follow, requires a bounded single-link regular file
owned by the current user with mode no broader than `0600`, validates the strict
versioned JSON document, rejects any resolved non-loopback endpoint, and sends
only the bearer header. This opt-in is weaker because any process running as the
same user can read the file while it exists. Do not print, copy,
or pass the credential through argv or environment. A manually started legacy
`jarvis serve` remains explicitly unauthenticated and rejects any Authorization
header.

When CLI handoff is explicitly enabled, an absolute `JARVIS_MAC_IPC_AUTH_FILE`
may override the standard path. The unsigned distribution launch lane uses
that override inside its temporary profile because macOS
Application Support discovery is not redirected by a synthetic `HOME`. The app
uses that path only for the enabled owner-only handoff file. The supervisor
removes `JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF`, `JARVIS_MAC_IPC_AUTH_FILE`,
`JARVIS_MAC_RELEASE_SMOKE`, and `JARVIS_IPC_TOKEN_FILE` from the child server environment. Normal packaged
launches use UDS and do not create a handoff file or TCP listener.
Authenticated app-supervised launches include the non-secret
`--supervised-parent-pid <swift-app-pid>` argument. The core validates it is a
non-init direct parent before opening SQLite and watches that relationship for
the server lifetime. Abrupt app termination therefore drops the IPC server and
releases the database owner lease; manual or externally supervised `jarvis
serve` processes omit this argument and remain operator-owned. Bearer authority
continues to arrive only through bounded startup stdin.
The unsigned launch lane sets exact `JARVIS_MAC_RELEASE_SMOKE=true` only on the
app and requires its non-secret readiness line, which is emitted only after the
Swift client completes authenticated health, dry-run command, task and audit
inspection, diagnostics, pause, blocked-command, and resume checks through the
default UDS. Failure at any step suppresses success and best-effort resume
cleanup prevents the smoke from intentionally leaving the core paused. The
variable never reaches the core.

Focused regression and E2E proof for this boundary:

```sh
swift test --disable-sandbox --package-path apps/mac
cargo test -p jarvis-cli --test local_ipc_e2e -- --nocapture
./scripts/package-distribution.sh --unsigned-launch-check
./scripts/release-docs-drift-smoke.sh
```

Coverage must prove default UDS cross-process route parity, successful real-peer
audit-token/designated-requirement checks plus same-EUID wrong-code rejection
before framing, negative EUID comparison, bearer failure, strict frame/schema/base64
and trailing-input decoding, frame/body hard deadlines and configured concurrency
bounds, socket path/mode/owner validation,
generation cleanup and restart invalidation. The distribution lane must prove
the release-built app and Swift client traverse health, command, task/audit,
diagnostics, pause/block/resume over that same default UDS, persist the expected
task/audit state, and finish durably resumed before teardown. It must separately prove that the
app launch includes its exact direct parent PID, abrupt app termination makes
the supervised core self-exit and release the database owner lease, and a
second launch can open the same database without manual cleanup. It must also prove that the
exact opt-in selects loopback TCP, creates and lifecycle-clears the hardened
file, accepts only an absolute override, strips app-only variables from the
child, and preserves no-downgrade behavior. The unsigned/ad-hoc lane verifies
stable identifiers and exact-build cdhash mechanics but not Developer ID
publisher identity. These lanes do not prove device authentication, XPC, App
Sandbox, notarization, or live-device behavior.

`jarvis smoke` starts an ephemeral loopback server and verifies the currently
implemented foundation surfaces: health, command execution, pause blocking,
resume, plugin manifest listing, and repository-backed task plus explicit
memory-management paths. Raw memory list/get endpoints are not advertised as
safe inspection paths in `/contract`; use `/memory/classification` for the
safe aggregate memory summary.

```sh
cargo run -p jarvis-cli -- smoke
```

Release-readiness triage can run before starting a server. The command prefers
`/release/readiness` from a running IPC endpoint, then falls back to the same
conservative local summary when the endpoint is unavailable. By default it
treats standard release reports as inventory only; release operators can enable
evidence-aware blocker clearing by starting or restarting the core with
`JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external` after preserving the relevant
QA reports; the structured readiness payload exposes `evidence_mode_enabled` so
operators can distinguish conservative local mode from a core that was actually
started in external evidence mode:

```sh
cargo run -p jarvis-cli -- release readiness
```

Evidence-aware readiness only accepts release reports that pass
`release evidence-status` semantic checks. The app bundle item checks
`Contents/Info.plist` for the expected bundle ID, short version, build
version, and approved microphone/Speech privacy prompt copy before it can count
as present, and the bundled core item checks the
packaged `Contents/Resources/bin/jarvis-cli.version` marker without executing
the artifact path. Missing or stale bundled-core markers should be remediated
by rerunning `./scripts/package-distribution.sh --unsigned-launch-check` for
local evidence, or the signed packaging lane for final release evidence.
Live-device QA checks schema/type,
`self_test_fixture=false`, expected bundle ID, matching short/build version,
bundled-core path/version/SHA-256 binding, repository-backed command-result
evidence, ordered non-future UTC voice-check timestamps, and structured
scheduler notification observation. Plugin-trust checks ordered non-future UTC
review and egress validation timestamps,
`review_source=owner-asserted-manual-review`, non-empty owner-recorded
marketplace, malware, signed-publisher, OS sandbox, host-egress, deny-fixture,
allow-fixture, and manual-review evidence fields, plus per-category archived
artifact URI/SHA-256 bindings. The final evidence bundle checks the expected release version,
signed-distribution provenance report, SHA-256 digest shape, semantic validity
of the signed-provenance, live-device QA, and plugin-trust QA child reports
referenced by the bundle, and `local_signature_validation=true`. Wrong
bundle/version metadata, malformed timestamps, future-dated timestamps,
reversed timestamps, missing signed provenance, non-owner plugin-trust review
source, disabled local signature validation, or a self-test fixture leave
evidence invalid.

Focused regression checks for that release evidence boundary:

```sh
cargo test -p jarvis-core live_device_qa_report -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e release_readiness_rejects_semantically_invalid_live_voice_evidence -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e release_readiness_rejects_missing_live_voice_evidence_fields -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e release_live_device_qa_script_generated_report_clears_evidence_status -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e release_plugin_trust_qa_assertion_report_is_accepted_by_evidence_status -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e release_help_surfaces_current_evidence_boundaries -- --nocapture
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

For a cold Ollama model, warm it before starting Jarvis or increase the local
provider timeout:

```sh
ollama run llama3.2 "Say hello in one short sentence."
JARVIS_LOCAL_MODEL_TIMEOUT_MS=60000 cargo run -p jarvis-cli -- serve
```

The Swift Model tab can also supervise this configuration without editing shell
environment variables. It reads the local Ollama inventory from `/api/tags`,
shows installed models plus recommended downloadable Gemma/Qwen/Llama/Mistral
options with estimated RAM, pulls missing selections through streamed
`/api/pull` progress, automatically reloads inventory after completion, treats
Ollama `:latest` aliases as installed for the selected base model, enables
Start only after the selected model is installed, loads or unloads the selected
model through `/api/generate` keep-alive requests, and restarts the supervised
core with `JARVIS_LOCAL_MODEL_PROVIDER`, `JARVIS_LOCAL_MODEL`,
`JARVIS_OLLAMA_BASE_URL`, and `JARVIS_LOCAL_MODEL_TIMEOUT_MS` overrides. If
another terminal already owns the core port, stop that external `jarvis serve`
process before applying the Model tab restart.

If a Model tab download fails with `pull model manifest: 412` and says the model
requires a newer version of Ollama, use **Upgrade Ollama…** and confirm the
Homebrew mutation before retrying the same model. The button is enabled only for
a loopback endpoint and a non-busy Model tab. It verifies a Homebrew formula,
invokes a fixed Homebrew executable without a shell, filters unrelated inherited
environment values, verifies the version after upgrade, and restarts the Ollama
Homebrew service only when it was already running. Remote or non-Homebrew Ollama
installations remain manual update paths and a stopped service is not silently
started.

The focused process-level E2E uses a temporary fake Homebrew executable, not the
machine's real package manager, while exercising the Model configuration model,
real Foundation process runner, version transition, exact command order, and
already-running service restart:

```sh
swift test --disable-sandbox --package-path apps/mac --filter ollamaUpgradeProcessEndToEnd
```

Live local testing with `llama3.2` has proven this Ollama route can complete
real model commands. Local model behavior is still model-dependent, so the
runtime derives the default provider-visible tool catalog from validated
first-party manifests, exposes that redacted default through `jarvis tools
list`, advertises it as an Ollama JSON allowlist and ChatGPT/OpenAI-compatible
native tool definitions, and keeps validating every model-planned tool request
before execution. A reactive local command may explicitly add eligible
installed WASM schemas with `--installed-wasm-tools`; this never changes the
cloud provider catalog.

Provider tool troubleshooting: if Ollama or a ChatGPT/OpenAI-compatible provider
requests `plugin_id: "status"` or `plugin_id: "chrome_extension"`, that is a
provider hallucination, not a missing installed plugin. Inspect the default
model-visible catalog with `cargo run -q -p jarvis-cli -- tools list`,
`cargo run -q -p jarvis-cli -- tools model`, or
`cargo run -q -p jarvis-cli -- tools catalog`. Production inventory contains
`system_status.status` and excludes deterministic `fake_*` test fixtures.
The macOS app is the production owner of user-selected workspace bookmarks and
passes resolved roots through bounded startup stdin. For compatibility testing,
the CLI still accepts a legacy explicit root flag:

```bash
cargo run -p jarvis-cli -- serve \
  --workspace-root project=/absolute/path/to/project
cargo run -q -p jarvis-cli -- tools list
cargo test -p jarvis-core workspace_inspect -- --nocapture
cargo test -p jarvis-core runtime_cancellation -- --nocapture
cargo test -p jarvis-core explicit_command_handle_cancels_only_its_active_model_transport -- --nocapture
cargo test -p jarvis-core consumed_runtime_cancellation_tombstones_are_bounded_fifo -- --nocapture
cargo test -p jarvis-core command_cancellation_response_distinguishes_active_from_not_found -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e active_command_cancellation_is_end_to_end_and_finalized_handles_report_not_found -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter commandConsoleCancelsItsActiveSubmission
swift test --disable-sandbox --package-path apps/mac --filter commandConsoleSerializesConcurrentSubmissions
cargo test -p jarvis-cli --test local_ipc_e2e production_workspace -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter WorkspaceRoot
```

For an operator-visible cancellation handle, submit and cancel from separate
terminals while the command is active:

```bash
HANDLE="$(uuidgen)"
cargo run -p jarvis-cli -- command "status check" \
  --cancellation-id "$HANDLE" --endpoint http://127.0.0.1:7787 --json
cargo run -p jarvis-cli -- cancel-command "$HANDLE" \
  --endpoint http://127.0.0.1:7787
```

The command request `cancellation_id` field is additive and optional for older clients. Current
Swift and CLI clients generate the UUID before `POST /commands`; Swift shows
Cancel only while that submission is active. A cancellation response with
`outcome: cancellation_requested` proves the exact active handle accepted the
signal. `outcome: not_found` means no matching execution was active when the
authenticated `POST /runtime/cancellations/:id` request linearized. If cancellation wins finalization, the response is
cancelled and late steps/tool results are suppressed. This is cooperative
local-process cancellation and cannot undo an external effect already applied.
The core also rejects reuse of the 1,024 most recently consumed UUIDs through a
bounded FIFO tombstone set, so delayed stale cancels cannot target new work in
that window. Tombstones are process-local and eventually evicted; clients must
always generate fresh random UUIDs rather than intentionally recycle handles.

`workspace_inspect.list` and `workspace_inspect.read_text` are absent when no
root is configured. They use only opaque root IDs plus relative paths, enforce
descriptor-anchored no-follow traversal and hard output budgets, reject
hidden/credential-like/symlink/special/binary/oversized targets, and are
local-model-only. Their audit evidence contains metadata, not contents or
absolute paths. Installed plugins remain outside model-originated planning
unless an individual reactive local-model command explicitly opts in to the
eligible installed `local_wasm` subset described below.
The cross-process workspace E2E uses the versioned startup-stdin envelope and
checks that app-style root paths are absent from argv; a separate compatibility
case covers the legacy flag. These commands prove bookmark/store/supervisor and
bounded repository contracts, not macOS sandboxing, child sandbox-extension
inheritance, same-user IPC isolation, malware resistance, signed distribution,
live-device QA, or marketplace trust.
Use `path: "@root"` only when listing the configured root; empty paths are
invalid. The current ceilings fail closed beyond 200 visible entries, 64 KiB
per read, 16 KiB per line, and 128 KiB cumulative tool output per task.
Restart E2E also preserves unresolved historical `fake_*` approvals as
critical `removed_fixture_approval` policy-review attention while proving the
fixture action cannot execute; it never deletes approval history silently.

The interactive CLI defaults to operator-readable text for `jarvis command`
and its `jarvis ask` alias, `jarvis plugins list/get`, `jarvis tools list`,
`jarvis tasks list/get/audit`, `jarvis routes list/get`,
`jarvis activity summary`, `jarvis release readiness`, and
`jarvis release evidence-status`. Use `--json` on those commands when a
script, test, or debugging session needs the exact IPC payload with full audit,
route, readiness, or evidence inventory details. For compatibility with older
automation, `jarvis release readiness --format json` and
`jarvis release evidence-status --format json` are accepted as aliases for the
release inspection JSON payloads. The three release runbook commands also
accept `--format json` as an alias for their structured summaries.
`JARVIS_CLI_JSON=1` is available for test harnesses that need to keep all CLI
calls machine-readable.

Manual model smoke recipes, run from a second terminal while the server above
is running:

```sh
cargo run -p jarvis-cli -- health
cargo run -p jarvis-cli -- tools model --json
cargo run -p jarvis-cli -- ask "Check Jarvis status and explain it in plain English."
cargo run -p jarvis-cli -- command --json "plugin status"
cargo run -p jarvis-cli -- routes list --json
cargo run -p jarvis-cli -- diagnostics export
```

Use the same second-terminal commands for fake, Ollama, or
ChatGPT/OpenAI-compatible server runs. Provider-specific failures should show
up as failed command responses with redacted route/audit evidence rather than
as missing CLI/server setup.

To exercise the opt-in ChatGPT/OpenAI-compatible provider boundary, disable
the local provider and provide an API key. The key is never serialized in
provider config, provider status, route evidence, diagnostics, or structured
provider errors:

```sh
JARVIS_LOCAL_MODEL_ENABLED=false \
JARVIS_CHATGPT_ENABLED=true \
JARVIS_CHATGPT_AUTH=api_key \
JARVIS_OPENAI_API_KEY=... \
JARVIS_CHATGPT_MODEL=gpt-4.1-mini \
cargo run -p jarvis-cli -- serve
```

To use the ChatGPT/Codex account route without OpenAI Platform billing, sign in
once with the Codex CLI and then select the Codex account auth mode:

```sh
codex login --device-auth

JARVIS_LOCAL_MODEL_ENABLED=false \
JARVIS_CHATGPT_ENABLED=true \
JARVIS_CHATGPT_AUTH=codex_account \
JARVIS_CHATGPT_MODEL=gpt-5.6-sol \
JARVIS_CHATGPT_REASONING_EFFORT=medium \
JARVIS_CHATGPT_REQUIRES_APPROVAL=false \
JARVIS_CODEX_EXECUTABLE="$(command -v codex)" \
cargo run -p jarvis-cli -- serve
```

If a sensitive macOS Console command stops at `waiting_for_approval`, choose
`Approve & Send` to retry that exact command with `cloud_route_approved=true`.
The Model tab's `Ask before every cloud prompt` toggle may also deliberately
apply that flow to ordinary conversation. The approval
is one-shot, accepted only for a non-proactive request over authenticated local
IPC, and does not override the Restricted cloud block.

Jarvis sends the redacted request to this subprocess over stdin, uses a private
temporary final-message file, clears unrelated inherited environment values,
ignores user config and project rules, disables approvals, and requests the
Codex CLI read-only sandbox. It also uses strict config with web search disabled,
disables the current CLI tool/integration feature set, discards subprocess
logs, kills the child if its private response file crosses 1 MiB, and repeats
the size check before reading. Prompt delivery runs concurrently with timeout
and response-file monitoring, so a child that never reads stdin cannot bypass
either bound. The Jarvis request payload contains only
redacted route context, although Codex supplies its own runtime/system context.
The configured CLI must support this full constrained argument contract; an
older or incompatible CLI fails closed with update/login guidance before model
execution. The CLI E2E suite exercises the real
`jarvis serve` boundary with a stub Codex executable and verifies the expected
argument contract, health auth-mode reporting, prompt stdin, environment
minimization, response routing, and path/secret redaction. It also proves the
Console approval contract across IPC: a Personal command first stops at
`waiting_for_approval`, the exact retry with `cloud_route_approved=true`
executes, and a later unapproved command stops again because the grant is not
session-wide.

Focused Codex-account approval E2E:

```sh
cargo test -p jarvis-cli --test local_ipc_e2e serve_executes_codex_account_through_constrained_cli_subprocess -- --nocapture
```

If the selected local or ChatGPT/OpenAI-compatible provider fails during
execution, `/commands` now returns a normal failed command response with
redacted `model_step_failed` audit evidence and route evidence instead of an
IPC transport error.

Ollama-compatible and ChatGPT/OpenAI-compatible text responses may also return
a strict JSON envelope with `message`, `complete`, and `tool_requests`.
Accepted `tool_requests` are fed into the same bounded route-scoped schema,
policy, approval, and audit path as fake-model tool plans; malformed envelopes
fail with redacted diagnostics. Unknown plugin IDs, undeclared actions, and
malformed inputs fail closed before policy check or tool execution, emit
`tool_request_rejected` audit evidence, and are returned to the model as
`rejected` tool results for bounded recovery. Oversized tool plans still fail
the task. ChatGPT/OpenAI-compatible responses may also return native OpenAI
`tool_calls` for the runtime-derived first-party inventory; these are
translated into the same bounded first-party path. Installed WASM definitions
are available only to an explicitly opted-in reactive local route and are
sorted and capped at 16 actions, 1 KiB per description, 16 KiB per input
schema, and 64 KiB combined; they are never available to this cloud path. This
is provider tool compatibility, not broad installed-plugin
orchestration.

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

`Jarvis.app` exposes the same loop through the Scheduler tab's default-off
automation setting. Apply the setting to restart the app-supervised core; the
app clamps the interval to at least 1,000 ms and limits each tick and stale
recovery batch to 64. Exact `JARVIS_MAC_SCHEDULER_AUTOMATION_ENABLED=true` is
reserved as an explicit packaged-test/operator launch opt-in. A test may also
lower the interval with `JARVIS_MAC_SCHEDULER_AUTOMATION_INTERVAL_MS`, subject
to the same 1,000 ms floor. Both overrides are ephemeral and stripped from the
child environment. Focused proof:

```sh
swift test --disable-sandbox --package-path apps/mac --filter schedulerAutomation
swift test --disable-sandbox --package-path apps/mac --filter schedulerAttentionCoordinator
swift test --disable-sandbox --package-path apps/mac --filter automaticSchedulerNotifications
swift test --disable-sandbox --package-path apps/mac --filter schedulerCoordinatorRealIPCEndToEnd
swift test --disable-sandbox --package-path apps/mac --filter supervisorPreservesOwnedSchedulerAutomationAcrossRepeatedStart
./scripts/package-distribution.sh --unsigned-launch-check
```

The Swift tests prove persistence, ephemeral environment overrides, bounds,
generation-safe lifecycle cancellation including delayed authorization,
repeated-start ownership, real-IPC durable outbox submission/acknowledgement,
restart replay, partial acknowledgement failure handling, no-prompt
authorization, and stable occurrence-revision requests. The unsigned launch check proves the packaged
app starts its supervised core with bounded scheduler/recovery arguments and
uses its authenticated Swift client to create and observe an automatically
completed due job plus matching scheduler audit and outbox acknowledgement
evidence. CLI E2E independently proves pause behavior, bounded ticks, durable
restart replay, redaction, and CAS acknowledgement. The handoff is at-least-once;
none of these checks prove exactly-once delivery, live OS display, or OS wake
reliability.

To recover stale persisted `Running` scheduler jobs before a repository-backed
server starts accepting IPC traffic, opt in explicitly:

```sh
cargo run -p jarvis-cli -- serve \
  --db-path /tmp/jarvis.sqlite \
  --scheduler-recover-stale-on-startup \
  --scheduler-stale-older-than-seconds 3600 \
  --scheduler-stale-recovery-limit 16
```

Startup recovery uses the same redacted stale-recovery audit path as
`scheduler recover-stale`, with `automatic_recovery: true`. It does not expose
scheduler command text or run stale job side effects.

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
cargo run -p jarvis-cli -- ask "Check Jarvis status and explain it in plain English."
cargo run -p jarvis-cli -- command --json "plugin status"
cargo run -p jarvis-cli -- plugins list
cargo run -p jarvis-cli -- plugins list --json
cargo run -p jarvis-cli -- tools list
cargo run -p jarvis-cli -- tools list --json
cargo run -p jarvis-cli -- release readiness
cargo run -p jarvis-cli -- diagnostics export
cargo run -p jarvis-cli -- permissions grants
cargo run -p jarvis-cli -- activity summary
cargo run -p jarvis-cli -- activity watch --max-events 2 --interval-ms 500
cargo run -p jarvis-cli -- scheduler list
cargo run -p jarvis-cli -- scheduler schedule "manual check" "status check"
cargo run -p jarvis-cli -- scheduler schedule "approval fail closed" "plugin approval echo scheduler pause"
cargo run -p jarvis-cli -- scheduler run-due --limit 1
cargo run -p jarvis-cli -- scheduler notifications --limit 64
cargo run -p jarvis-cli -- scheduler acknowledge-notification <occurrence-uuid> --revision 1 --disposition suppressed-not-authorized
cargo run -p jarvis-cli -- scheduler recover-stale --older-than-seconds 3600 --limit 16
cargo run -p jarvis-cli -- plugins installed
cargo run -p jarvis-cli -- pause --reason "manual smoke"
cargo run -p jarvis-cli -- pause-status
cargo run -p jarvis-cli -- resume
```

Current boundary: the command endpoint runs `ConversationRuntime` with
`FakeLocalModel` by default, an opt-in Ollama-compatible local HTTP provider, or
an opt-in ChatGPT/OpenAI-compatible HTTP provider from typed env config. It
records local-first `ModelRouter` audit evidence, sends ChatGPT only minimized
redacted route context after policy selection, can execute bounded production
first-party commands such as `status` through the policy
engine, honors `--dry-run` for plugin execution, and can persist task/audit
state plus redacted model-route records when configured with a
repository-backed IPC state. It also has deterministic coverage for bounded
model-planned first-party tool calls and explicit default-off reactive-local
installed WASM calls.
Approval-required command scaffolds such as `plugin approval echo ...` fail
closed by returning `waiting_for_approval`, persisting an inspectable pending
approval when repository backing is enabled, and requiring a separate CLI/IPC
grant or denial. Grant and denial recheck pending state and commit the decision
plus a redacted decision audit in one immediate transaction. Audit failure
rolls all decision fields back to pending; actor and reason remain available on
the approval record but are absent from audit payloads, so no unaudited grant
chain can become execution authority. Direct installed-plugin runs also pass
the permission engine: dry runs do not execute, eligible Low/default-sensitivity
requests remain direct, and Confirm or sensitive requests persist approval
without runtime entry. Granting an approval does not execute the side effect;
approved first-party and installed-plugin actions require an explicit
`jarvis approvals execute <approval-id>` replay, which verifies the original
task, action, risk, scope, input-schema, current-policy contract, and matching
approval_granted audit evidence before
schema v13 atomically creates a unique durable execution claim with redacted
policy/claim audit evidence. Only the successful claimant invokes the plugin;
duplicate and post-restart replay returns conflict/HTTP 409. Terminal execution
state, task state, and terminal audits commit together. Any durable claim
permanently consumes that approval: failure, cancellation, timeout, restart, or
a storage interruption after claim can leave the effect ambiguous, and
automatic retry is forbidden. Inspect audit evidence and create a new approval
when another attempt is appropriate. On restart, schema v16 projects a
pre-existing unresolved claim into a separate redacted attention ledger before
IPC starts. `jarvis approvals attention` lists unacknowledged rows without
action/input/reason/actor/path/digest data and distinguishes the true total from
the returned count, 100-item limit, and truncation flag. `jarvis approvals
acknowledge-without-retry <execution-id> --revision <observed-revision>` sends
the exact revision plus the fixed `acknowledged_without_retry` disposition.
The CAS records review but does not enter the plugin, alter/delete the consumed
claim, create a new approval, or authorize a retry. Installed-plugin approvals additionally
use a schema-v15 private binding over canonical input, manifest/provenance, and
execution grant. The binding is revalidated before claim and its input/digests
are redacted from public and audit surfaces.

CLI and Swift generate a fresh cancellation UUID for every approved execution.
The CLI also accepts an explicit handle when coordinating a separate cancel:

```sh
HANDLE="$(uuidgen | tr '[:upper:]' '[:lower:]')"
cargo run -p jarvis-cli -- approvals execute <approval-id> --cancellation-id "$HANDLE"
cargo run -p jarvis-cli -- plugins cancel-run "$HANDLE"
```

Cancellation is accepted only while that exact approved execution is active.
When it wins output acceptance, the durable claim and task become cancelled and
late output is discarded; an external effect already performed is not undone.
The Approval Center uses the same retained UUID for its in-flight Cancel Run
control and clears it when execution completes.

Focused one-shot approval proof:

```sh
cargo test -p jarvis-core concurrent_approved_execution_has_exactly_one_winner -- --nocapture
cargo test -p jarvis-core approval_decision_and_redacted_audit_commit_or_roll_back_together -- --nocapture
cargo test -p jarvis-core approved_row_without_matching_grant_audit_cannot_be_claimed -- --nocapture
cargo test -p jarvis-core matching_legacy_raw_metadata_grant_audit_remains_claimable -- --nocapture
cargo test -p jarvis-core migration_13_backfills_completed_approval_execution_as_consumed -- --nocapture
cargo test -p jarvis-core claimed_approval_restart_attention_is_insert_once_redacted_and_acknowledged_by_cas -- --nocapture
cargo test -p jarvis-core approval_execution_attention_count_is_not_limited_by_the_bounded_page -- --nocapture
cargo test -p jarvis-core approval_execution_attention_revision_overflow_fails_closed -- --nocapture
cargo test -p jarvis-core file_backed_repository_owner_lease_is_exclusive_and_released -- --nocapture
cargo test -p jarvis-core repository_owner_lease_serializes_preflight_backup_and_migration -- --nocapture
cargo test -p jarvis-core repository_owner_lease_rejects_symlink_and_insecure_lock_before_database_creation -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e file_backed_core_ownership_blocks_second_process_and_defers_live_claim_reconciliation -- --nocapture
cargo test -p jarvis-core installed_plugin_pending_approval_binds_input_without_audit_disclosure -- --nocapture
cargo test -p jarvis-core installed_plugin_confirm_invocation_requires_bound_one_shot_approval -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e authenticated_approved_installed_execution_can_be_cancelled_after_claim -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e approval_decision_audit_failure_rolls_back_across_cli_ipc_and_restart -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e approved_row_without_grant_audit_cannot_claim_or_enter_plugin_across_restart -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e concurrent_approved_execution_is_one_shot_across_ipc_and_restart -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e serve_exposes_local_ipc_contract_and_persists_state -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter approvalManagementModel
```

The storage decision test injects audit failure for both grant and denial and
proves whole-transaction rollback. The real-server CLI IPC test proves a
failed grant remains pending and non-executable across restart before recovery
creates one redacted audit. The Rust race test proves one claimant, the schema-v13 migration test proves
historical terminal audit evidence remains consumed, the cross-process IPC E2E
proves one HTTP winner plus durable HTTP 409 after restart, and the Swift tests
prove duplicate-submit suppression and claimed-approval hiding after refresh.
The installed-plugin tests prove Low compatibility, Confirm/sensitive non-entry,
schema-v15 input/contract binding and redaction, explicit approved execution,
and permanent replay consumption through the real CLI/IPC path.
The authority-chain tests prove an approved row without a matching grant audit
cannot claim or enter the plugin across restart, unrelated audits cannot be
substituted, and the exact legacy raw-metadata audit shape remains compatible.
`jarvis permissions grants` reads the combined local grant
surface: approval counts/history, high-risk pending count, installed-plugin
`metadata_only` grant records, and the invariant that side effects still
require approval. Installed `local_subprocess` plugins remain disabled by
default and execute only after an explicit `subprocess_stdio` grant through the
constrained JSON stdin/stdout runner. The Swift Plugin tab reads the same
installed registry records and adds typed, non-executing provenance
verification plus explicit compatible-grant enable/disable controls. It
serializes each plugin's mutations, shows exact declared permissions and hosts,
binds enable/disable to the confirmed lifecycle-contract digest, refreshes the
authoritative registry after every result, disables lifecycle actions while
that registry is stale, and keeps first-party
manifests visible if the repository-backed registry endpoint is unavailable.
Rust commits each grant mutation and its redacted non-execution audit
atomically. The Swift app now exposes the
Speech/AVFoundation input adapter controls and AVFoundation speech-output
preview controls, but release claims for real voice still require entitlement
packaging, live microphone checks, live audio-output checks, and manual device
validation. Swift approval decision, approved-run, plugin registry, and voice
controls are covered by the Swift contract/model tests, including
speech-output utterance identity so stale AVSpeech completion/cancel callbacks
cannot make newer playback look idle.
Local plugin install is metadata-only:
`jarvis plugins install /absolute/path/to/jarvis-plugin.json` validates and
stores a disabled registry record with local provenance hashes when repository
backing is enabled. Use `jarvis plugins verify-installed <id>` before enabling
local subprocess execution; enablement fails closed unless the manifest and
subprocess command still match the install snapshot. Non-network subprocess
plugins use the default `jarvis plugins enable-installed <id>` grant; plugins
with network-declaring actions must use
`jarvis plugins enable-installed <id> --grant subprocess_stdio_network`. Use
`jarvis plugins verify-publisher <id> --trusted-origin "<manifest author>"`
only after provenance matches to mark the manifest author claim as
operator-reviewed. For signed manifests, use
`jarvis plugins verify-publisher-signature <id> --trusted-public-key "<base64 ed25519 public key>"`
after provenance matches; this verifies the portable manifest identity
signature against the explicit trusted key with local `source_path` omitted,
but still does not prove marketplace approval or malware safety.

Audited local update and redacted lifecycle history require a running
repository-backed core:

```sh
cargo run -p jarvis-cli -- plugins update-preview <id> /absolute/path/to/jarvis-plugin.json
cargo run -p jarvis-cli -- plugins update-apply <id> /absolute/path/to/jarvis-plugin.json \
  --expected-lifecycle-contract-sha256 <64hex> \
  --expected-candidate-update-contract-sha256 <64hex> \
  --confirm
cargo run -p jarvis-cli -- plugins history <id>
```

New local installs require valid SemVer 2.0.0. Preview validates the exact
installed lifecycle digest, plugin identity, unchanged source/runtime kind,
normally strictly newer SemVer candidate, and bounded
local snapshot. It returns `current_lifecycle_contract_sha256` and opaque
`candidate_update_contract_sha256`. Confirmed apply requires both values from
that visibly reviewed preview and never auto-refreshes or substitutes them. It
reloads the candidate and rejects lifecycle or exact-snapshot drift before the
atomic mutation. Success preserves the installed record identity, replaces metadata
and provenance, appends `installed_plugin_updated`, and resets execution to
disabled `metadata_only`; verify and explicitly enable again before execution.
A persisted pre-SemVer record may make one fully governed transition to valid
SemVer; every later update is strictly ordered by SemVer precedence.
History returns at most 100 newest plugin-scoped entries with only entry ID,
plugin ID, action, outcome, and timestamp. This is local lifecycle evidence,
not marketplace, publisher, malware, OS-sandbox, host-egress, signing,
notarization, or live-device proof.

Focused installed-plugin lifecycle verification:

```sh
cargo test -p jarvis-core semantic_version_update -- --nocapture
cargo test -p jarvis-core installed_plugin_update_is_cas_bound_atomic_and_persistent -- --nocapture
cargo test -p jarvis-core installed_plugin_update_rejects_changed_candidate_and_rolls_back_on_audit_failure -- --nocapture
cargo test -p jarvis-core installed_plugin_history_is_plugin_scoped_newest_first_and_bounded -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e installed_plugin_update_preview_apply_history_is_cas_bound_redacted_and_persistent -- --nocapture
cargo test -p jarvis-core installed_plugin_execution_authority_and_audit_commit_atomically -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter pluginUpdateClientUsesTypedRedactedContracts
swift test --disable-sandbox --package-path apps/mac --filter pluginManagerUpdateRequiresPreviewAndConfirmation
swift test --disable-sandbox --package-path apps/mac --filter pluginLifecycleHistoryFailureDoesNotStaleRegistry
swift test --disable-sandbox --package-path apps/mac --filter pluginLifecycleClientUsesTypedContracts
swift test --disable-sandbox --package-path apps/mac --filter pluginManager
swift test --disable-sandbox --package-path apps/mac --filter pluginEnablementConfirmationIsExplicit
swift test --disable-sandbox --package-path apps/mac --filter pluginManagerRealIPCLifecycleEndToEnd
```

The update storage tests cover strict identity/version/source checks,
transactional persistence, preserved install identity/time, disabled authority,
publisher-verification reset, candidate and lifecycle compare-and-set failures,
injected audit-write rollback, and the newest-first 100-entry plugin-scoped
history bound. The cross-process CLI E2E covers duplicate-install rejection,
redacted preview tokens, changed-candidate and stale-lifecycle rejection,
confirmed apply, disabled post-update state, redacted history, restart
persistence, and required re-verification before re-enable over the authenticated
loopback compatibility server. It is not default-UDS, publisher, marketplace,
malware, OS-sandbox, host-egress, signing, notarization, or live-device proof.

The real-core Swift E2E uses the authenticated loopback-TCP compatibility path,
not the packaged app's default peer-identity-validated Unix-domain socket. With
a file-backed repository it verifies raw mutation-response redaction, exact
digest compare-and-set rejection, malformed disabled-grant rejection, enabled
restart persistence, disabled restart persistence, and audit evidence loaded
after each restart. A sentinel proves lifecycle mutation never starts plugin
code. This does not prove the default UDS transport, OS sandboxing, host-level egress enforcement,
publisher/marketplace trust, malware safety, signing, or live-device behavior.
Network-capable plugin actions must request `network` and declare
`network_access.mode: declared_hosts` with exact plain-hostname
`allowed_hosts`; policy review surfaces them as `network_plugin_action` items,
and executable installed plugins with those actions require
`subprocess_stdio_network`. This is runtime grant gating plus manifest
governance, not OS-level network sandboxing or host-level egress filtering.
Installed subprocess plugins can also emit bounded `jarvis_progress` stderr
JSON frames. Jarvis exposes only parsed sequence/stage/message progress events
and `installed_plugin_progress` audit entries. Model execution also emits
model-step completion/failure audit evidence, and model-output chunk metadata.
`/activity/events` projects these as redacted `activity_progress` frames; raw
stderr and raw model chunk text remain redacted. Ollama generation consumes the
provider's native NDJSON transport with byte/response/metadata limits,
terminal-frame validation, and in-flight runtime cancellation. Chunk metadata
is persisted and shown only after the complete stream and any tool envelope
validate; the bounded Swift watch buffers SSE evidence and does not render live
tokens or partial assistant transcript rows.

Focused checks for the activity-progress and external handoff snapshot surface:

```sh
cargo test -p jarvis-core repository_backed_state_endpoints_expose_tasks_and_audit -- --nocapture
cargo test -p jarvis-core model::tests::ollama -- --nocapture
cargo test -p jarvis-core runtime::tests::cancellation_dominates_a_model_completion_race_before_audit_or_tools -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e release_external_handoff_snapshots_match_live_runbook_commands -- --nocapture
swift test --package-path apps/mac --filter JarvisMacCoreTests/parsesActivityEventStream
./scripts/release-external-handoff.sh --self-test
```

Background scheduler execution is opt-in on `jarvis serve`; it does not start
for default smoke or manual inspection sessions unless `--scheduler-background`
is passed.

When the server is started with `--db-path`, these inspection commands are also
available:

```sh
cargo run -p jarvis-cli -- tasks list
cargo run -p jarvis-cli -- tasks list --json
cargo run -p jarvis-cli -- tasks audit
cargo run -p jarvis-cli -- routes list
cargo run -p jarvis-cli -- routes list --task-id <task-id>
cargo run -p jarvis-cli -- routes get <route-id>
cargo run -p jarvis-cli -- routes get <route-id> --json
cargo run -p jarvis-cli -- approvals list --status pending
cargo run -p jarvis-cli -- approvals approve <approval-id> --decided-by cli --reason "reviewed"
cargo run -p jarvis-cli -- approvals execute <approval-id> --cancellation-id <uuid>
cargo run -p jarvis-cli -- approvals deny <approval-id> --decided-by cli --reason "not safe"
cargo run -p jarvis-cli -- release readiness
cargo run -p jarvis-cli -- permissions review
cargo run -p jarvis-cli -- activity summary
cargo run -p jarvis-cli -- activity summary --json
cargo run -p jarvis-cli -- activity watch --max-events 2 --interval-ms 500
cargo run -p jarvis-cli -- plugins verify-publisher <plugin-id> --trusted-origin "<manifest author>" --decided-by cli
cargo run -p jarvis-cli -- plugins verify-publisher-signature <plugin-id> --trusted-public-key "<base64 ed25519 public key>" --decided-by cli
cargo run -p jarvis-cli -- memory list
cargo run -p jarvis-cli -- memory classification --include-deleted
cargo run -p jarvis-cli -- memory retention-plan
cargo run -p jarvis-cli -- memory index-status
cargo run -p jarvis-cli -- memory index-rebuild
cargo run -p jarvis-cli -- memory create workflow release-gate "run local gate before PR" --provenance "manual note" --sensitivity workspace
cargo run -p jarvis-cli -- memory restore <memory-id>
cargo run -p jarvis-cli -- command "prepare the release gate" --memory-context --sensitivity workspace
cargo run -p jarvis-cli -- scheduler attention
cargo run -p jarvis-cli -- diagnostics export
```

`jarvis permissions review` includes pending approvals, plugin review items,
active scheduler triggers, unreviewed memory items, and deleted sensitive
memory retained in local storage. `jarvis memory retention-plan` and the Swift
Memory tab expose the memory-specific redacted operator queue for those
retention actions while keeping purge/rewrite automation out of scope. Memory
review items include category/key and
sensitivity only; memory values stay out of policy review and diagnostics
export. `jarvis diagnostics export` exposes aggregate active, unreviewed, and
sensitive memory counts when repository backing is enabled. Its dedicated
health projection exposes `emergency_pause_reason_present` rather than arbitrary
emergency-pause reason text and limits the legacy reason field to null or the
fixed `redacted` compatibility marker; `/health`, pause, and pause-status remain
explicit operator surfaces with their existing reason contract.
`jarvis memory index-status` reports only projection state and counts;
`jarvis memory index-rebuild` atomically replaces the versioned local manifest
from active SQLite records. They do not themselves perform retrieval or model
routing. `jarvis command --memory-context` is a separate explicit opt-in. It
retrieves at most four reviewed active Public/Workspace/Personal records into a
4 KiB untrusted context only after a local, non-proactive route is selected.
Missing/stale/corrupt projections, high-sensitivity records, cloud routes, and
proactive requests fail closed; audit and route evidence contain counts only.
`jarvis release readiness` is read-only and summarizes implemented feature
proofs, pending feature boundaries, recommended verification commands, and
manual production blockers. The default CLI output is operator-readable and
falls back to conservative local metadata when loopback IPC is unavailable,
including restricted environments that deny loopback sockets; use
`--all-commands` for the complete readable verification runbook, or `--json` or
`JARVIS_CLI_JSON=1` for the exact structured payload. Evidence-aware mode can
clear the live voice/audio blocker from a valid live-device QA report. When the
running core has `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external`, it can
report `evidence_mode_enabled: true` and compute `production_ready: true` only when every required
`/release/evidence-status` item is present, no missing or invalid evidence
remains, and evidence-cleared features leave no pending readiness features.
That is still owner-recorded external evidence, not proof that Jarvis performed
signing, notarization, stapling, live-device QA, plugin trust QA, or manual
release QA.
The release command `--help` output is part of the operator contract: it should
preserve the read-only scope, IPC-first/local-fallback behavior, explicit
external-mode opt-in on the running core, and file/report inspection plus
semantic-validation boundary for evidence status. The
`release_help_surfaces_current_evidence_boundaries` CLI E2E test keeps
`jarvis release evidence-status --help` aligned with the default
operator-readable output, `--json` escape hatch, owner-asserted plugin-trust
review-source requirement, host-egress evidence fields, final-bundle archive URI
validation, and final-bundle local signature-validation check.
`./scripts/release-plugin-trust-qa.sh --check` is the local plugin-trust
preflight for marketplace review, malware scanning, signed publisher policy,
OS-level process/network sandbox validation and host-level egress validation.
Its `--check` output prints the exact template, source, and `--assert-complete`
commands for owner evidence capture. Its `--self-test` mode uses fake
flags and fake evidence notes to verify report generation only; real release
evidence must come from `--assert-complete` after the owner validates every
`JARVIS_PLUGIN_QA_*` flag and populates the owner/timestamp/evidence-note fields
plus the matching archived artifact URI and SHA-256 digest for each plugin-trust
category. The assertion path rejects artifact URIs that lack a URI scheme and
location or point at placeholder, self-test, fixture, or temporary paths.
The downstream evidence consumers apply the same durability rule: the bundle
script, evidence doctor, and `jarvis release evidence-status` reject plugin-trust
reports whose `evidence_artifacts.*.uri` values are bare paths or temporary
locations, even if the producer-side report was hand-edited after generation.
CLI E2E now runs `release-plugin-trust-qa.sh --assert-complete` with
owner-recorded archive URI/SHA-256 evidence fields and verifies the generated
plugin-trust QA report is accepted by `jarvis release evidence-status`.
The generated report carries `schema_version: 1` and
`evidence_type: owner_recorded_plugin_trust_qa` plus the current release
`version`; final operator evidence must also keep `self_test_fixture=false` and
`review_source=owner-asserted-manual-review`, and the doctor/status gates reject
plugin-trust reports with the wrong identity, wrong version, self-test identity,
or non-owner review source.
Use `./scripts/release-plugin-trust-qa.sh --write-template
target/release-plugin-trust-qa.env` to generate a sourceable plugin-trust QA
template. The template defaults every validation flag to `false`; operators
should edit/source it only after marketplace review, malware scan, signed
publisher policy review, OS sandbox validation, and host-level egress fixture
validation have actually completed. The release readiness runbook also includes
`set -a && source target/release-plugin-trust-qa.env && set +a &&
./scripts/release-plugin-trust-qa.sh --assert-complete` as the template-backed
assertion path.
`./scripts/release-evidence-bundle.sh --check` is the final evidence-bundle
preflight. Its `--self-test` validates bundle manifest generation with fake
artifacts and fake QA reports only, and locks in the exact template, source,
and `--bundle` commands printed by `--check`; real release evidence must come from
`--bundle` after signed/notarized distribution artifacts, signed-distribution
provenance, live-device QA, and plugin-trust QA evidence exist and every
`JARVIS_EVIDENCE_*` flag is true.
The `--check` output points operators to
`./scripts/release-evidence-bundle.sh --write-template
target/release-evidence-bundle.env` to generate a sourceable final-bundle
template. The template defaults every validation flag to `false`; operators
must flip each one only after the matching external release check is complete.
The bundle script validates `JARVIS_EVIDENCE_REPORTS_ARCHIVE_URI` even in its
fake self-test lane, and its self-test plus CLI E2E coverage reject temporary
archive paths and bare non-URI archive locations before a final bundle is
written.
It also defaults `JARVIS_EVIDENCE_OVERWRITE_OUTPUT=false`, so `--bundle`
fails instead of replacing an existing final bundle unless the operator has
preserved the old artifact and intentionally sets the overwrite flag to `true`.
The final bundle output path must also be distinct from the signed-provenance,
live-device QA, plugin-trust QA, app zip, and installer package input paths, so
the bundle writer cannot replace evidence it has just validated.
The release readiness runbook also includes
`set -a && source target/release-evidence-bundle.env && set +a &&
./scripts/release-evidence-bundle.sh --bundle` as the template-backed bundle
path.
`jarvis release readiness --all-commands` is ordered as a release execution
runbook: local gates, unsigned distribution launch check, signed-distribution
runbook triage, signed/notarized packaging, live-device QA, plugin-trust
runbook triage, plugin-trust QA, final evidence bundle generation,
evidence-doctor assertion, then the external evidence-mode readiness check.
`./scripts/release-external-handoff.sh --write target/release-external-handoff`
generates a single operator handoff directory with sourceable live-device,
plugin-trust, and final-bundle env templates plus read-only JSON snapshots for
readiness, evidence-status, and the three release runbooks. It also writes
`release-evidence-checklist.md`, which names the exact signed-distribution
artifact paths, live-device command and scheduler notification fields,
plugin-trust artifact URI/SHA-256 bindings, and final reports archive URI that
the release operator must fill before the final doctor assertion, plus
`release-handoff-manifest.json` with the release version, git commit, snapshot
endpoint, proof boundary, and per-file SHA-256 digests for the handoff package.
The handoff shell self-test verifies the expected manifest file list, release
version, current git commit, byte counts, and SHA-256 digests before passing.
The CLI E2E also checks that `release-evidence-status.json` preserves the same
completion state, missing evidence, invalid evidence, and evidence item rows as
a fresh external-mode direct CLI evidence-status query.
Its `--check` and `--self-test` modes are part of the local release gate and
prove only handoff generation mechanics; they do not perform signing,
notarization, stapling, installation, Finder launch, live-device QA,
plugin-trust QA, host-level egress enforcement, or final evidence archival.
`cargo run -p jarvis-cli -- release
signed-distribution-runbook` is read-only; it summarizes the current
signed-app-bundle, app executable, bundled core, signed zip, signed installer,
and signed-provenance evidence items and prints the exact package-distribution,
evidence-status, evidence-doctor, and live-device follow-up commands without
performing signing, notarization, stapling, Gatekeeper assessment, installation,
or QA. CLI E2E pins the exact signed-distribution evidence key set, command
sequence, manual checks, and `--json`/`--format json` parity for this runbook.
The same runbook families are also exposed through redacted IPC endpoints for
the Swift Release tab; Rust contract/runbook tests and Swift IPC/model tests
cover that app-facing surface without treating it as completed release
evidence. The CLI `--json` and `--format json` runbook output is the
operator/snapshot JSON shape used by release scripts and E2E handoff checks;
the IPC endpoints return the app-facing `ReleaseRunbookResponse` contract with
shared commands, manual checks, proof boundary, and evidence summaries, but the
two payloads are not required to be byte-for-byte identical.
`cargo run -p jarvis-cli -- release plugin-trust-runbook` is read-only;
it summarizes the current `plugin_trust_qa_report` evidence item and prints the
exact plugin-trust template, assertion, evidence-status, evidence-doctor, and
signed-distribution follow-up commands without performing marketplace review,
malware scanning, sandbox deployment, host-level egress enforcement, signing,
notarization, live-device QA, or final evidence bundling.
`cargo run -p jarvis-cli -- release evidence-bundle-runbook` is also read-only;
it summarizes the signed-distribution provenance, live-device QA,
plugin-trust QA, and final `release_evidence_bundle` rows, prints the
final-bundle template, bundle, evidence-doctor, external evidence-status, and
external readiness commands, and explicitly states that it does not generate
the final bundle or perform signing, notarization, installation, live-device
QA, marketplace review, malware scanning, sandbox deployment, or host-level
egress enforcement. The external handoff package now snapshots
`evidence-bundle-runbook.json` beside the signed-distribution, live-device, and
plugin-trust runbooks, and the manifest/self-test/E2E checks pin the runbook
snapshot command list and proof boundary.
The doctor/status paths are read-only inventory plus semantic validation for
expected paths, app bundle metadata, bundled-core marker metadata, JSON flags,
non-future report timestamps, signed-distribution provenance, artifact/report
digest bindings, final-bundle child-report semantic validity, owner-recorded
release evidence fields including durable archive URI validation, and release metadata;
they do not perform Developer ID signing, notarization, stapling, installation,
live-device QA, plugin-trust QA, owner assertions, final bundle creation, or
host-level egress enforcement. The real `--bundle` path also locally validates
the app code signature, app stapling ticket, installer package signature,
installer stapling ticket, and app zip payload before writing the manifest. It also requires the
`Jarvis-<version>-signed-provenance.json` report generated by the full
packaging lane, including signing identities, notary submission IDs/log paths
plus log SHA-256 digests, staple validation, Gatekeeper assessment, bundled
core version, and artifact digests that match the current zip/pkg files. It rejects disabled
local signature validation outside the fake self-test lane, parses every required
live-device/plugin-trust report flag, requires non-empty and non-placeholder
owner-recorded evidence-note fields in both QA reports and the final bundle,
requires live-device QA app bundle metadata to match the
expected installed app path plus bundle id/version, requires the observed
transcript to match the spoken test phrase, and writes SHA-256 digests for
distribution artifacts, signed provenance, and QA reports before writing
production evidence through a structured JSON writer so owner-recorded notes
with quotes, backslashes, or line breaks remain valid JSON. Owner evidence-note
validation rejects both exact
placeholders and embedded placeholder wording such as `TODO before release`,
`pending release evidence`, or fixture/example/self-test language.
`./scripts/release-evidence-doctor.sh --check` inventories the expected signed
artifact paths, signed-distribution provenance report, live-device QA report,
plugin-trust QA report, and final
evidence bundle manifest, then reports present, missing, or invalid evidence
without failing the local gate. It also validates the packaged app metadata and
bundled core version marker before counting the local app artifacts as
present. When evidence is missing it also prints the
next package preflight, both supported signing credential forms, external
handoff directory generator, live-device template/assertion, plugin-trust
template/assertion, and final-bundle template/bundle commands so operators can
move from inventory to evidence capture without cross-referencing another
checklist. Its
`--assert-complete` is included in the release-readiness runbook as the final
inventory assertion after `--bundle`. It requires the final bundle manifest to
include a non-future UTC generation timestamp, expected release version, non-empty
artifact/report paths matching the configured evidence paths,
SHA-256-shaped artifact/report digests matching current files, and
`validation_flags.local_signature_validation=true`, and it rejects final bundles
that reference semantically invalid signed-provenance, live-device QA, or
plugin-trust QA child reports even when their recorded digests match. This
also binds live-device QA back to the signed distribution by requiring
`bundled_core.sha256` to match signed-provenance
`artifacts.bundled_core_sha256`, requires the owner final-bundle completion time
to sit after all child report generation timestamps and before final bundle
generation, and rejects app zips that do not contain exactly one top-level
`Jarvis.app` payload with `Info.plist`, the app executable, and the bundled
core. This matches the semantic floor exposed by `/release/evidence-status`.
Its `--self-test` uses fake
artifacts/reports to prove the inventory logic only; it is not a signing,
notarization, stapling, clean-profile installation, live-device QA,
marketplace review, malware scan, OS sandbox, or host-level egress validator.
Plugin-trust evidence is timestamp-strict across the shell evidence path:
`release-plugin-trust-qa.sh --assert-complete` requires non-future UTC `Z` review
timestamps with `review_started_at <= review_completed_at`, and the
bundle/doctor paths also require plugin report `generated_at` to be UTC, non-future, and no
earlier than `review_completed_at`. Structured host egress evidence must also
include the policy/profile label, ordered UTC egress validation timestamp, and
deny/allow fixture notes. Each plugin-trust category must bind its archived
manual evidence artifact with a durable non-placeholder URI and SHA-256 digest;
evidence-status, evidence-doctor, and final bundle validation all reject
missing or invalid bindings across the full marketplace, malware, sandbox,
egress, signed-publisher, and manual-review artifact set.
Production plugin-trust reports must carry
`review_source=owner-asserted-manual-review`; imported reports and self-test
review sources are rejected before they can clear evidence-aware readiness.
`jarvis release evidence-status` exposes the same standard artifact/report
inventory through `/release/evidence-status`; the default CLI output is
operator-readable, includes per-item paths and details for present, missing,
or invalid evidence items when the structured payload provides them, and
`--json` preserves the exact structured payload. It
also rejects signed-provenance zip/pkg/notary-log digests that no longer match
the current artifact files and preserved notarytool logs, and rejects final evidence bundles with the wrong
`schema_version: 1` / `evidence_type: release_evidence_bundle` identity. It is
file/report inventory plus report semantic validation
only and does not prove signing, notarization, installed app launch, live-device QA, marketplace review,
malware scanning, OS sandboxing, or executable runtime behavior. Non-default live-device and plugin-trust
report paths can be provided through either the QA script variables
(`JARVIS_QA_REPORT_PATH`, `JARVIS_PLUGIN_QA_REPORT_PATH`) or the bundle/doctor
aliases (`JARVIS_EVIDENCE_LIVE_QA_REPORT`,
`JARVIS_EVIDENCE_PLUGIN_QA_REPORT`).

## Useful Focused Commands

These commands are targeted iteration checks. Use them to localize failures or
prove a narrow surface, but do not report them as the local release gate for
executable changes unless `./scripts/release-local.sh` also ran or the change is
explicitly docs-only.

```sh
cargo test -p jarvis-core
cargo test -p jarvis-core release_evidence_status -- --nocapture
cargo test -p jarvis-core permission_policy_review -- --nocapture
cargo test -p jarvis-core permission_policy_review_summarizes_unreviewed_memory_without_values -- --nocapture
cargo test -p jarvis-core memory_index -- --nocapture
cargo test -p jarvis-core local_memory_context -- --nocapture
cargo test -p jarvis-core stale_memory_index_blocks -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e reviewed_local_memory_context_is_bounded_redacted_and_fails_closed_cross_process -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter commandConsoleMemoryContextIsExplicitOptIn
./scripts/release-plugin-trust-qa.sh --self-test
./scripts/release-evidence-bundle.sh --self-test
./scripts/release-evidence-doctor.sh --self-test
cargo test -p jarvis-core diagnostics_export_is_redacted_and_counts_repository_state -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e app_supervised_ipc_auth_is_fail_closed_and_cli_token_file_is_safe -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter 'JarvisMacCoreTests.JarvisMacCoreTests/decodesDiagnosticsExport'
cargo test -p jarvis-core release_runbooks_expose_current_evidence_without_side_effects -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e release_evidence_bundle_runbook_summarizes_next_operator_steps -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e release_runbook_ipc_endpoints_emit_normalized_core_json -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e release_external_handoff_snapshots_match_live_runbook_commands -- --nocapture
cargo test -p jarvis-core scheduler_attention -- --nocapture
cargo test -p jarvis-core run_due_scheduler_jobs_executes_and_persists_visible_tasks -- --nocapture
cargo test -p jarvis-core run_due_scheduler_jobs_blocks_non_proactive_plugin_actions -- --nocapture
cargo test -p jarvis-core scheduler_proactive_policy_audit_matches_policy_review_classification -- --nocapture
cargo test -p jarvis-core detects_stale_running_jobs_in_oldest_first_order -- --nocapture
cargo test -p jarvis-core recover_stale_scheduler_jobs_marks_running_jobs_failed_and_audits_redacted -- --nocapture
cargo test -p jarvis-core automatic_stale_scheduler_recovery_marks_audit_without_command_text -- --nocapture
cargo test -p jarvis-core ollama_http_provider_parses_tool_request_envelope -- --nocapture
cargo test -p jarvis-core chatgpt_http_provider_parses_tool_request_envelope -- --nocapture
cargo test -p jarvis-core chatgpt_http_provider_parses_native_tool_calls -- --nocapture
cargo test -p jarvis-core request_supplied_first_party_tool_inventory -- --nocapture
cargo test -p jarvis-core model_request_advertises_registered_first_party_tools_only -- --nocapture
cargo test -p jarvis-core provider_tool_request_envelope_rejects_malformed_tool_requests_without_leaking_prompt -- --nocapture
cargo test -p jarvis-core provider_tool_request_envelope_rejects_mixed_prose_and_tool_json -- --nocapture
cargo test -p jarvis-core provider_originated_tool_request_executes_first_party_tool_and_feeds_result -- --nocapture
cargo test -p jarvis-cli serve_executes_chatgpt_native_tool_call --test local_ipc_e2e -- --nocapture
cargo test -p jarvis-core model_provider_failure_returns_failed_response_with_route_evidence -- --nocapture
cargo test -p jarvis-core command_schema_returns_failed_runtime_response_for_model_provider_error -- --nocapture
cargo test -p jarvis-core repository_backed_state_endpoints_expose_tasks_and_audit -- --nocapture
cargo test -p jarvis-core contract_endpoint_documents_safe_inspection_paths -- --nocapture
cargo test -p jarvis-core --test e2e_scaffold
cargo test -p jarvis-cli
cargo test -p jarvis-cli --test local_ipc_e2e
cargo test -p jarvis-cli --test local_ipc_e2e serve_can_recover_stale_scheduler_jobs_on_startup -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e serve_executes_ollama_provider_tool_request_envelope -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e serve_rejects_ollama_hallucinated_tool_with_registered_tool_guidance -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e serve_rejects_ollama_mixed_prose_tool_json_as_malformed_model_output -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e -- --ignored
./scripts/storage-migration-backup-smoke.sh
./scripts/release-operator-qa-smoke.sh
./scripts/packaged-supervision-proof.sh
./scripts/package-distribution.sh --check
./scripts/package-distribution.sh --running-app-guard-self-test
./scripts/package-distribution.sh --running-app-guard-e2e
./scripts/package-distribution.sh --unsigned-structure-check
./scripts/package-distribution.sh --unsigned-launch-check
swift test --package-path apps/mac --filter JarvisMacCoreTests
```

The non-ignored `local_ipc_e2e` test is the current cross-process E2E
expectation for Rust/CLI changes. The ignored variant includes the opt-in
release-proof smoke command and is run by `./scripts/release-local.sh`.
`./scripts/storage-migration-backup-smoke.sh` is the focused storage recovery
proof for migration changes: it runs Rust tests that create a legacy
file-backed DB, verify preflight backup creation, corrupt the DB after backup
to prove restore on migration-open failure, verify newer schema versions fail
with an explicit upgrade diagnostic, and migrate representative schema v1-v13
fixtures while preserving tasks, audit, memory, scheduler, approval, plugin,
and route rows.
`./scripts/release-operator-qa-smoke.sh` is the local operator-facing release
QA proof for CLI/repository surfaces: it starts a loopback core with an
isolated SQLite database, exercises command, audit, route inspection, memory
create/update/review/delete/restore, scheduler attention and due execution,
activity summary/watch, permission review, diagnostics, emergency
pause/block/resume, release readiness, and then restarts the core against the
same database to verify recovered memory, task, scheduler, and diagnostics
state. It is local operator QA evidence only, not a clean-profile installed-app
or live-device validation pass.
`./scripts/packaged-supervision-proof.sh` is the focused Swift/Rust bridge
proof for supervision changes: it builds `jarvis-cli`, copies it into a
temporary `Jarvis.app/Contents/Resources/bin/` layout, runs the Swift supervisor
coverage against that configured executable, starts the copied binary with a
repository-backed database, verifies packaged-layout health, command, audit,
diagnostics, emergency pause, blocked command, pause status, and resume
surfaces, and then runs the CLI smoke command.
`swift test --package-path apps/mac --filter JarvisMacCoreTests` is the focused
Swift contract/model proof for Mac app model changes, including scheduler
notification authorization, due/failed/emergency-pause-blocked request
creation, duplicate suppression, and denied-permission fail-closed behavior
through a fake adapter, plus voice permission-state sequencing that blocks
capture until microphone/Speech permissions are granted without claiming live
prompt or device validation, and Release tab runbook-load warning behavior
that keeps readiness/evidence visible when read-only runbook fetches fail.
`./scripts/package-distribution.sh` is the stricter distribution packaging
lane, and `--unsigned-launch-check` is now part of `./scripts/release-local.sh`
so release-built app layout regressions are caught by the default gate. Its
`--check` mode validates local tool availability plus app and bundled-core
entitlement templates without Apple credentials, `--check-guidance-self-test`
locks the signed-distribution, live-device, plugin-trust, final-bundle, and
doctor handoff commands printed by that preflight, and
`--entitlements-policy-self-test` proves microphone access stays on the app
entitlement template while the bundled core template omits it. The live-device handoff in that
preflight includes the release-core command evidence capture, the
`task:<uuid>`/`audit:<uuid>` evidence-ID recording rule, and endpoint-aware
external evidence-mode evidence-status/readiness checks before plugin-trust and
final bundle handoff. The final bundle handoff printed by `--check` ends with
`./scripts/release-evidence-doctor.sh --assert-complete` after the read-only
doctor inventory check, so the package preflight guidance carries the same
final assertion as the release evidence bundle and doctor lanes. Its
`--running-app-guard-self-test` locks the fail-closed parser and refusal message
used before any artifact-producing lane removes the distribution directory.
`--running-app-guard-e2e` additionally launches temporary harmless executables
at the exact app/core bundle paths, proves real process-name plus text-vnode
inspection blocks replacement, stops only those fixtures, and proves the bundle
is accepted afterward.
The same `--unsigned-launch-check` crash-style E2E sends `SIGKILL` to the
release-built app, requires its authenticated core to self-exit within a
bounded interval, verifies the UDS is removed, and then relaunches against the
same SQLite database. This is repository-owned proof that new app-supervised
cores do not survive an abrupt app loss; it does not terminate or claim control
over older/manual cores that were launched without the parent contract.
Packaging now refuses to replace the exact configured `Jarvis.app` while its
app or bundled-core executable is running, and tells the operator to quit the
app or select a different `JARVIS_DISTRIBUTION_DIR`. This prevents a surviving
process from observing a different on-disk signed bundle; it does not weaken
the runtime code-identity check or eliminate the narrow process-check race.
Its `--unsigned-structure-check` mode builds and inspects the release app/pkg
structure without Developer ID
credentials, including unsigned package identifier, version, and `/Applications`
install-location metadata. Its `--unsigned-launch-check` mode also validates
that package metadata, launches the release-built app executable with an
isolated temporary HOME, verifies the bundled core over the app-supervised
transport, and checks
command, audit, diagnostics, pause/block/resume, and SQLite state through the
release app layout. The packaged core is also checked
with `jarvis-cli --version`, and release evidence scripts reject bundles whose
core binary does not report the expected release version. Full mode requires
`JARVIS_DEVELOPER_ID_APPLICATION`,
`JARVIS_DEVELOPER_ID_INSTALLER`, and either a notarytool keychain profile or
Apple ID/team/app-specific-password notarytool credentials. It signs the
release bundle with hardened runtime and microphone entitlements, submits the
app zip for notarization, staples the app, then creates a signed
`/Applications` installer package at
`target/distribution/Jarvis-<release-version>.pkg`,
validates its package identifier, version, and `/Applications` install-location
metadata, checks its installer signature, submits it for notarization, and
staples the package. The signed-provenance report records exact notary
`Accepted` statuses, preserved notarytool log paths plus SHA-256 digests, not
only submission UUIDs. Passing the unsigned structure or launch checks still does not prove
signing/notarization, and passing full mode still does not replace
clean-profile install, Finder launch, live microphone/Speech validation, App
Store review, spoken transcript handoff, or live audio-output validation.
`./scripts/release-version-consistency.sh --check` derives that release version
from Rust package metadata and is part of `./scripts/release-local.sh`, so
package, live QA, evidence bundle, and evidence doctor defaults cannot silently
drift from the protocol/master/core/CLI crate and local dependency versions.
`./scripts/release-live-device-qa.sh --check` keeps the live-device QA runbook
in the default release gate. It validates the repo-owned entitlement/checklist
preconditions and prints the required clean-profile install, Finder launch,
microphone/Speech, spoken transcript handoff, live audio-output, notification,
restart, and manual QA steps. `--write-template target/release-live-device-qa.env`
writes a sourceable checklist for the release operator to fill on the validated
machine, with `JARVIS_QA_EXPECTED_VERSION` materialized from the canonical
Rust package release version at generation time. The template also includes a
single sourceable `JARVIS_RELEASE_CORE_ENDPOINT` value, the app-owned
`JARVIS_IPC_TOKEN_FILE` path (never the bearer value), the release-core
`jarvis command ... --json` evidence capture that reuses that endpoint, the
`JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:<uuid>"`/`"audit:<uuid>"` rule, and
the external evidence-mode `release evidence-status` and `release readiness`
checks to run against the same endpoint after report generation. The CLI/IPC
live-device runbook, package preflight, and evidence-doctor next-step guidance
mirror that handoff so operators see the release-core command capture and
endpoint-aware external evidence-mode commands before report generation.
The signed-distribution and plugin-trust runbooks reuse the same guarded
`JARVIS_RELEASE_CORE_ENDPOINT` evidence-status command before handoff continues
to doctor checks or final bundling, so copied operator commands do not silently
inspect a different running core.
The generated external handoff README uses the same guarded
`JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external` plus
`JARVIS_RELEASE_CORE_ENDPOINT` commands for the final evidence-status and
readiness queries, so the one-shot operator handoff cannot fall back to a
placeholder endpoint during the final proof step.
`--assert-complete` is for the release machine after those checks are
actually performed and all required `JARVIS_QA_*` flags are explicitly set to
`true`, including
`JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true` for the spoken
transcript handoff into the same command path. The assertion command also
requires ordered UTC timestamps plus non-empty owner-recorded evidence fields:
`JARVIS_QA_OWNER_NAME`, `JARVIS_QA_DEVICE_LABEL`, `JARVIS_QA_PROFILE_LABEL`,
`JARVIS_QA_VOICE_CHECK_STARTED_AT`, `JARVIS_QA_VOICE_CHECK_COMPLETED_AT`,
`JARVIS_QA_MICROPHONE_EVIDENCE_NOTE`,
`JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE`,
`JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE`, and
`JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE`. It also requires structured
spoken-command observation fields: `JARVIS_QA_VOICE_TEST_PHRASE`,
`JARVIS_QA_OBSERVED_TRANSCRIPT`, `JARVIS_QA_EXPECTED_COMMAND_TEXT`,
`JARVIS_QA_OBSERVED_COMMAND_TEXT`, `JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID`,
`JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL`, and structured notification observation
fields: `JARVIS_QA_NOTIFICATION_KIND` (`due_now`, `failed`, or
`blocked_by_emergency_pause`), `JARVIS_QA_NOTIFICATION_TITLE`,
`JARVIS_QA_NOTIFICATION_BODY`, and
`JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER=jarvis.scheduler`.
All owner-recorded evidence-note fields must contain non-placeholder text, not
values such as `TODO`, `pending`, `n/a`, `fixture`, or `self-test fixture`, and
`JARVIS_QA_SELF_TEST_FIXTURE=true` is reserved for the script's internal fake
fixture self-test rather than release evidence. `--assert-complete`,
`jarvis release evidence-status`, and `/release/evidence-status` enforce the
same non-empty and non-placeholder live-device QA report fields before that
evidence can clear `live_voice_loop`. CLI E2E removes required live voice,
command-observation, audio-output-device, and notification-observation fields
from owner-recorded reports and verifies evidence-status plus external-mode
readiness fail closed.
CLI E2E also runs `release-live-device-qa.sh --assert-complete` with a
repository-backed command result, verifies the script-generated live-device QA
report is accepted by `jarvis release evidence-status`, and confirms
external-mode readiness moves `live_voice_loop` to implemented while production
readiness stays blocked by the remaining signed distribution and final evidence
bundle gates.
The assertion consumes `JARVIS_QA_SIGNED_PROVENANCE_REPORT` (defaulting to the
canonical signed-provenance output), verifies the installed app executable with
codesign, stapler, and Gatekeeper, and requires its SHA-256, code Identifier,
TeamIdentifier, and CDHash to match signed provenance. The JSON report records
those fields plus the signed-provenance path/SHA-256. Final bundle, doctor, and
Rust/CLI validators reject report substitution or executable identity drift.
The observed transcript must match the spoken test phrase after trimming, the
expected installed app path must match `JARVIS_QA_INSTALLED_APP_PATH` or
`/Applications/Jarvis.app`, expected and observed command text must match after
trimming, `JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID` must be `task:<uuid>` or
`audit:<uuid>` from live command/audit evidence, and repository-backed
`/release/evidence-status` must resolve it to an existing task or
task-associated audit row before it can clear readiness. `generated_at` must be
UTC and no earlier than the completed voice check, but not future-dated.
The notification observation timestamp must match
`JARVIS_QA_NOTIFICATION_OBSERVED_AT`, must be UTC, and must be no earlier than
`JARVIS_QA_VOICE_CHECK_STARTED_AT`. The shell assertion self-test,
`jarvis release evidence-status`, and final bundle validation reject blank
notification title/body values, notification kinds outside `due_now`, `failed`,
or `blocked_by_emergency_pause`, and any notification thread identifier other
than `jarvis.scheduler`.
On success, `--assert-complete` writes a JSON evidence report to
`JARVIS_QA_REPORT_PATH` or `target/release-live-device-qa-report.json` by
default. The report includes installed-app metadata, app microphone/Speech usage
descriptions, bundled-core path/version/digest evidence, all required
validation flags, voice-loop evidence fields, owner/device/profile/timestamp
and live voice/non-voice evidence-note fields, structured command observation
including `audio_output_device_label`, structured notification observation,
schema identity, and the proof boundary.
Focused cross-report E2E is:

```sh
cargo test -p jarvis-cli --test local_ipc_e2e release_evidence_status_rejects_live_app_executable_digest_mismatch -- --nocapture
```

This proves fail-closed report binding with fixtures and asserts executable
drift keeps both the live-device evidence item invalid and `live_voice_loop`
pending even in external evidence mode; it does not perform real Developer ID
signing, installation, or live-device QA.
The installed app metadata must match the approved `Info.plist` copy exactly:
`NSMicrophoneUsageDescription` is `Jarvis uses microphone input only when you explicitly start local voice capture.`, and
`NSSpeechRecognitionUsageDescription` is `Jarvis uses speech recognition only to turn your spoken command into a local assistant request.`.
After generating it, run `cargo run -p jarvis-cli -- release evidence-status`
and `cargo run -p jarvis-cli -- release readiness` against a core started or
restarted with `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external`
to confirm any live voice/audio blocker changes are backed by the report.
The final `./scripts/release-evidence-doctor.sh --assert-complete` assertion
also calls `jarvis release evidence-status --json`; set
`JARVIS_EVIDENCE_STATUS_ENDPOINT` to the release core endpoint when proving
repository-backed task/audit evidence during final release evidence review.
Preserve that report with release notes when making a production-ready claim.
`--self-test` uses a fake app fixture to exercise only the assertion/report
mechanics and is included in `./scripts/release-local.sh`.
Docs-only branches should at least run a render/lint-oriented documentation
check when available, plus `cargo fmt --check` if the branch also touches Rust
examples or scripts. Record any skipped full-gate stage as a blocker, not as
implicit coverage.

For documentation-only production-sweep slices, use focused repository checks
that prove the required docs and diagrams are still present:

```sh
rg -n "Current Implementation And Evidence Boundary Diagram|End-Goal Production Architecture" docs/architecture-map.md
rg -n "phase-3|phase3|six-agent|worktree|E2E|production-readiness|proof" docs/knowledge-base/jarvis-project-facts.md docs/release-checklist.md docs/build-test-commands.md README.md
git diff --check
```

These checks do not replace `./scripts/release-local.sh` for executable
changes. They only support docs-only PR evidence and should be reported as such.
For the phase-3 docs architecture slice, the expected verification is the two
`rg` checks above plus `git diff --check`; code gates belong to the executable
phase-3 slices unless this branch starts changing code.

Post-merge cleanup audit:

```sh
gh pr list --state open --json number,title,headRefName,baseRefName,url
gh run list --workflow release-local.yml --branch main --limit 5
git worktree list --porcelain
git branch --merged main --list 'codex/*'
git branch --no-merged main --list 'codex/*'
git status --short --branch
```

Use this after merging PR slices to confirm no review PRs remain open, public
release-local workflow evidence exists for `main`, active worktrees are known,
merged `codex/` topic branches are distinguishable from unmerged historical
lanes, and the primary checkout is clean before making readiness statements.

For installed subprocess plugin progress-event changes, run the focused
redaction/audit test before the full release gate:

```sh
cargo test -p jarvis-core installed_plugin_runner_records_subprocess_progress_events_without_raw_stderr -- --nocapture
```

For installed subprocess lifecycle or cancellation changes, run the bounded
process-group unit lane and the authenticated approved-execution E2E before the
full release gate:

```sh
cargo test -p jarvis-core local_subprocess_ -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e authenticated_approved_installed_execution_can_be_cancelled_after_claim -- --nocapture
```

The unit lane covers normal bounded output, output-limit cleanup while stdin is
blocked, emergency pause, cancellation, TERM-ignoring in-group descendants,
group KILL, and leader reaping. The authenticated E2E waits for an in-group
descendant heartbeat, cancels the exact active approval handle, proves that
fixture stops before the manifest timeout, persists effect-possible/non-retryable
cancellation, and rejects replay after restart. It does not prove effect
rollback, an OS sandbox, containment of deliberate `setsid`/`setpgid`
process-group escape, host-level egress enforcement, publisher trust, or
live-device behavior.

## Release Evidence Boundary

Passing `./scripts/release-local.sh` proves the current Rust workspace builds,
passes standard and ignored release-proof tests, runs the CLI smoke command,
packages the Rust crates, runs local release/runbook preflights and fake-fixture
evidence self-tests, and passes the Swift package build/test gate. The
cross-process IPC E2E test proves the local server and CLI can exchange JSON for
health, runtime-backed command execution, deterministic first-party plugin
execution, route/policy/plugin audit evidence, approval-required persistence and
grant/deny decisions, scheduler schedule/cancel and persistence, redacted
diagnostics export, memory classification summary fields,
redacted memory retention-plan fields,
memory create/update/review/delete/restore and persistence, plugin
manifests, installed-plugin source-tree provenance verification, permission-grant
provenance summary fields, operator-pinned publisher-origin verification,
trusted-key publisher-signature verification,
network-capable plugin policy review items,
permission policy review items, redacted scheduler trigger review items, fail-closed
subprocess enablement, installed subprocess minimal environment isolation,
installed subprocess stdout/stderr byte limits, installed subprocess
output-limit fail-closed behavior through the CLI IPC path, installed subprocess
process-group cancellation and in-group descendant cessation through
authenticated IPC,
non-retryable one-shot cancellation across restart, installed subprocess
progress-frame response/audit redaction, repository-backed activity summary status,
redacted recent task metadata, recent-audit evidence without recent task command
bodies, bounded activity event streaming over server-sent events, redacted
scheduler attention handoff, Swift bounded activity event parsing/model
coverage, scheduler due-job
execution/reschedule audit evidence, redacted proactive scheduler policy
audit evidence before due command submission, explicit and opt-in startup
stale-running scheduler recovery after persisted running jobs survive restart,
scheduler fail-closed
emergency pause on non-accepted due jobs, and emergency-pause blocking/resume
surfaces.
Runtime unit tests additionally prove bounded fake-model and provider-envelope
first-party tool-call orchestration plus default-off reactive-local installed
WASM planning, including policy checks, approval stops, validation failures,
route-scoped provider-visible inventory, and tool-result feedback into later
model steps. Focused provider tests prove
typed Ollama-compatible request/error behavior, strict
Ollama-compatible and ChatGPT/OpenAI-compatible response-envelope parsing,
malformed envelope redaction, and structured failed command responses for
selected model-provider failures without requiring a live model during the
default release gate. The cross-process local IPC E2E includes an
Ollama-compatible stub that emits provider tool-request envelopes and proves
the runtime advertises registered first-party tools, and, only with the
per-command opt-in, eligible installed WASM schemas; executes the selected
tool; revalidates installed state and exact provenance before guest entry; and
returns the provider's final message. That E2E also proves default-off,
subprocess exclusion, mutation denial, and redaction. Runtime unit coverage
proves cloud/proactive exclusion; the installed-WASM confinement suite retains
disabled, pause, cancellation, timeout, and budget fail-closed proof. They do not prove
live ChatGPT service execution,
advanced memory classification policy beyond the current summary surface, live
microphone capture, or live audio output until those surfaces are manually
validated. The current Swift gate proves the
Mac shell builds, decodes IPC contracts, decodes live CLI fallback JSON
for release readiness, release evidence-status, and all three release runbooks,
decodes release runbook payloads, requests the three runbook IPC endpoints,
refreshes Release tab runbook state including runbook-load warnings, preflights
direct macOS Speech/microphone authorization before concrete voice capture,
shows release evidence status rows with explicit path, detail, and
production/manual-gate context,
shows Model tab streamed Ollama download progress, automatic inventory reload,
`:latest` installed-model alias handling, and Start/Download gating through
focused model and app presentation tests, including normalized update-required
pull failures plus confirmed loopback-only Homebrew upgrade command sequencing,
minimal-environment enforcement, version verification, and no-start behavior for
an already stopped service. A process-level E2E additionally drives the model
through a real Foundation child process backed by a temporary fake Homebrew
executable and verifies the resulting version file, restart sentinel, and exact
command log without changing the host installation,
exposes management models for
approval evidence, memory classification summary, memory policy review counts,
memory create/update/review/delete/restore state, runs/audit,
activity summary, permission policy review, scheduler attention summaries,
diagnostics, release readiness, contract compatibility policy and feature proof/boundary metadata,
text-transcript voice handoff state, opt-in final-transcript auto-submit through
fake adapter final-result events, adapter-backed voice input controls,
adapter-backed speech-output preview controls, adapter-backed scheduler
notification controls, and
Keychain-backed supervised-core credential injection
without requiring live microphone access, live audio output, or real credentials
or live OS notification delivery in tests. It can supervise a configured local core process
abstraction. It also covers Swift approval decision calls against the Rust IPC
approval endpoints. The packaged supervision proof additionally checks the
expected `Resources/bin/jarvis-cli` bundle layout with a locally built core
binary and exercises repository-backed command, audit, diagnostics, and
emergency-pause IPC through that copied binary. The unsigned distribution
launch check assembles the release app layout, creates an unsigned installer
payload, launches the app executable with isolated profile state, and verifies
app-supervised core IPC through the bundled `jarvis-cli`. The deprecated
`packaged-app-release-smoke.sh` wrapper delegates to that command. These gates
still do not prove Developer ID signing,
notarization, installer behavior, entitlement validation, Finder/LaunchServices
launch, microphone permissions, live speech-to-text, or live audio-output
behavior unless the stricter distribution lane and manual checks named in the
release checklist are also completed. The live-device QA script standardizes
that final manual evidence but does not create live microphone, Speech,
notification, or audio-output proof when run in `--check` mode. A valid
`--assert-complete` report can clear the live voice/audio readiness blocker
only when evidence-aware readiness mode is explicitly enabled; `--self-test`
and stale default `target/` reports must not be treated as production evidence.

The public-repo production workflow expects isolated worktrees, topic branches,
reviewable PRs, and clear ownership. A six-agent autonomous sweep can reduce
elapsed time, but readiness claims still depend on checked-in implementation
and the verification commands above. Each feature phase should name the E2E or
focused integration coverage it relies on; when a phase changes behavior and no
such coverage exists, adding coverage is part of the phase rather than a
follow-up readiness claim.

### Native menu-bar presence

The Swift shell includes a native `MenuBarExtra` that shares the existing
`JarvisCoreSupervisor`, command console, and model configuration state. Its
stable `jarvis-main` scene route reopens the existing window instead of
creating a second shell or core owner. Run the Swift package contract gate:

```bash
swift test --package-path apps/mac
```

`JarvisMacAppTests` pins the scene route and maps stopped, starting, available,
and degraded supervisor states to conservative menu status and enabled-action
behavior. This is Swift scene/model contract coverage. It does not prove macOS
menu rendering, window reopening through Finder/LaunchServices, or live
lifecycle actions in a signed installed app; record those in manual release QA.

### Trusted system-wake foundation

```bash
cargo test -p jarvis-core trusted_wake -- --nocapture
cargo test -p jarvis-cli --test local_ipc_e2e trusted_wake -- --nocapture
swift test --disable-sandbox --package-path apps/mac --filter TrustedWake
```

The focused lanes include signed rotate and destructive recover cross-process
E2E, legacy-bootstrap bypass rejection, wrong-key/token-replay/old-signature
rejection, one-shot disabled install, grant expiry and quarantine, Swift
Keychain journal reconciliation, near-expiry preservation of a healthy core,
and supervisor lifecycle serialization. The concurrent-provision fixtures use
an async bounded readiness wait so they do not block the main actor while the
detached bootstrap provider is scheduled on a loaded runner. The CLI exposes read-only inspection
through `system-wake status`, prepare through `system-wake key-prepare`, and
cancel/reset through `system-wake key-cancel`; supervised install/resume uses
`serve --trusted-wake-key-control-stdin` with bounded stdin and EOF.
`key-prepare` requires `--document-stdin` and accepts the entire JSON document
through a maximum 8192-byte stdin payload; it exposes no proof, key,
confirmation, or token argv options. Its JSON response contains a short-lived
one-time `grant_token` secret. The supported app path immediately persists the
grant in its device-only Keychain journal, constructs the distinct install
document, and passes that document through supervised stdin. A manual caller
must equivalently use a trusted in-memory or device-only journal transformer to
construct the install document; the raw prepare response is not install stdin.
Never display it in a terminal or place prepare/proof/token material in shell
history, logs, command arguments, or intermediate files.

These prove bounded local contracts only. Recovery confirmation is accident
prevention after bearer possession in the packaged app (or on an explicitly
unauthenticated legacy server), not device or OS-identity authentication;
the tests do not prove Apple attestation, OS wake provenance, background
launch, same-user/process isolation, exactly-once effects, or live QA.
