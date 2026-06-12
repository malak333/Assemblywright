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
pull requests, pushes to `main`, and manual dispatch. The workflow is
configuration evidence only; it still does not perform Developer ID signing,
notarization, clean-profile installation, Finder launch validation, live-device
QA, or plugin marketplace trust review.
`/contract` and release readiness expose this lane as `release_ci_gate` with
the same boundary.

```sh
./scripts/release-version-consistency.sh --check
./scripts/release-ci-workflow-smoke.sh
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
swift test --disable-sandbox --package-path apps/mac
swift build --disable-sandbox --package-path apps/mac
```

Focused workflow-shape check:

```sh
./scripts/release-ci-workflow-smoke.sh
```

## Current Health Check

`jarvis health` is a strict IPC liveness check. Strict IPC commands such as
`jarvis command`, pause/resume, scheduler, task/audit/activity/route, memory,
approval, diagnostics, installed-plugin, and permission-center operations also
require a reachable repository-backed core. If the endpoint is down, these
commands exit non-zero with operator guidance to start `jarvis serve`, run the
offline ephemeral `jarvis smoke` check, or use read-only fallback inspection
commands such as `jarvis release readiness`, `jarvis plugins list`, and
`jarvis tools list`, instead of returning a raw connection-refused error.

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
QA reports:

```sh
cargo run -p jarvis-cli -- release readiness
```

Evidence-aware readiness only accepts release reports that pass
`release evidence-status` semantic checks. The app bundle item checks
`Contents/Info.plist` for the expected bundle ID, short version, and build
version before it can count as present, and the bundled core item checks the
packaged `Contents/Resources/bin/jarvis-cli.version` marker without executing
the artifact path. Missing or stale bundled-core markers should be remediated
by rerunning `./scripts/package-distribution.sh --unsigned-launch-check` for
local evidence, or the signed packaging lane for final release evidence.
Live-device QA checks schema/type,
`self_test_fixture=false`, expected bundle ID, matching short/build version,
repository-backed command-result evidence, and ordered non-future UTC
voice-check timestamps. Plugin-trust checks ordered non-future UTC review and
egress validation timestamps, `review_source=owner-asserted-manual-review`,
and non-empty owner-recorded marketplace, malware, signed-publisher, OS
sandbox, host-egress, deny-fixture, allow-fixture, and manual-review evidence
fields. The final evidence bundle checks the expected release version,
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

Live local testing with `llama3.2` has proven this Ollama route can complete
real model commands. Local model behavior is still model-dependent, so the
runtime derives the provider-visible tool catalog from validated first-party
manifests, exposes the same redacted catalog through `jarvis tools list`,
advertises it as an Ollama JSON allowlist and ChatGPT/OpenAI-compatible native
tool definitions, and keeps validating every model-planned tool request before
execution.

Provider tool troubleshooting: if Ollama or a ChatGPT/OpenAI-compatible provider
requests `plugin_id: "status"` or `plugin_id: "chrome_extension"`, that is a
provider hallucination, not a missing installed plugin. Inspect the exact
model-visible catalog with `cargo run -q -p jarvis-cli -- tools list`,
`cargo run -q -p jarvis-cli -- tools model`, or
`cargo run -q -p jarvis-cli -- tools catalog`. Current valid first-party
model-visible pairs are `fake_echo.approval_echo`, `fake_echo.echo`, and
`fake_status.status`; installed plugins can be inspected or explicitly run
through `jarvis plugins ...` commands, but they are not exposed to
model-originated tool planning.

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
`tool_calls` for the same runtime-derived first-party tool inventory; these
are translated into the same bounded first-party path. This is provider tool
compatibility, not installed-plugin orchestration.

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
cargo run -p jarvis-cli -- tasks list --json
cargo run -p jarvis-cli -- tasks audit
cargo run -p jarvis-cli -- routes list
cargo run -p jarvis-cli -- routes list --task-id <task-id>
cargo run -p jarvis-cli -- routes get <route-id>
cargo run -p jarvis-cli -- routes get <route-id> --json
cargo run -p jarvis-cli -- approvals list --status pending
cargo run -p jarvis-cli -- approvals approve <approval-id> --decided-by cli --reason "reviewed"
cargo run -p jarvis-cli -- approvals execute <approval-id>
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
cargo run -p jarvis-cli -- memory create workflow release-gate "run local gate before PR" --provenance "manual note" --sensitivity workspace
cargo run -p jarvis-cli -- memory restore <memory-id>
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
sensitive memory counts when repository backing is enabled.
`jarvis release readiness` is read-only and summarizes implemented feature
proofs, pending feature boundaries, recommended verification commands, and
manual production blockers. The default CLI output is operator-readable and
falls back to conservative local metadata when loopback IPC is unavailable,
including restricted environments that deny loopback sockets; use
`--all-commands` for the complete readable verification runbook, or `--json` or
`JARVIS_CLI_JSON=1` for the exact structured payload. Evidence-aware mode can
clear the live voice/audio blocker from a valid live-device QA report. When the
running core has `JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external`, it can
compute `production_ready: true` only when every required
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
`JARVIS_PLUGIN_QA_*` flag and populates the owner/timestamp/evidence-note fields.
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
The release readiness runbook also includes
`set -a && source target/release-evidence-bundle.env && set +a &&
./scripts/release-evidence-bundle.sh --bundle` as the template-backed bundle
path.
`jarvis release readiness --all-commands` is ordered as a release execution
runbook: local gates, unsigned distribution launch check, signed-distribution
runbook triage, signed/notarized packaging, live-device QA, plugin-trust
runbook triage, plugin-trust QA, final evidence bundle generation,
evidence-doctor assertion, then the external evidence-mode readiness check.
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
evidence.
`cargo run -p jarvis-cli -- release plugin-trust-runbook` is read-only;
it summarizes the current `plugin_trust_qa_report` evidence item and prints the
exact plugin-trust template, assertion, evidence-status, evidence-doctor, and
signed-distribution follow-up commands without performing marketplace review,
malware scanning, sandbox deployment, host-level egress enforcement, signing,
notarization, live-device QA, or final evidence bundling.
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
packaging lane, including signing identities, notary submission IDs/log paths,
staple validation, Gatekeeper assessment, bundled core version, and artifact
digests that match the current zip/pkg files. It rejects disabled
local signature validation outside the fake self-test lane, parses every required
live-device/plugin-trust report flag, requires non-empty and non-placeholder
owner-recorded evidence-note fields in both QA reports and the final bundle,
requires live-device QA app bundle metadata to match the
expected installed app path plus bundle id/version, requires the observed
transcript to match the spoken test phrase, and writes SHA-256 digests for
distribution artifacts, signed provenance, and QA reports before writing
production evidence.
`./scripts/release-evidence-doctor.sh --check` inventories the expected signed
artifact paths, signed-distribution provenance report, live-device QA report,
plugin-trust QA report, and final
evidence bundle manifest, then reports present, missing, or invalid evidence
without failing the local gate. It also validates the packaged app metadata and
bundled core version marker before counting the local app artifacts as
present. When evidence is missing it also prints the
next package preflight, signing, live-device template/assertion, plugin-trust
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
deny/allow fixture notes. Production plugin-trust reports must carry
`review_source=owner-asserted-manual-review`; imported reports and self-test
review sources are rejected before they can clear evidence-aware readiness.
`jarvis release evidence-status` exposes the same standard artifact/report
inventory through `/release/evidence-status`; the default CLI output is
operator-readable, includes per-item paths and details for present, missing,
or invalid evidence items when the structured payload provides them, and
`--json` preserves the exact structured payload. It
also rejects signed-provenance zip/pkg digests that no longer match the current
artifact files, and rejects final evidence bundles with the wrong
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
./scripts/release-plugin-trust-qa.sh --self-test
./scripts/release-evidence-bundle.sh --self-test
./scripts/release-evidence-doctor.sh --self-test
cargo test -p jarvis-core diagnostics_export_is_redacted_and_counts_repository_state -- --nocapture
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
with an explicit upgrade diagnostic, and migrate representative schema v1-v8
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
through a fake adapter.
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
final bundle handoff. Its `--unsigned-structure-check`
mode builds and inspects the release app/pkg structure without Developer ID
credentials, including unsigned package identifier, version, and `/Applications`
install-location metadata. Its `--unsigned-launch-check` mode also validates
that package metadata, launches the release-built app executable with an
isolated temporary HOME, verifies the bundled core over loopback IPC, and checks
command, audit, diagnostics, pause/block/resume, and SQLite state through the
release app layout. The packaged core is also checked
with `jarvis-cli --version`, and release evidence scripts reject bundles whose
core binary does not report the expected release version. Full mode requires
`JARVIS_DEVELOPER_ID_APPLICATION`,
`JARVIS_DEVELOPER_ID_INSTALLER`, and notarytool credentials. It signs the
release bundle with hardened runtime and microphone entitlements, submits the
app zip for notarization, staples the app, then creates a signed
`/Applications` installer package at
`target/distribution/Jarvis-<release-version>.pkg`,
checks its installer signature, submits it for notarization, and staples the
package. The signed-provenance report records exact notary `Accepted` statuses,
not only submission UUIDs. Passing the unsigned structure or launch checks still does not prove
signing/notarization, and passing full mode still does not replace
clean-profile install, Finder launch, live microphone/Speech validation, App
Store review, spoken transcript handoff, or live audio-output validation.
`./scripts/release-version-consistency.sh --check` derives that release version
from Rust package metadata and is part of `./scripts/release-local.sh`, so
package, live QA, evidence bundle, and evidence doctor defaults cannot silently
drift from the CLI/core crate versions.
`./scripts/release-live-device-qa.sh --check` keeps the live-device QA runbook
in the default release gate. It validates the repo-owned entitlement/checklist
preconditions and prints the required clean-profile install, Finder launch,
microphone/Speech, spoken transcript handoff, live audio-output, notification,
restart, and manual QA steps. `--write-template target/release-live-device-qa.env`
writes a sourceable checklist for the release operator to fill on the validated
machine, with `JARVIS_QA_EXPECTED_VERSION` materialized from the canonical
Rust package release version at generation time. The template also includes the
release-core `jarvis command ... --json` evidence capture, the
`JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:<uuid>"`/`"audit:<uuid>"` rule, and
the external evidence-mode `release evidence-status` and `release readiness`
checks to run after report generation. The CLI/IPC live-device runbook mirrors
that guidance so operators see the release-core command capture and endpoint
aware external evidence-mode commands before report generation. `--assert-complete` is for the release machine after those checks are
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
`JARVIS_QA_OBSERVED_COMMAND_TEXT`, `JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID`, and
`JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL`.
All owner-recorded evidence-note fields must contain non-placeholder text, not
values such as `TODO`, `pending`, `n/a`, `fixture`, or `self-test fixture`, and
`JARVIS_QA_SELF_TEST_FIXTURE=true` is reserved for the script's internal fake
fixture self-test rather than release evidence. `--assert-complete`,
`jarvis release evidence-status`, and `/release/evidence-status` enforce the
same non-empty and non-placeholder live-device QA report fields before that
evidence can clear `live_voice_loop`.
The observed transcript must match the spoken test phrase after trimming, the
expected installed app path must match `JARVIS_QA_INSTALLED_APP_PATH` or
`/Applications/Jarvis.app`, expected and observed command text must match after
trimming, `JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID` must be `task:<uuid>` or
`audit:<uuid>` from live command/audit evidence, and repository-backed
`/release/evidence-status` must resolve it to an existing task or
task-associated audit row before it can clear readiness. `generated_at` must be
UTC and no earlier than the completed voice check, but not future-dated.
On success, `--assert-complete` writes a JSON evidence report to
`JARVIS_QA_REPORT_PATH` or `target/release-live-device-qa-report.json` by
default. The report includes installed-app metadata, app microphone/Speech usage
descriptions, bundled-core path/version/digest evidence, all required
validation flags, voice-loop evidence fields, owner/device/profile/timestamp
and live voice/non-voice evidence-note fields, structured command observation
including `audio_output_device_label`, schema identity, and the proof boundary.
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

For installed subprocess plugin progress-event changes, run the focused
redaction/audit test before the full release gate:

```sh
cargo test -p jarvis-core installed_plugin_runner_records_subprocess_progress_events_without_raw_stderr -- --nocapture
```

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
first-party tool-call orchestration, including policy checks, approval stops,
validation failures, runtime-derived provider-visible first-party inventory,
and tool-result feedback into later model steps. Focused provider tests prove
typed Ollama-compatible request/error behavior, strict
Ollama-compatible and ChatGPT/OpenAI-compatible response-envelope parsing,
malformed envelope redaction, and structured failed command responses for
selected model-provider failures without requiring a live model during the
default release gate. The cross-process local IPC E2E includes an
Ollama-compatible stub that emits a provider tool-request envelope and proves
the runtime advertises the registered first-party tools, executes the selected
first-party tool, and returns the provider's final message. They do not prove
live ChatGPT service execution,
advanced memory classification policy beyond the current summary surface, live
microphone capture, or live audio output until those surfaces are manually
validated. The current Swift gate proves the
Mac shell builds, decodes IPC contracts, decodes live CLI fallback JSON
for release readiness and release evidence-status, decodes release runbook
payloads, requests the three runbook IPC endpoints, refreshes Release tab
runbook state, exposes management models for
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
