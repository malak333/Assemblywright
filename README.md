# Jarvis

Jarvis is a local-first macOS assistant foundation. The current repo implements
the Rust core described in [DESIGN.md](DESIGN.md): durable task/audit
primitives, policy-gated first-party plugin commands, bounded model-planned
first-party tool orchestration, strict provider response envelopes for
first-party tool requests, local-first model routing evidence, opt-in
Ollama-compatible local HTTP and ChatGPT/OpenAI-compatible provider boundaries,
bounded Ollama-native NDJSON transport streaming with terminal-frame
quarantine and in-flight cancellation,
structured provider-failure responses with route/audit evidence, plugin
contracts, metadata-only local plugin installation, local plugin
provenance snapshots, scheduler state, redacted diagnostics export, a loopback
IPC surface with compatibility policy plus feature proof/boundary metadata,
repository-backed activity summary and activity event stream, conservative
release-readiness inspection, read-only release runbook IPC surfaces, and CLI smoke paths for the Swift shell
and local packaged app proof.

Trusted macOS system-wake events are a disabled-by-default local foundation.
Swift stores a P-256 private key and monotonic counter in device-only Keychain
items. Normal startup does not touch that Keychain material; an explicit
Provision action prepares only the public key while the current supervised core
keeps running, then performs one bounded-stdin restart. Swift signs
session/challenge/generation-bound wake payloads. Counter allocation also
advances past Rust's durable replay high-water to recover safely from Keychain
counter loss or a backward wall clock. Rust schema v11 persists
replay and dispatch state before using the existing proactive scheduler,
policy, plugin, and emergency-pause funnel. This is not Apple attestation, OS
wake provenance, background launch, same-user IPC, exactly-once effects,
live-device QA, or production readiness.
Explicit normal key rotation requires an old-key, session-bound,
domain-separated P-256 proof. Explicit lost-key recovery uses a stronger typed
warning but no old-key proof. The packaged app route requires its per-launch
bearer while an explicit legacy server does not; in either mode the phrase is
operator accident prevention, not device, OS-identity, or ownership
authentication. Both use a
short-lived one-shot grant, a single supervised stdin restart, staged Keychain
material, durable crash reconciliation, and a disabled new enrollment that
must be enabled separately. Secret-bearing prepare JSON is accepted by the CLI
only with `system-wake key-prepare --document-stdin`; its response must be
delivered directly to trusted device-only Keychain journal code, which builds
the distinct supervised install document. The raw response is not install
stdin, and neither form belongs in argv, terminal output, shell history, logs,
or intermediate files.
It also includes the buildable Swift/SwiftUI Mac shell under
`apps/mac`, with a tested IPC client, command-console state model,
activity/audit panel with current progress summary, memory
create/update/review/delete and restore management, memory classification
summary, redacted memory retention-plan review, memory review counts in diagnostics and permission policy review, provenance-aware
permission/grant inspection, permission policy review items, redacted scheduler
attention summaries for app handoff, scheduler trigger policy-review items,
release-readiness blocker inspection,
read-only signed-distribution/live-device/plugin-trust runbook rendering,
adapter-backed scheduler notification controls, degraded-mode handling,
Speech/AVFoundation voice input controls, AVFoundation speech-output controls,
and a core supervisor abstraction. A native `MenuBarExtra` keeps Jarvis
reachable when its main window is closed, reflects the shared supervisor's
stopped/starting/available/degraded state, reopens the existing SwiftUI shell,
and routes refresh/start/stop through the same app-owned models. This local
contract does not replace signed-app, Finder/LaunchServices, or live-device UI
validation.
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
assistant: distribution signing/notarization, live microphone/Speech capture
validation, live audio-output validation,
marketplace plugin trust, OS-level plugin network sandboxing, live OS-level
scheduler notification validation, and manual release QA are still target
architecture. The
default command path still uses `FakeLocalModel`; set
`JARVIS_LOCAL_MODEL_PROVIDER=ollama`, `JARVIS_LOCAL_MODEL`, and optionally
`JARVIS_OLLAMA_BASE_URL`/`JARVIS_LOCAL_MODEL_TIMEOUT_MS` to exercise the local
HTTP provider. ChatGPT execution is disabled by default and requires explicit
typed env opt-in. The Platform API-key path uses
`JARVIS_CHATGPT_ENABLED=true`, `JARVIS_CHATGPT_AUTH=api_key`,
`JARVIS_OPENAI_API_KEY`, and optional `JARVIS_CHATGPT_MODEL`,
`JARVIS_OPENAI_BASE_URL`, and `JARVIS_CHATGPT_TIMEOUT_MS`. The Codex account
path uses a logged-in Codex CLI instead: run `codex login --device-auth`, then
launch with `JARVIS_CHATGPT_ENABLED=true`,
`JARVIS_CHATGPT_AUTH=codex_account`, and optional `JARVIS_CODEX_EXECUTABLE`,
`JARVIS_CHATGPT_MODEL`, and `JARVIS_CHATGPT_TIMEOUT_MS`. Route policy still
blocks restricted data and sends only redacted route context. Provider failures
return failed command responses with redacted diagnostics instead of becoming
IPC transport errors. Provider text responses may also use a strict JSON
envelope with `message`, `complete`, and `tool_requests`; accepted tool
requests still pass through the existing first-party schema, policy, approval,
and audit path. ChatGPT/OpenAI-compatible responses may also return native
OpenAI `tool_calls` for the advertised first-party tool definitions; those are
translated into the same bounded first-party path. Plain text remains
supported, and this is not installed-plugin orchestration or broad third-party
tool execution. `/tools/model` and `jarvis tools list` expose the same redacted
registered first-party model-tool catalog that providers receive. Local-model
prompts now include that catalog as a JSON allowlist of exact `plugin_id` and
`action` pairs, and hallucinated or invalid model-planned plugin IDs/actions
fail closed before policy checks or tool execution, then feed registered-tool
guidance back to the model as rejected tool results for bounded recovery. Mixed
prose plus JSON `tool_requests` is treated as malformed provider output instead
of a normal answer.
The Ollama adapter requests `stream:true`, caps the HTTP body and assembled
response, accepts LF/CRLF NDJSON across arbitrary byte boundaries, and requires
one terminal `done:true` frame. Partial text and JSON-looking tool envelopes
stay private to the adapter until the terminal stream and full envelope
validate; cancellation drops the active request and discards partial state.
Activity and Swift surfaces receive redacted byte/character metadata only after
that validation. This is native transport streaming, not partial assistant
transcript rendering or raw-token UI streaming.
The macOS Model tab exposes separate approved cloud routes for `OpenAI API` and
`Codex account`: both disable the local provider for the app-supervised core
and keep `JARVIS_CHATGPT_REQUIRES_APPROVAL=true`. `OpenAI API` stores the
application credential in Keychain instead of SQLite or docs, while `Codex
account` shells through the logged-in Codex CLI and does not require an OpenAI
Platform API key. Jarvis launches that subprocess from a temporary directory,
passes the redacted prompt over stdin, ignores user configuration and project
rules, fixes approval policy to `never`, requests the CLI's read-only sandbox,
uses strict config with web search disabled, mechanically disables the current
CLI tool/integration feature set (including shell, unified-exec, code-host,
apps/plugins, browser, computer use, image generation, multi-agent, and
workspace dependencies), forwards only the small environment allowlist needed
for account auth and networking, discards child logs, monitors and kills the
child if its private final-message file crosses 1 MiB, and repeats the size
check before reading. The
request payload contains only Jarvis's redacted route context, but Codex still
adds its own runtime/system context. A CLI that does not support the constrained
argument contract fails closed before model execution; update the bundled
Codex/ChatGPT app or CLI before retrying. Use `OpenAI API` when a non-agentic
HTTP provider boundary is required.
Plugin availability for model planning means the `/tools/model` first-party
catalog only. `jarvis tools list`, `jarvis tools model`, and
`jarvis tools catalog` all print that same catalog. Chrome/browser-extension
capabilities are unavailable unless they appear there, and installed local
plugins remain outside model-originated planning.
Production inventory excludes the deterministic `fake_*` test fixtures. It
always includes the bounded metadata-only `system_status.status` action. The
macOS app owns durable, user-selected workspace grants as security-scoped
bookmarks and hands their opaque IDs plus resolved paths to the supervised core
through one bounded startup-stdin envelope. The legacy repeatable
`jarvis serve --workspace-root <id>=<absolute-path>` operator option remains
available for compatibility. Either route adds local-only
`workspace_inspect.list` and `workspace_inspect.read_text` for that held root;
without a configured root those actions are absent. Workspace
requests accept only a root ID and relative path, reject traversal, symlinks,
hidden/credential-like/special/binary/oversized targets, cap results, redact
absolute paths and contents from audit, and cannot continue through ChatGPT.
Directory listing uses the explicit `@root` sentinel for the configured root;
empty paths remain invalid. Directories over 200 visible entries fail closed,
text reads stop at 64
KiB with a 16 KiB line cap, and a task cannot accumulate more than 128 KiB of
tool output.
App-selected absolute roots are absent from child arguments, environment,
health, audit, and UI presentation. The app resolves every bookmark fail closed
before launch and retains balanced security-scope access only for the supervised
process lifetime. Manual use of the legacy `--workspace-root` option still
exposes the configured path in that operator-launched process's arguments.
App-supervised launches also rotate a 32-byte bearer credential, deliver it in
the same bounded startup-stdin envelope, and require it on every loopback route.
The app writes the current credential to the owner-only
`~/Library/Application Support/Jarvis/ipc-session-auth.json` handoff file so an
explicit local operator can run, for example,
`jarvis --ipc-token-file "$HOME/Library/Application Support/Jarvis/ipc-session-auth.json" health`.
The token never enters child argv, environment, audit, diagnostics, or UI. A
managed client fails closed before sending when its credential is unavailable;
managed Swift/CLI clients reject non-loopback endpoints before bearer exposure,
and an authenticated core rejects non-loopback binds;
an explicitly unauthenticated legacy server rejects any Authorization header,
preventing silent managed-client downgrade. Repository tests prove possession-
based loopback authentication and restrictive file handling, not App Sandbox
enforcement, sandbox-extension inheritance by the child, OS identity,
same-user/process isolation, or live-device behavior.
For broader registered plugin manifest inspection, `jarvis plugins list`
defaults to a compact operator-readable summary and `jarvis plugins list --json`
prints full manifest schemas.
Live Ollama testing has proven the opt-in local HTTP route can complete real
model commands; model-specific tool discipline can still vary, so the runtime
boundary remains authoritative. Local plugin
installation stores validated manifest metadata disabled by default and captures
a local source-tree provenance snapshot, including manifest and subprocess
entrypoint hashes. Executable local subprocess plugins require the snapshot to
verify as unchanged plus an explicit
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

Installed `local_wasm` plugins provide a narrower compute-only alternative.
They require the explicit `wasm_compute` grant and the custom
`jarvis_json_v1` exports `memory`, `jarvis_alloc`, and `jarvis_run`. Jarvis
rejects all imports, including WASI, environment, filesystem, network, clock,
and process authority, and enforces 4 MiB module, 256 KiB request, 1 MiB output,
16 MiB linear-memory, zero table elements, and 10 million fuel ceilings. Eligible actions are
low-risk, non-proactive computation only, with no memory, model, or network
access. Exact module bytes are included in the install provenance snapshot;
pause, cancellation, timeout, or fuel exhaustion fails closed before output is
accepted. The installed-plugin inspection endpoint and Swift Plugin tab expose
only redacted runtime/confinement metadata. `WASM confined` means Wasmi
language-level confinement; it does not mean a macOS OS sandbox, same-user IPC
isolation, malware analysis, publisher/marketplace trust, signing/notarization,
or live-device validation. `local_subprocess` remains a separate runner and is
presented as `not OS sandboxed`.
Runs may carry a unique `--cancellation-id`; `jarvis plugins cancel-run <id>`
requests cooperative cancellation through local IPC. Wasmi checks it between
fuel slices and before accepting output.

## Production Work Protocol

This public repository is being advanced through isolated worktrees, topic
branches, and reviewable PR slices. During the autonomous production sweep,
agents should stay inside their assigned ownership, preserve unrelated edits,
and treat cross-process E2E plus the local release gate as the evidence bar.
The public GitHub workflow at `.github/workflows/release-local.yml` mirrors
that local gate on macOS for pull requests, pushes to `main`, and manual
dispatch; `scripts/release-ci-workflow-smoke.sh` keeps the workflow wired to
`./scripts/release-local.sh`. The same boundary is exposed as the
`release_ci_gate` contract/readiness feature so clients can cite public CI
evidence without broadening it into distribution or live-device proof.
Passing the local gate supports only the implemented Rust/Swift foundation
claim; it is not proof of a finished packaged assistant.
`/release/readiness`, `jarvis release readiness`, and the Swift Release tab
summarize implemented feature proofs, pending feature boundaries, recommended
verification commands, and manual production blockers in read-only surfaces.
`/release/live-device-runbook`, `/release/signed-distribution-runbook`, and
`/release/plugin-trust-runbook` expose the same runbook families as structured
read-only IPC payloads, and the Swift Release tab renders them when the running
core supports those endpoints. These runbooks summarize current evidence and
next operator commands only; they do not sign, notarize, install, launch,
validate live devices, review plugins, or generate final evidence.
`./scripts/release-external-handoff.sh --write target/release-external-handoff`
prepares the public release-operator handoff directory with sourceable
live-device, plugin-trust, and final-bundle env templates, read-only
readiness/evidence/runbook JSON snapshots, `release-evidence-checklist.md`, and
`release-handoff-manifest.json` with generation metadata plus per-file SHA-256
digests. That checklist names the exact signed-distribution paths, live-device
command/notification fields, plugin artifact URI/SHA-256 bindings, and final
archive URI still required before the final doctor assertion. The manifest helps
operators archive and compare the handoff package, but it is still handoff
scaffolding only, not owner-recorded external evidence that those checks were
completed.
The CLI command defaults to operator-readable text, supports `--json` for the
exact structured payload, and supports `--all-commands` when operators need the
complete readable verification runbook instead of the compact first commands.
It prefers a running IPC server but falls back to the same conservative local
readiness summary when the server is unavailable or loopback IPC is denied, so
release triage still works before starting the supervised core.
Readiness feature metadata includes the repository-backed operator QA smoke as
implemented local evidence, with clean-profile installed-app and live-device
QA still listed as manual gates.
The response keeps `production_ready: false` by default. When the running core
has `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external`, readiness can compute
`production_ready: true` only when every required `/release/evidence-status`
item is present, no missing or invalid evidence remains, and evidence-cleared
features leave no pending readiness features. The structured readiness payload
also exposes `evidence_mode_enabled` so operators can verify that the running
core, not only the CLI process, was started in external evidence mode. That remains owner-recorded
external evidence: Jarvis does not itself perform Developer ID signing,
notarization, stapling, clean-profile install/Finder validation, live-device
QA, plugin trust QA, or manual release QA.
Only start or restart the core with
`JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external` after owner-recorded external
QA reports and signed-distribution evidence have been collected, then query
`jarvis release readiness` against that running core.
In external evidence mode, the live-device QA report must still pass semantic
validation for the expected installed app path, bundle identifier, short/build
version, non-self-test identity, ordered non-future UTC voice-check timestamps, and
structured spoken-command observation with a task/audit command evidence
reference that resolves through repository-backed IPC evidence-status before it
can clear the live voice blocker. The shell QA scripts still preflight that
field by shape only because they do not own the SQLite repository.
Opt-in final-transcript auto-submit is text-path parity only; it does not clear
live microphone/Speech/audio-output validation or manual release QA.

Phase 3 landed through separate worktrees for model route persistence, plugin
subprocess grant gating, voice input controls, local packaged app launch proof,
permission grants UX, docs architecture alignment, distribution packaging, and
Keychain launch credential injection. Follow-on slices have added Swift memory
CRUD, local plugin provenance verification, scheduler attention handoff, and
permission policy review plus scheduler trigger review and notification
controls, including redacted proactive scheduler policy audit before due
command execution, explicit stale-running scheduler recovery, opt-in startup
stale-running recovery, and strict provider tool-request envelope coverage for
Ollama-compatible and ChatGPT/OpenAI-compatible text responses, including the
macOS Model tab routes over that same guarded cloud provider. Later slices
continue the same branch/PR discipline;
release language should describe only the merged repo-owned surfaces with
recorded focused E2E or integration proof.
The permission center now surfaces installed-plugin provenance status from
`/permissions/grants`, including unverified plugin counts and local integrity
state. `/permissions/policy-review` turns pending approvals and installed
plugin provenance/grant/network concerns plus unreviewed memory items and
deleted sensitive memory retained in local storage into explicit review items
without exposing memory values. `/memory/retention-plan` and
`jarvis memory retention-plan` now provide the memory-specific, redacted
operator action list for active unreviewed memory and deleted sensitive memory
that is still retained locally; the plan is inspection-only and keeps
automation disabled. The Swift Memory tab renders the same redacted plan above
the item list. `/memory/index/status`, `/memory/index/rebuild`, the matching
`jarvis memory index-*` commands, and the Swift Memory tab now govern an atomic,
versioned projection manifest rebuilt only from active canonical SQLite rows.
Public status is count-only and the projection is not used for retrieval or
cloud context. These surfaces still do not grant broader marketplace
trust, malware safety, autonomous memory rewriting, semantic retrieval, purge automation, or
OS-level network sandboxing.
Approval grant/deny decisions remain side-effect-free. Approved first-party
approval records require a separate one-shot `/approvals/:id/execute` or
`jarvis approvals execute <approval-id>` replay, which verifies the original
action and scopes before recording `approval_executed` audit evidence. The
Swift Approval Center exposes the same boundary by showing Run Approved only
for approved records that do not already have execution audit evidence.

## Build And Test

For executable PR evidence, run the canonical local release gate:

```sh
./scripts/release-local.sh
```

It wraps Rust fmt/clippy/tests, ignored release-proof tests, smoke scripts,
cargo package verification, signed-provenance self-tests, unsigned
release-layout launch checks, release evidence preflights/self-tests, the
GitHub workflow smoke check, external handoff checklist generation, and Swift
build/test. Focused commands below are for local iteration or
ownership-specific proof; they do not replace the full gate for executable
changes.

For the current IPC smoke path, start the local server and run CLI commands
from a second terminal:

```sh
cargo run -p jarvis-cli -- serve
cargo run -p jarvis-cli -- health
```

`jarvis health` and strict IPC commands such as `jarvis command`,
pause/resume, scheduler, task/audit/activity/route, memory, approval,
diagnostics, installed-plugin, and permission-center operations require a
running core server. If the server is down, they exit non-zero with guidance
to start `jarvis serve`, run the offline ephemeral `jarvis smoke` check, or use
read-only fallback inspection commands such as `jarvis release readiness`,
`jarvis plugins list`, and `jarvis tools list`.

Use `cargo run -p jarvis-cli -- serve --db-path /tmp/jarvis.sqlite` when you
want manual IPC commands to persist task and audit state locally.

Use the following focused commands when iterating on the named surface, then run
`./scripts/release-local.sh` before treating executable changes as release-gate
evidence.

For branches that touch SQLite migrations or file-backed repository startup,
run the focused storage recovery proof:

```sh
./scripts/storage-migration-backup-smoke.sh
```

That script proves legacy file-backed DB migration creates a preflight backup,
failed migration-open restores the backup, newer schema versions fail with an
explicit upgrade diagnostic, and representative schema v1-v11 fixtures preserve
critical rows through the current migration path. It does not replace installer
upgrade QA.

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

For distribution packaging work, run:

```sh
./scripts/release-version-consistency.sh --check
./scripts/package-distribution.sh --check
./scripts/package-distribution.sh --unsigned-structure-check
./scripts/package-distribution.sh --unsigned-launch-check
```

`./scripts/packaged-app-release-smoke.sh` remains only as a deprecated
compatibility wrapper that delegates to
`./scripts/package-distribution.sh --unsigned-launch-check`; new release
evidence and readiness recommendations should cite the distribution command.
The release version is derived from the Rust package metadata and checked
against the CLI/core crate versions before packaging or evidence scripts build
versioned artifact names. The CLI also exposes `jarvis --version`, and the
distribution packaging/evidence scripts now require the bundled
`Contents/Resources/bin/jarvis-cli --version` output to match that expected
release version before artifact evidence can pass. Packaging also writes a
read-only `Contents/Resources/bin/jarvis-cli.version` marker, and
`/release/evidence-status` validates that marker without executing the bundled
artifact path. If the marker is missing or stale, rebuild the distribution
artifact with `./scripts/package-distribution.sh --unsigned-launch-check` for
local evidence, or rerun the signed packaging lane before final release
evidence.
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
package creation when Apple credentials are provided. It now also writes a
`Jarvis-<version>-signed-provenance.json` report with signing identities,
notary submission IDs/log paths, staple validation output, Gatekeeper
assessment output, bundled core `jarvis --version` output, and artifact
SHA-256 digests for the signed zip/pkg, plus the bundled
`Contents/Resources/bin/jarvis-cli` path and SHA-256 digest. It
still does not replace clean-profile installer run, Finder launch, App Store
validation, or live microphone/Speech/audio-output validation.
For those live checks, `./scripts/release-live-device-qa.sh --check` prints the
required manual runbook and is part of the default local release gate.
`cargo run -p jarvis-cli -- release signed-distribution-runbook` summarizes the
current signed app bundle, bundled core, signed zip/pkg, and signed provenance
evidence inventory and prints the next signing/evidence commands without
performing signing, notarization, stapling, Gatekeeper assessment, installation,
or QA.
`cargo run -p jarvis-cli -- release live-device-runbook` prints the same
operator path together with the current `live_voice_loop` and
`live_device_qa_report` evidence status, without performing live validation,
and is now covered by the default local release gate. The runbook commands also
include the release-core command evidence capture, the `task:<uuid>`/`audit:<uuid>`
recording rule, and the external evidence-mode readiness/evidence-status
commands with the release endpoint placeholder.
`./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env`
writes a sourceable checklist of every required `JARVIS_QA_*` flag and evidence
field. The generated template also carries the command to run against the
release core, the `task:<uuid>`/`audit:<uuid>` evidence-ID rule, and the
evidence-mode readiness/evidence-status verification commands to run after the
report is generated. After the owner validates a signed installed app on a real
Mac profile, fill that template, source it, and rerun the script with
`--assert-complete`.
The assertion requires explicit transcript handoff validation, structured
spoken-command observation fields with the installed app path matching the
expected `/Applications/Jarvis.app` path, unless explicitly overridden with
`JARVIS_QA_INSTALLED_APP_PATH`, the observed transcript matching the spoken
test phrase, and expected command text matching observed command text,
`JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID` set to `task:<uuid>` or `audit:<uuid>`
from the live command/audit evidence, owner/device/profile, ordered UTC
timestamps, non-voice owner evidence notes for clean-profile, Finder launch,
notification, restart, and manual QA, voice evidence-note fields, and a bundled-core binding for the
installed `Contents/Resources/bin/jarvis-cli` path, `jarvis <version>` output,
and SHA-256 digest. Owner-recorded evidence-note fields must contain
non-placeholder text, not values such as `TODO`, `pending`, `n/a`, `fixture`, or
`self-test fixture`; `JARVIS_QA_SELF_TEST_FIXTURE` is reserved for the script's internal
`--self-test` report and is not valid release evidence.
`/release/evidence-status` applies the same non-empty and non-placeholder checks
to the generated report, so weak owner evidence cannot clear `live_voice_loop`.
Missing required live voice evidence notes, command-result evidence ID,
audio-output device label, notification title/body/thread/timestamp, or proof
boundary also keep `live_device_qa_report` invalid and keep external-mode
readiness fail-closed.
It writes a JSON report, defaulting to
`target/release-live-device-qa-report.json`, with installed-app metadata,
microphone/Speech permission prompt evidence, spoken transcript handoff into
the command path, speech-output playback evidence, owner-recorded live voice
and non-voice evidence notes, bundled-core path/version/digest evidence, structured command
observation, and the proof boundary. The
installed app metadata must match the approved `Info.plist` copy exactly:
`NSMicrophoneUsageDescription` is `Jarvis uses microphone input only when you explicitly start local voice capture.`, and
`NSSpeechRecognitionUsageDescription` is `Jarvis uses speech recognition only to turn your spoken command into a local assistant request.`. The
local release gate also runs `./scripts/release-live-device-qa.sh --self-test`
against a fake app fixture to prove the assertion/report mechanics without
claiming real device validation.
For plugin trust checks, `./scripts/release-plugin-trust-qa.sh --check` prints
the marketplace, malware-analysis, OS sandbox, and egress-enforcement runbook
and is part of the default local release gate. Its `--self-test` mode proves
JSON evidence-report mechanics with fake flags and fake evidence notes only.
`./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env`
writes a sourceable checklist of every required `JARVIS_PLUGIN_QA_*` flag and
evidence field. The template defaults validation flags to `false`; fill and
source it only after external plugin trust checks have actually completed.
`jarvis release readiness --all-commands` includes the matching
`set -a && source target/release-plugin-trust-qa.env && set +a &&
./scripts/release-plugin-trust-qa.sh --assert-complete` command so operators
can use the generated template instead of reconstructing the long inline env
command.
`cargo run -p jarvis-cli -- release plugin-trust-runbook` prints the same
plugin-trust operator path with current `plugin_trust_qa_report` evidence
status, exact template/assertion/evidence commands, and the explicit boundary
that no marketplace review, malware scanning, sandbox deployment, or
host-level egress enforcement was performed.
The owner-recorded `--assert-complete` path writes
`target/release-plugin-trust-qa-report.json` after all required
`JARVIS_PLUGIN_QA_*` flags are true and owner/timestamp/evidence-note fields are
populated. The report must identify itself with `schema_version: 1` and
`evidence_type: owner_recorded_plugin_trust_qa`, and `self_test_fixture` must be
`false`, before the doctor/status gates will accept it. Operator reports must
also keep `review_source: owner-asserted-manual-review`; imported or
self-test review sources are rejected by the assertion, doctor, bundle, and
evidence-status paths. Artifact URIs must include a durable URI scheme/location
and cannot point at placeholder, self-test, fixture, or temporary paths.
Host-level egress evidence now requires an owner-recorded policy label, UTC
egress validation timestamp, denied undeclared-host fixture note, and
declared-host allow fixture note, but that report remains manual external
evidence rather than repo-local proof of marketplace, host sandbox, or host
egress enforcement systems.
`./scripts/release-evidence-bundle.sh --check` ties those external proof paths
together by listing the expected signed distribution artifact paths,
signed-distribution provenance report, live-device QA report,
plugin-trust QA report, and owner validation flags
required before a final release evidence manifest can be written. The generated
manifest must identify itself with `schema_version: 1` and
`evidence_type: release_evidence_bundle` before the doctor/status gates accept
it. The `--check` and doctor/status paths are read-only inventory plus report
semantic validation: they do not perform Developer ID signing, notarization,
stapling, installation, or manual QA, but they validate signed-provenance JSON
semantics, app bundle metadata, bundled-core version/digest binding, QA report
fields, final-bundle path/digest bindings, owner-recorded release evidence
fields, and local-signature-validation flags before reporting evidence as usable. The `--check` output
also points operators to
`./scripts/release-evidence-bundle.sh --write-template
target/release-evidence-bundle.env` to generate the sourceable final-bundle
checklist with every `JARVIS_EVIDENCE_*` validation flag defaulting to `false`;
source it only after the matching external release evidence has actually been
validated. `jarvis release readiness --all-commands` includes the matching
`set -a && source target/release-evidence-bundle.env && set +a &&
./scripts/release-evidence-bundle.sh --bundle` command before the inline
owner-flag example, followed by `./scripts/release-evidence-doctor.sh
--assert-complete` as the final inventory assertion. Its `--self-test` mode
uses fake artifacts/reports to prove bundle mechanics only; `--bundle` writes
`target/release-evidence-bundle.json` after the referenced evidence files,
including signed provenance, exist, all required `JARVIS_EVIDENCE_*` flags
are true, and final owner name/timestamp/evidence-note/archive fields are
populated. Non-default live-device and plugin-trust report paths can be supplied
with either the QA script variables (`JARVIS_QA_REPORT_PATH`,
`JARVIS_PLUGIN_QA_REPORT_PATH`) or the bundle/doctor aliases
(`JARVIS_EVIDENCE_LIVE_QA_REPORT`, `JARVIS_EVIDENCE_PLUGIN_QA_REPORT`). The
live-device QA report must bind the validated bundle identifier, app version,
build version, and voice permission usage strings to the installed app. Unlike
doctor/status inventory, the real `--bundle` path also validates the signed
provenance report, local app signature, app stapling ticket, installer signature, installer stapling ticket,
and app zip payload, then records SHA-256 digests for distribution artifacts and
QA reports before writing the manifest.
The full readiness runbook is ordered for release execution: local gates and the
unsigned launch check first, the Developer ID packaging/notarization command
before live-device QA evidence capture, plugin-trust QA before final bundling,
the evidence doctor assertion after both bundle paths, and the external
evidence-mode readiness check last.

With a repository-backed server running, `jarvis release readiness`,
`jarvis release evidence-status`,
`jarvis tasks`, `jarvis routes`, `jarvis memory`, `jarvis activity summary`,
`jarvis activity watch`, `jarvis scheduler`, `jarvis diagnostics`, and
`jarvis plugins` expose the current readiness evidence, durable state, status
counts, redacted recent task metadata, recent audit progress, bounded activity
events, redacted model-route evidence, redacted scheduler attention handoff,
scheduler trigger policy review, redacted diagnostics,
operator-readable first-party plugin manifest summaries, disabled installed-plugin registry metadata, and
structured release evidence file/report presence over IPC. The default readable
release evidence-status output includes per-item paths and invalid/missing
details, while `--json` preserves the raw IPC payload. Task, route, and
activity summary commands plus registered plugin/tool inspection default to
operator-readable text; use `--json` or `JARVIS_CLI_JSON=1` for exact IPC
payloads, including stored task input and full plugin schemas.
Evidence-status items report present/missing/invalid inventory. Artifact paths
are presence-only checks except the app bundle, whose `Info.plist` bundle id,
short version, and build version must match the expected release metadata. JSON
reports receive semantic validation for signed-distribution provenance
version/bundle metadata, bundled core path/version/SHA-256 binding,
signing/notary/staple and Gatekeeper fields, required flags, SHA-256 digests,
signed-provenance zip/pkg/core digest matches against current artifact files,
live-device QA metadata, plugin-trust non-future timestamps plus
`review_source: owner-asserted-manual-review`, and final bundle
path/digest semantics, child-report semantic validity, and
`validation_flags.local_signature_validation`. The status surface still does not
perform signing, notarization, stapling, installation, or manual QA.

## Docs

- [Architecture map](docs/architecture-map.md)
- [Production readiness sweep - 2026-06-11](docs/production-readiness-sweep-2026-06-11.md)
- [Production readiness sweep - 2026-06-10](docs/production-readiness-sweep-2026-06-10.md)
- [Plugin contract](docs/plugin-contract.md)
- [Safety rules](docs/safety-rules.md)
- [Build and test commands](docs/build-test-commands.md)
- [Release checklist](docs/release-checklist.md)
- [Knowledge-base facts](docs/knowledge-base/jarvis-project-facts.md)

The architecture map includes both the implemented current-state diagram and
the end-goal production diagram, plus a phase table that separates verified
foundation work from future production assistant requirements.
