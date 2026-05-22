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
./scripts/release-operator-qa-smoke.sh
cargo package --workspace --allow-dirty
./scripts/package-distribution.sh --unsigned-launch-check
./scripts/release-live-device-qa.sh --check
./scripts/release-live-device-qa.sh --self-test
./scripts/release-plugin-trust-qa.sh --check
./scripts/release-plugin-trust-qa.sh --self-test
./scripts/release-evidence-bundle.sh --check
./scripts/release-evidence-bundle.sh --self-test
./scripts/release-evidence-doctor.sh --check
./scripts/release-evidence-doctor.sh --self-test
swift test --package-path apps/mac
swift build --package-path apps/mac
```

## Current Health Check

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
conservative local summary when the endpoint is unavailable:

```sh
cargo run -p jarvis-cli -- release readiness
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

Live local testing with `llama3.2` has proven this Ollama route can complete
real model commands. Local model behavior is still model-dependent, so the
runtime advertises the exact first-party tool inventory in the prompt and
keeps validating every model-planned tool request before execution.

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

If the selected local or ChatGPT/OpenAI-compatible provider fails during
execution, `/commands` now returns a normal failed command response with
redacted `model_step_failed` audit evidence and route evidence instead of an
IPC transport error.

Ollama-compatible and ChatGPT/OpenAI-compatible text responses may also return
a strict JSON envelope with `message`, `complete`, and `tool_requests`.
Accepted `tool_requests` are fed into the same bounded first-party schema,
policy, approval, and audit path as fake-model tool plans; malformed envelopes
fail with redacted diagnostics. Unknown plugin IDs, undeclared actions, and
malformed inputs fail closed before policy check or tool execution, emit
`tool_request_rejected` audit evidence, and are returned to the model as
`rejected` tool results for bounded recovery. Oversized tool plans still fail
the task. ChatGPT/OpenAI-compatible responses may also return native OpenAI
`tool_calls` for the advertised first-party tools; these are translated into
the same bounded first-party path. This is provider tool compatibility, not
installed-plugin orchestration.

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
cargo run -p jarvis-cli -- plugins list
cargo run -p jarvis-cli -- release readiness
cargo run -p jarvis-cli -- diagnostics export
cargo run -p jarvis-cli -- permissions grants
cargo run -p jarvis-cli -- activity summary
cargo run -p jarvis-cli -- activity watch --max-events 2 --interval-ms 500
cargo run -p jarvis-cli -- scheduler list
cargo run -p jarvis-cli -- scheduler schedule "manual check" "status check"
cargo run -p jarvis-cli -- scheduler schedule "approval fail closed" "plugin approval echo scheduler pause"
cargo run -p jarvis-cli -- scheduler run-due --limit 1
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
the side effect; approved first-party actions require an explicit
`jarvis approvals execute <approval-id>` replay, which verifies the original
action and scope contract before recording `approval_executed` audit evidence.
`jarvis permissions grants` reads the combined local grant
surface: approval counts/history, high-risk pending count, installed-plugin
`metadata_only` grant records, and the invariant that side effects still
require approval. Installed `local_subprocess` plugins remain disabled by
default and execute only after an explicit `subprocess_stdio` grant through the
constrained JSON stdin/stdout runner. The Swift Plugin tab reads the same
installed registry records for read-only provenance/grant review, and its model
keeps first-party manifests visible if the repository-backed registry endpoint
is unavailable. The Swift app now exposes the
Speech/AVFoundation input adapter controls and AVFoundation speech-output
preview controls, but release claims for real voice still require entitlement
packaging, live microphone checks, live audio-output checks, and manual device
validation. Swift approval decision, approved-run, plugin registry, and voice
controls are covered by the Swift contract/model tests.
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
after provenance matches; this verifies the manifest signature against the
explicit trusted key, but still does not prove marketplace approval or malware
safety.
Network-capable plugin actions must request `network` and declare
`network_access.mode: declared_hosts` with exact plain-hostname
`allowed_hosts`; policy review surfaces them as `network_plugin_action` items,
and executable installed plugins with those actions require
`subprocess_stdio_network`. This is runtime grant gating plus manifest
governance, not OS-level network sandboxing or host-level egress filtering.
Installed subprocess plugins can also emit bounded `jarvis_progress` stderr
JSON frames. Jarvis exposes only parsed sequence/stage/message progress events
and `installed_plugin_progress` audit entries, plus redacted
`activity_progress` frames on `/activity/events`; raw stderr remains redacted.
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
cargo run -p jarvis-cli -- approvals execute <approval-id>
cargo run -p jarvis-cli -- approvals deny <approval-id> --decided-by cli --reason "not safe"
cargo run -p jarvis-cli -- release readiness
cargo run -p jarvis-cli -- permissions review
cargo run -p jarvis-cli -- activity summary
cargo run -p jarvis-cli -- activity watch --max-events 2 --interval-ms 500
cargo run -p jarvis-cli -- plugins verify-publisher <plugin-id> --trusted-origin "<manifest author>" --decided-by cli
cargo run -p jarvis-cli -- plugins verify-publisher-signature <plugin-id> --trusted-public-key "<base64 ed25519 public key>" --decided-by cli
cargo run -p jarvis-cli -- memory list
cargo run -p jarvis-cli -- memory classification --include-deleted
cargo run -p jarvis-cli -- memory create workflow release-gate "run local gate before PR" --provenance "manual note" --sensitivity workspace
cargo run -p jarvis-cli -- memory restore <memory-id>
cargo run -p jarvis-cli -- scheduler attention
cargo run -p jarvis-cli -- diagnostics export
```

`jarvis permissions review` includes pending approvals, plugin review items,
active scheduler triggers, unreviewed memory items, and deleted sensitive
memory retained in local storage. Memory review items include category/key and
sensitivity only; memory values stay out of policy review and diagnostics
export. `jarvis diagnostics export` exposes aggregate active, unreviewed, and
sensitive memory counts when repository backing is enabled.
`jarvis release readiness` is read-only and summarizes implemented feature
proofs, pending feature boundaries, recommended verification commands, and
manual production blockers with `production_ready: false` until the external
distribution and live-device gates are completed.
`./scripts/release-plugin-trust-qa.sh --check` is the local plugin-trust
preflight for marketplace review, malware scanning, signed publisher policy,
OS sandbox, and host-level egress validation. Its `--self-test` mode uses fake
flags to verify report generation only; real release evidence must come from
`--assert-complete` after the owner validates every `JARVIS_PLUGIN_QA_*` flag.
`./scripts/release-evidence-bundle.sh --check` is the final evidence-bundle
preflight. Its `--self-test` validates bundle manifest generation with fake
artifacts and fake QA reports only; real release evidence must come from
`--bundle` after signed/notarized distribution artifacts, live-device QA, and
plugin-trust QA evidence exist and every `JARVIS_EVIDENCE_*` flag is true.
The real bundle path also locally validates the signed app, app stapling
ticket, signed installer package, installer stapling ticket, and app zip
payload before writing the manifest. It rejects disabled local signature
validation outside the fake self-test lane, parses every required
live-device/plugin-trust report flag, requires live-device QA app bundle
metadata to match the expected bundle id/version, and writes SHA-256 digests
for distribution artifacts and QA reports before writing production evidence.
`./scripts/release-evidence-doctor.sh --check` inventories the expected signed
artifact paths, live-device QA report, plugin-trust QA report, and final
evidence bundle manifest, then reports present and missing evidence without
failing the local gate. Its `--self-test` uses fake artifacts/reports to prove
the inventory logic only; `--assert-complete` is reserved for release triage
after real external evidence exists.

## Useful Focused Commands

```sh
cargo test -p jarvis-core
cargo test -p jarvis-core permission_policy_review -- --nocapture
cargo test -p jarvis-core permission_policy_review_summarizes_unreviewed_memory_without_values -- --nocapture
cargo test -p jarvis-core diagnostics_export_is_redacted_and_counts_repository_state -- --nocapture
cargo test -p jarvis-core scheduler_attention -- --nocapture
cargo test -p jarvis-core run_due_scheduler_jobs_executes_and_persists_visible_tasks -- --nocapture
cargo test -p jarvis-core scheduler_proactive_policy_audit_matches_policy_review_classification -- --nocapture
cargo test -p jarvis-core detects_stale_running_jobs_in_oldest_first_order -- --nocapture
cargo test -p jarvis-core recover_stale_scheduler_jobs_marks_running_jobs_failed_and_audits_redacted -- --nocapture
cargo test -p jarvis-core automatic_stale_scheduler_recovery_marks_audit_without_command_text -- --nocapture
cargo test -p jarvis-core ollama_http_provider_parses_tool_request_envelope -- --nocapture
cargo test -p jarvis-core chatgpt_http_provider_parses_tool_request_envelope -- --nocapture
cargo test -p jarvis-core chatgpt_http_provider_parses_native_tool_calls -- --nocapture
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
./scripts/packaged-app-release-smoke.sh
./scripts/package-distribution.sh --check
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
to prove restore on migration-open failure, and verify newer schema versions
fail with an explicit upgrade diagnostic.
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
`./scripts/packaged-app-release-smoke.sh` is the stronger local packaged app
proof for packaging/release changes: it builds the Swift app executable,
assembles a deterministic `Jarvis.app`, writes `Info.plist`, bundles
`jarvis-cli` in `Contents/Resources/bin/`, ad-hoc signs with `codesign -` when
available using `packaging/Jarvis.entitlements`, verifies microphone usage
strings and the `com.apple.security.device.audio-input` entitlement, launches
the app executable under a temporary HOME/profile with an isolated endpoint and
database path, and verifies app-supervised core health, command, audit,
diagnostics, emergency pause, blocked command, pause status, resume, and
temp-profile SQLite state. It is still local evidence only, not Developer ID
signing, notarization, installer validation, App Store distribution,
Finder/LaunchServices validation, or real microphone/Speech/live audio-output
coverage.
`swift test --package-path apps/mac --filter JarvisMacCoreTests` is the focused
Swift contract/model proof for Mac app model changes, including scheduler
notification authorization, due/failed request creation, duplicate suppression,
and denied-permission fail-closed behavior through a fake adapter.
`./scripts/package-distribution.sh` is the stricter distribution packaging
lane, and `--unsigned-launch-check` is now part of `./scripts/release-local.sh`
so release-built app layout regressions are caught by the default gate. Its
`--check` mode validates local tool availability and the entitlements
template without Apple credentials. Its `--unsigned-structure-check` mode
builds and inspects the release app/pkg structure without Developer ID
credentials. Its `--unsigned-launch-check` mode also launches the release-built
app executable with an isolated temporary HOME, verifies the bundled core over
loopback IPC, and checks command, audit, diagnostics, pause/block/resume, and
SQLite state through the release app layout. Full mode requires
`JARVIS_DEVELOPER_ID_APPLICATION`,
`JARVIS_DEVELOPER_ID_INSTALLER`, and notarytool credentials. It signs the
release bundle with hardened runtime and microphone entitlements, submits the
app zip for notarization, staples the app, then creates a signed
`/Applications` installer package at `target/distribution/Jarvis-0.1.4.pkg`,
checks its installer signature, submits it for notarization, and staples the
package. Passing the unsigned structure or launch checks still does not prove
signing/notarization, and passing full mode still does not replace
clean-profile install, Finder launch, live microphone/Speech validation, App
Store review, or live audio-output validation.
`./scripts/release-live-device-qa.sh --check` keeps the live-device QA runbook
in the default release gate. It validates the repo-owned entitlement/checklist
preconditions and prints the required clean-profile install, Finder launch,
microphone/Speech, live audio-output, notification, restart, and manual QA
steps. `--assert-complete` is for the release machine after those checks are
actually performed and all required `JARVIS_QA_*` flags are explicitly set to
`true`.
On success, `--assert-complete` writes a JSON evidence report to
`JARVIS_QA_REPORT_PATH` or `target/release-live-device-qa-report.json` by
default. Preserve that report with release notes when making a production-ready
claim. `--self-test` uses a fake app fixture to exercise only the
assertion/report mechanics and is included in `./scripts/release-local.sh`.
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

For installed subprocess plugin progress-event changes, run the focused
redaction/audit test before the full release gate:

```sh
cargo test -p jarvis-core installed_plugin_runner_records_subprocess_progress_events_without_raw_stderr -- --nocapture
```

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
manifests, installed-plugin source-tree provenance verification, permission-grant
provenance summary fields, operator-pinned publisher-origin verification,
trusted-key publisher-signature verification,
network-capable plugin policy review items,
permission policy review items, redacted scheduler trigger review items, fail-closed
subprocess enablement, installed subprocess minimal environment isolation,
installed subprocess stdout/stderr byte limits, installed subprocess
output-limit fail-closed behavior through the CLI IPC path, installed subprocess
progress-frame response/audit redaction, repository-backed activity summary status/recent-audit
evidence, bounded activity event streaming over server-sent events, redacted
scheduler attention handoff, Swift bounded activity event parsing/model
coverage, scheduler due-job
execution/reschedule audit evidence, redacted proactive scheduler policy
audit evidence before due command submission, explicit and opt-in startup
stale-running scheduler recovery after persisted running jobs survive restart,
scheduler fail-closed
emergency pause on non-accepted due jobs, and emergency-pause blocking/resume
surfaces.
Runtime unit tests additionally prove bounded fake-model and provider-envelope
first-party tool-call orchestration, including policy checks, approval stops,
validation failures, and tool-result feedback into later model steps. Focused
provider tests prove typed Ollama-compatible request/error behavior, strict
Ollama-compatible and ChatGPT/OpenAI-compatible response-envelope parsing,
malformed envelope redaction, and structured failed command responses for
selected model-provider failures without requiring a live model during the
default release gate. The cross-process local IPC E2E includes an
Ollama-compatible stub that emits a provider tool-request envelope and proves
the runtime executes the first-party tool before returning the provider's final
message. They do not prove live ChatGPT service execution,
advanced memory classification policy beyond the current summary surface, live
microphone capture, or live audio output until those surfaces are manually
validated. The current Swift gate proves the
Mac shell scaffold builds, decodes IPC contracts, exposes management models for
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
emergency-pause IPC through that copied binary. The packaged app release smoke
assembles and ad-hoc signs a local `Jarvis.app`, launches it with isolated
profile state, and verifies app-supervised core IPC through the bundled
`jarvis-cli`. These gates still do not prove Developer ID signing,
notarization, installer behavior, entitlement validation, Finder/LaunchServices
launch, microphone permissions, live speech-to-text, or live audio-output
behavior unless the stricter distribution lane and manual checks named in the
release checklist are also completed. The live-device QA script standardizes
that final manual evidence but does not create live microphone, Speech,
notification, or audio-output proof when run in `--check` mode.

The public-repo production workflow expects isolated worktrees, topic branches,
reviewable PRs, and clear ownership. A six-agent autonomous sweep can reduce
elapsed time, but readiness claims still depend on checked-in implementation
and the verification commands above. Each feature phase should name the E2E or
focused integration coverage it relies on; when a phase changes behavior and no
such coverage exists, adding coverage is part of the phase rather than a
follow-up readiness claim.
