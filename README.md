# Assemblywright

> Orchestrated intelligence. Verified software.

Assemblywright is a local-first developer-agent system that coordinates
specialized models through approved specifications, bounded implementation
stages, and deterministic proof. The current repository also retains the
general macOS assistant foundation that preceded the rebrand. It implements
the Rust core described in [DESIGN.md](DESIGN.md): durable task/audit
primitives, policy-gated first-party plugin commands, bounded model-planned
first-party tool orchestration plus explicit local-only installed WASM tool
opt-in, strict provider response envelopes for model-tool requests, local-first
model routing evidence, opt-in
Ollama-compatible local HTTP and ChatGPT/OpenAI-compatible provider boundaries,
bounded Ollama-native NDJSON transport streaming with terminal-frame
quarantine and in-flight cancellation,
structured provider-failure responses with route/audit evidence, plugin
contracts, metadata-only local plugin installation, local plugin
provenance snapshots, scheduler state, redacted diagnostics export, a default
app-supervised Unix-domain-socket IPC surface plus explicit loopback CLI
compatibility policy and feature proof/boundary metadata,
repository-backed activity summary and activity event stream, conservative
release-readiness inspection, read-only release runbook IPC surfaces, and CLI smoke paths for the Swift shell
and local packaged app proof.

The workspace also contains a portable `jarvis-protocol` crate and a portable
`jarvis-master` durable state kernel plus headless executable as default-inert
distributed-development seams. The protocol crate defines versioned, bounded
handshake, capability, job, lease, cancellation, and result contracts. The
master kernel adds an isolated SQLite schema for explicitly registered device
metadata, connection epochs and sequence high-water marks, queued steps,
leased attempts, cancellation, expiry, exact result acceptance, capability
limits, disconnect handling, and restart reconciliation. The executable adds
single-process database ownership, setup/serve/health operator commands, an
authenticated loopback development transport, and a separate deterministic
fake-worker process. A Windows-only enrollment CLI adds a DPAPI-current-user
protected ECDSA P-256 enrollment CA, ten-minute single-use digest-only grants,
verified client CSRs, 30-day client certificates, server-bound device identity,
rotation, and immediate certificate/device revocation. Schema v4 migrates the
existing lifecycle database transactionally, retains the v1/v2 state, and adds
a metadata-only event journal with a server-issued durable stream cursor. An
explicit `serve --remote-bind <concrete-ip>:<port>` additionally starts a TLS
1.3-only listener beside the loopback listener. It requires enrolled client
certificates, rechecks durable certificate/device revocation on every request,
binds the application handshake to the TLS exporter, enforces certificate-owned
device identity and role, reconciles the accepted connection when its TLS
socket closes, and exposes MacBridge-only bounded event pages. Enqueue, lease,
terminal result, cancellation, disconnect, expiry, and restart reconciliation
commit cursor events with their authoritative state transitions. These surfaces
are checked by the Windows distributed gate. The
Windows service lifecycle adds explicit install/start/stop/status,
maintenance-enter/exit, recovery, and uninstall commands. The SCM service uses
automatic start, bounded 5/15/60-second restart attempts, the existing
single-owner/reconciliation path, and a durable fail-closed maintenance marker
that blocks new enqueue/lease admission while allowing already-started results
to settle. LocalSystem is loopback-only; remote mTLS requires explicit
owner-account credentials through bounded stdin so the service can access that
owner's DPAPI CA. Owner-account installation resolves the exact Windows SID and
idempotently grants `SeServiceLogonRight` through the native LSA policy API;
failure removes the partially installed service. Private-overlay discovery,
live model inference, repository authority, or Codex dispatch is not
implemented by these foundation slices; the existing macOS runtime and release
boundary remain authoritative. The Mac package now adds a default-inert
Developer Mode enrollment and bridge client. A Windows-local `enrollment pair`
process retains the raw single-use grant, emits only a strict public invitation,
and accepts one public CSR reply. Swift generates a non-exported Secure Enclave
P-256 key, journals enrollment metadata and installed certificates in a
distinct device-only Keychain namespace, pins the invitation CA and endpoint,
requires TLS 1.3 mutual authentication, derives the fixed TLS exporter, and
accepts the application session only when the master returns the exact registry
revision. The provisioned bridge CLI also has an explicit `monitor` mode that
reuses one authenticated session for exact bounded health samples and drops the
session before reconnecting with capped backoff after any transport or response
failure. The Swift app can explicitly supervise this separately signed helper
when `JARVIS_MAC_DEVELOPER_BRIDGE_EXECUTABLE` names its exact executable and
`JARVIS_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER` independently pins its Apple
team. The opt-in is absent by default, verifies Apple code identity plus the distinct
bridge Keychain group, clears the child environment, parses only strict bounded
redacted snapshots, and exposes a read-only Developer tab. The workspace now
also includes a default-inert Rust `jarvis-agent` relay. It requires a matching
direct parent before storage opens, receives socket/code-identity/bearer policy
through bounded stdin, reuses the secure owner-only UDS framing, and persists
only the exact accepted event cursor. A macOS cross-process E2E proves bearer
and code-identity enforcement, argv non-disclosure, exact cursor advance, replay
denial, and parent-mismatch denial. With the additional
`JARVIS_MAC_DEVELOPER_AGENT_EXECUTABLE` and
`JARVIS_MAC_DEVELOPER_AGENT_DATA_DIR` opt-in, the app passes only those
secret-free paths to the signed helper; the helper pins and directly supervises
the exact agent, retains the Keychain/mTLS channel, and forwards authenticated
metadata batches into the durable cursor. A third, exact-value opt-in,
`JARVIS_MAC_DEVELOPER_FIXTURE_JOBS_ENABLED=true`, additionally exposes only the
Public, no-retention `fixture.reasoning` synthetic-echo diagnostic. The master
binds lease, result, cancellation, acknowledgement, device, epoch, and attempt;
the agent keeps the active fixture only in memory and suppresses output after
cancellation. A separate exact-value
`JARVIS_MAC_DEVELOPER_MLX_JOBS_ENABLED=true` standard-profile lane now runs one
bounded Public, no-retention `mlx.reasoning` request at a time through an
explicit absolute `mlx_lm.generate` executable and offline model directory.
The prompt travels only on stdin; inherited environment and telemetry are
cleared, stdout is bounded, stderr is discarded, and cancellation, timeout, or
emergency pause reaps the dedicated process group before suppressing output.
This is local LLM execution, with frontier cloud review remaining selective;
it is not a bundled helper, unattended background service, general production
distributed-job runtime, repository executor, or Codex/Git path.

The approved Durable Feature Conveyor now has a default-inert schema-v5
repository kernel in the authoritative Windows `jarvis-master`: immutable
digest-bound specifications, a 100-nonterminal-item owner-ordered queue,
dependency blocking, compare-and-set revisions, one active lease, exact
lifecycle advancement, cancellation without advancement, explicit safe
abandonment, and restart quarantine with atomic redacted audits. It has no
HTTP/API, coding worker, repository execution, Codex/review transport, GitHub
publication, Mac queue UI, or autonomous activation; see
[Durable Feature Conveyor Design](docs/feature-conveyor-design.md).

Fixture
enrollment uses a second exact Keychain identity
profile selected only by `--identity-profile fixture`; the original standard
identity namespace and default monitor/relay commands remain unchanged. The
fixture profile rejects MLX or mixed capabilities before key creation or TLS.
The app clears helper environment and passes that exact selector as a child
argument only with the existing fixture opt-in. The
Windows-local bearer-authenticated master exposes exact-body
`POST /v1/development/emergency-pause/activate` and
`POST /v1/development/emergency-pause/resume` actions. They mutate the live
master only on loopback, are absent from the enrolled-device mTLS router, and
cannot accept planning or enqueue payloads. Pause durably cancels already
leased fixture or MLX work so resume cannot make an old result acceptable again. The
named owner-device proof is
`./scripts/mac-windows-bridge-live-e2e.sh --run-relay`, which requires the same
durable stream to advance across a fresh app/helper/agent chain. The
separate live fixture proof is
`./scripts/mac-windows-bridge-live-e2e.sh --run-fixture`. It coordinates only
fixed Windows-local enqueue/pause/resume actions, verifies success,
cancellation/late-output suppression, same-cursor restart, and standard-profile
preservation, and emits a payload-free receipt. It does not self-approve or
grant remote enqueue/pause authority. The separate live MLX proof is
`./scripts/mac-windows-bridge-live-e2e.sh --run-mlx`; it requires the explicit
runtime/model inputs and uses `scripts/windows-mlx-live-control.ps1` for one
real completion plus pause-dominated cancellation, seven-second late-output
suppression, and same-cursor restart. Its payload-free receipt proves only this
two-device local-inference boundary. The
accepted target and migration boundaries are recorded in
[Distributed Developer Mode Design](docs/distributed-developer-mode-design.md).
The protocol seam has a named serialized contract E2E. The master kernel has a
file-backed fake-worker lifecycle E2E covering durable enqueue/lease/result,
wrong-lease and duplicate denial, cancellation, expiry, capability-specific
bounds, connection loss, late output rejection, restart abandonment, and safe
reissue. `master_process_e2e` additionally starts real child processes, proves
exclusive state ownership, bearer non-disclosure and unauthorized rejection,
oversized-body denial, one authenticated loopback fake-worker job, and restart
reconciliation. The fixture adapter and remote mTLS tests additionally prove the
same exact Public synthetic contract, authenticated-device result binding,
default-off agent admission, bounded cancellation acknowledgement, expiry, and
late-result rejection. Repository tests remain process/transport proof; live
cross-device fixture evidence requires a separately enrolled
`fixture.reasoning` device and must not be inferred from the current
`mlx.reasoning` enrollment.
`enrollment_identity_e2e` proves DPAPI round-trip protection on Windows,
digest-only grant persistence, strict stdin issuance, signed-CSR verification,
expiry and replay denial, rotation, revocation, the 16-device ceiling, and the
schema-v1-to-v4 migration. `event_cursor_e2e` proves durable, contiguous,
metadata-only events plus disconnect/requeue reconciliation.
`remote_mtls_e2e` provisions that real Windows
identity, starts the real master child process, negotiates TLS 1.3 mutual
authentication, denies pre-handshake health, proves exporter-bound health, channel-exporter replay denial,
monotonic reconnect epochs, socket-close reconciliation, revoked-certificate
denial, removal of remote raw-step enqueue, fixture-capability lease admission,
authenticated-device result binding, MacBridge-only event retrieval without
step-context leakage, and inference-worker event-route denial. It is
loopback cross-process transport proof with generated test clients,
not private-overlay reachability or a live Mac enrollment exchange.
`DeveloperBridgeTests` adds the Mac-side contract proof: exact secret-free
invitation and CSR documents, Keychain staging/install fail-closed seams,
exporter-bound handshake encoding, registry-revision matching, and channel
cancellation on missing binding or rejected acceptance. The production
Keychain and Network.framework adapters are compiled on macOS; a separate
owner-run two-device ceremony is still required to claim live Tailscale and
Keychain/mTLS evidence.

Focused Mac bridge commands are:

```sh
swift test --disable-sandbox --package-path apps/mac --filter DeveloperBridgeTests
swift run --package-path apps/mac jarvis-mac-bridge status
swift run --package-path apps/mac jarvis-mac-bridge connect
swift run --package-path apps/mac jarvis-mac-bridge monitor --samples 2
swift run --package-path apps/mac jarvis-mac-bridge monitor --samples 2 --reconnect-between-samples
./scripts/mac-windows-bridge-live-e2e.sh --check
./scripts/mac-windows-bridge-live-e2e.sh --run-outage
```

After the signed helper is built and enrollment is installed, a development
launch may opt in the app-owned monitor lifecycle without sharing bridge
Keychain authority with `Assemblywright.app`:

```sh
BRIDGE="$PWD/apps/mac/.build/jarvis-mac-bridge-signed/Build/Products/Debug/jarvis-mac-bridge.app/Contents/MacOS/jarvis-mac-bridge"
TEAM="${JARVIS_APPLE_DEVELOPMENT_TEAM:?set the independently trusted 10-character Apple team used to build the bridge}"
JARVIS_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE" \
  JARVIS_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$TEAM" \
  swift run --package-path apps/mac JarvisMacApp
```

The complete secret-free Windows/Mac pairing ceremony is documented in
`docs/build-test-commands.md`. Enrollment documents are accepted only on stdin;
the CLI has no grant-secret argument or environment-variable path.
The live two-device harness also invokes the signed helper through the
production app lifecycle, including static and running-PID code validation,
strict snapshot parsing, visible connected state, and bounded child reaping.
After owner enrollment, `./scripts/mac-windows-bridge-live-e2e.sh --run` is the
repeatable live-device E2E. It proves Tailscale reachability, the exact installed
Keychain identity, TLS 1.3 mTLS plus exporter-bound application acceptance,
authenticated health, a positive one-shot connection epoch, and two monitor
samples on one separate persistent connection. A second bounded diagnostic
closes its first accepted session and requires the next handshake to advance
the Windows connection epoch. Receipts forbid secret and raw maintenance
fields. This is owner/device evidence, not a hermetic CI test, induced network
failure, or proof of unattended app background operation.
The explicit `--run-outage` mode adds a coordinated live Windows-service
stop/start. It emits fixed stop/start-required markers so the owner can control
the enrolled service from the Windows console, and succeeds only after the
production Swift lifecycle records Connected, then Master Offline with a fixed
redacted error, then Connected on a strictly higher authenticated epoch. It is
service-outage recovery evidence, not a Tailscale adapter outage or long-run
unattended proof.
The explicit `--run-fixture` mode additionally requires a separately enrolled
fixture-only Keychain profile. At its fixed markers the owner runs
`scripts/windows-fixture-live-control.ps1` on Windows for the exact success,
delayed cancellation, pause, and resume actions. The harness accepts success
only after strict sanitized Windows receipts bind the exact task/step event
kinds and sequences to the same metadata stream, and the Mac agent cursor
reaches those exact sequences. Cancellation includes a seven-second
post-terminal observation rejecting late or duplicate task events, followed by
a complete page drain to a durable head observed after the deadline. Unexpected
same-task kinds and cursor regression fail closed. The cursor
then survives a fresh app/helper/agent chain, while the stable standard profile
projection remains unchanged and freshly reauthenticates. Windows certificate
revocation and confirmed
`jarvis-mac-bridge enrollment remove --identity-profile fixture --confirm`
remain separate cleanup steps. This receipt is live synthetic diagnostics
evidence, not MLX inference, repository/Codex/Git authority, unattended
reliability, signing/notarization, or release readiness.
The explicit `--run-mlx` mode uses the enrolled standard `mlx.reasoning`
identity. At its markers, the owner runs
`scripts/windows-mlx-live-control.ps1` for success, combined long
cancellation-and-pause, and resume, then pastes only each sanitized JSON
receipt into the Mac harness. Combining lease observation and pause on Windows
removes operator round-trip timing from cancellation evidence.
Success requires exact event ordering and stream binding, the durable Mac
cursor reaching each terminal sequence, exact expected-device and per-attempt
connection-epoch binding, seven seconds without a late or
duplicate cancelled-task event, and same-stream restart. It proves live local
MLX execution and cancellation only—not model quality, repository/Codex/Git
authority, OS sandbox containment, unattended reliability,
signing/notarization, or release readiness.
`windows_service_lifecycle_e2e` installs a unique temporary real SCM service on
the Windows CI runner, proves automatic-start configuration, starts the master
under LocalSystem, checks runtime health, proves maintenance admission denial
survives recovery restart, resumes and completes work, directly verifies
stop/status/start health transitions, then uninstalls while preserving master
state. A separate ignored elevated unit proof grants and enumerates the exact
owner-account service-logon right. Both are required explicitly by the Windows
gate; they do not prove the supplied owner password, remote mTLS under that
account, OS hardening, upgrades, backup/restore, or live cross-device
reliability.

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
summary, redacted memory retention-plan review, explicit bounded local-memory
context opt-in, memory review counts in diagnostics and permission policy review, provenance-aware
permission/grant inspection, permission policy review items, redacted scheduler
attention summaries for app handoff, scheduler trigger policy-review items,
release-readiness blocker inspection,
read-only signed-distribution/live-device/plugin-trust runbook rendering,
adapter-backed scheduler notification controls, degraded-mode handling,
Speech/AVFoundation voice input controls, AVFoundation speech-output controls,
and a core supervisor abstraction. A native `MenuBarExtra` keeps Assemblywright
reachable when its main window is closed, reflects the shared supervisor's
stopped/starting/available/degraded state, reopens the existing SwiftUI shell,
and routes refresh/start/stop through the same app-owned models. This local
contract does not replace signed-app, Finder/LaunchServices, or live-device UI
validation.
Scheduler due execution records a redacted proactive policy audit before
submitting commands, using the same trigger classification as
`/permissions/policy-review`, so proactive routines stay inspectable without
exposing scheduler command text.
Operators can also run `assemblywright scheduler recover-stale` to mark persisted
stale `Running` scheduler jobs failed with redacted audit evidence after a
crash or killed process leaves a job leased but unfinished.
`assemblywright serve --scheduler-recover-stale-on-startup` offers the same recovery as
an explicit startup option and marks the audit payload with
`automatic_recovery: true` while keeping scheduler command text redacted.
The packaged Scheduler tab now exposes the same capability as a persisted,
default-off app-supervised automation setting. Applying it deliberately
restarts the supervised core with fixed bounded loop/recovery arguments. While
enabled, a cancellable coordinator refreshes redacted attention and delivers
notifications only if permission was already granted. Rust first writes each
due, failed, or pause-blocked occurrence to a bounded schema-v14 outbox; Swift
uses a stable occurrence-revision identifier and acknowledges only after
notification-center submission or explicit no-authorization suppression. The
handoff is durable and at-least-once: a crash before acknowledgement may repeat
the stable request, as may a concurrent app consumer. Failure escalation uses a
new occurrence revision and may intentionally produce a later notification. It
never prompts automatically or claims notification
display, LaunchAgent, or OS-wake service.

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
`JARVIS_CHATGPT_REASONING_EFFORT`, `JARVIS_OPENAI_BASE_URL`, and
`JARVIS_CHATGPT_TIMEOUT_MS`. The Codex account
path uses a logged-in Codex CLI instead: run `codex login --device-auth`, then
launch with `JARVIS_CHATGPT_ENABLED=true`,
`JARVIS_CHATGPT_AUTH=codex_account`, and optional `JARVIS_CODEX_EXECUTABLE`,
`JARVIS_CHATGPT_MODEL`, `JARVIS_CHATGPT_REASONING_EFFORT`, and
`JARVIS_CHATGPT_TIMEOUT_MS`. Set `JARVIS_CHATGPT_REQUIRES_APPROVAL=false` to
send normal conversation without a repeated prompt; Private and
Credential-adjacent commands still require one-shot approval, proactive cloud
commands still cannot use that grant, and Restricted content remains blocked.
Route policy still
blocks restricted data and sends only redacted route context. Provider failures
return failed command responses with redacted diagnostics instead of becoming
IPC transport errors. Provider text responses may also use a strict JSON
envelope with `message`, `complete`, and `tool_requests`; accepted tool
requests still pass through the existing schema, policy, approval, and audit
path. ChatGPT/OpenAI-compatible responses may also return native
OpenAI `tool_calls` for the advertised first-party tool definitions; those are
translated into the same bounded first-party path. Plain text remains
supported. `/tools/model` and `assemblywright tools list` expose the redacted default
first-party catalog; an opted-in command derives its additional installed WASM
catalog at execution time rather than broadening that default inspection
surface. That extension is deterministic and capped at 16 actions, 1 KiB per
description, 16 KiB per input schema, and 64 KiB combined. Local-model
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
`Codex account`: both disable the local provider for the app-supervised core.
The `Ask before every cloud prompt` toggle controls
`JARVIS_CHATGPT_REQUIRES_APPROVAL`; it defaults off for normal conversation
without weakening sensitive-route, proactive, tool-action, or Restricted-data
gates. The Codex-account surface offers the current installed account-model
choices plus model-compatible reasoning-effort choices, propagated through
`JARVIS_CHATGPT_REASONING_EFFORT`. `OpenAI API` stores the
application credential in Keychain instead of SQLite or docs, while `Codex
account` shells through the logged-in Codex CLI and does not require an OpenAI
Platform API key. Assemblywright launches that subprocess from a temporary directory,
passes the redacted prompt over stdin, ignores user configuration and project
rules, fixes approval policy to `never`, requests the CLI's read-only sandbox,
uses strict config with web search disabled, mechanically disables the current
CLI tool/integration feature set (including shell, unified-exec, code-host,
apps/plugins, browser, computer use, image generation, multi-agent, and
workspace dependencies), forwards only the small environment allowlist needed
for account auth and networking, discards child logs, monitors and kills the
child if its private final-message file crosses 1 MiB, and repeats the size
check before reading. The
request payload contains only Assemblywright's redacted route context, but Codex still
adds its own runtime/system context. A CLI that does not support the constrained
argument contract fails closed before model execution; update the bundled
Codex/ChatGPT app or CLI before retrying. Use `OpenAI API` when a non-agentic
HTTP provider boundary is required.
When an enabled cloud route still requires approval, the macOS Console presents a
one-shot `Approve & Send` action for the blocked prompt. Approval is carried on
the authenticated retry, audited by the Rust route policy, is ignored for
proactive commands, and is not reusable by later prompts. Restricted content
remains blocked from cloud routing.
Plugin availability for model planning means the `/tools/model` first-party
catalog by default. `assemblywright tools list`, `assemblywright tools model`, and
`assemblywright tools catalog` all print that same default catalog. A command can pass
`--installed-wasm-tools` (the Swift console has the matching toggle) to add only
currently eligible installed `local_wasm` actions after a reactive local-model
route is selected. The flag defaults false, never applies to ChatGPT/cloud or
proactive commands, and never admits `local_subprocess`. Chrome/browser-extension
capabilities remain unavailable unless they are registered first-party tools.
Production inventory excludes the deterministic `fake_*` test fixtures. It
always includes the bounded metadata-only `system_status.status` action. The
macOS app owns durable, user-selected workspace grants as security-scoped
bookmarks and hands their opaque IDs plus resolved paths to the supervised core
through one bounded startup-stdin envelope. The legacy repeatable
`assemblywright serve --workspace-root <id>=<absolute-path>` operator option remains
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
App-supervised launches also rotate a 32-byte bearer and default to a
generation-random Unix domain socket. The bounded startup-stdin envelope carries
both the bearer and `ipc_transport:{kind:"unix_socket_peer_identity_v1",
socket_path:"/absolute/path.sock",peer_code_requirement:"...",
peer_identity_profile:"adhoc_exact|developer_id_hardened"}`; neither enters
child argv or environment. The app-owned runtime directory is current-owner
`0700` and the socket is `0600`. Both Swift and Rust obtain the connected
peer's audit token with `LOCAL_PEERTOKEN`, validate its running code through
Security.framework against the expected designated requirement, and retain the
current-EUID check. Rust performs identity checks before reading a frame, and
every route still requires the bearer.

Distribution packaging gives the bundled core the stable code identifier
`com.nobiletechnology.jarvis.core`; the app keeps the fixed
`com.nobiletechnology.jarvis` bundle identifier. Alternate package identifiers
are rejected. Ad-hoc `cdhash` requirements bind only the exact local
build and do not establish publisher trust. Developer ID mode requires the
stable app/core identifiers, Apple-generic anchored Developer ID Application
requirements, the same nonempty team identifier, and hardened-runtime flags.

The UDS protocol allows one four-byte big-endian length plus one strict,
versioned JSON request per connection, followed by a required client write-half
close before the framed response. It permits only GET, POST,
DELETE, and PATCH, uses standard padded base64 for bodies, and fails closed on
unknown fields, malformed frames, bounds, deadlines, or concurrency limits.
Stop, failure, replacement, and observed exit clear the matching launch state;
cleanup removes only a validated socket leaf, never a directory tree.

Exact `JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true` switches the app to the weaker
authenticated loopback TCP and owner-only
`~/Library/Application Support/Jarvis/ipc-session-auth.json` compatibility
path. `JARVIS_MAC_IPC_AUTH_FILE` can override that file with an absolute path
only in this mode. An operator can then run, for example,
`assemblywright --ipc-token-file "$HOME/Library/Application Support/Jarvis/ipc-session-auth.json" health`.
The token never enters child argv, environment, audit, diagnostics, or UI.
Loopback clients and servers retain strict loopback checks, and an explicitly
unauthenticated legacy server rejects any Authorization header. Repository
tests prove bounded transport, audit-token requirement enforcement, same-EUID
checks, bearer possession, route parity, cleanup, and compatibility behavior.
The ad-hoc lane proves exact-build identity mechanics only. It does not prove
Developer ID publisher identity, device authentication, XPC, App Sandbox,
notarization, or live-device behavior.
For broader registered plugin manifest inspection, `assemblywright plugins list`
defaults to a compact operator-readable summary and `assemblywright plugins list --json`
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
boundary. They may emit bounded `jarvis_progress` JSON frames on stderr; Assemblywright
exposes only parsed sequence/stage/message progress events in the run response
and audit log, while raw stderr stays redacted. Publisher-origin claims can be
operator-pinned only
after local provenance matches the install snapshot; this is an auditable
local review step. Manifests can also carry an Ed25519 publisher signature,
which Assemblywright verifies only against an explicit trusted public key after local
provenance matches; this is not marketplace trust or malware analysis.
Network-capable plugin actions must declare exact allowed hosts in the manifest
and appear in policy review; Assemblywright also fails closed unless executable
network-capable installed plugins are enabled with `subprocess_stdio_network`.
This is runtime grant gating and manifest governance, not an OS-level network
sandbox or host-level egress filter.

New local plugin installs require valid SemVer 2.0.0. For an already installed plugin, `assemblywright plugins update-preview <id>
/absolute/path/to/jarvis-plugin.json` performs redacted validation without a
mutation and prints `current_lifecycle_contract_sha256` plus
`candidate_update_contract_sha256` for review. Apply them without refreshing:
`assemblywright plugins update-apply <id> /absolute/path/to/jarvis-plugin.json
--expected-lifecycle-contract-sha256 <64hex>
--expected-candidate-update-contract-sha256 <64hex> --confirm`. Success resets
execution to disabled `metadata_only`. `assemblywright plugins history <id>` prints the
bounded redacted lifecycle projection. These commands require a running
repository-backed core. The candidate token is opaque aggregate integrity data,
not a raw component provenance hash or trust signal.
A persisted pre-SemVer record may make one fully governed transition to valid
SemVer; all later updates are strictly ordered by SemVer precedence.

Installed `local_wasm` plugins provide a narrower compute-only alternative.
They require the explicit `wasm_compute` grant and the custom
`jarvis_json_v1` exports `memory`, `jarvis_alloc`, and `jarvis_run`. Assemblywright
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
Runs may carry a unique `--cancellation-id`; `assemblywright plugins cancel-run <id>`
requests cooperative cancellation through local IPC. Wasmi checks it between
fuel slices and before accepting output.

Direct installed-plugin runs pass the same permission policy used by the rest
of the runtime. Contract dry runs remain non-executing, and eligible `low` risk
requests at the default `workspace` sensitivity can still execute directly.
`confirm` actions or more-sensitive invocations return `approval_required`
without entering Wasmi or starting a subprocess. `assemblywright plugins run-installed`
accepts `--sensitivity` so this policy input is explicit.

Model-planned use of this runtime is default-off. When a reactive local-model
command explicitly sets `installed_wasm_tools`, Assemblywright derives a redacted
catalog only from enabled `wasm_compute` records with current exact-byte
provenance and eligible action schemas, rejects first-party identifier
collisions, and repeats those checks immediately before guest execution. The
catalog and audit surfaces omit module bytes, paths, hashes, inputs, outputs,
publisher material, and subprocess configuration. Cloud/proactive routes and
all installed subprocess plugins stay excluded.

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
`/release/readiness`, `assemblywright release readiness`, and the Swift Release tab
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
external evidence: Assemblywright does not itself perform Developer ID signing,
notarization, stapling, clean-profile install/Finder validation, live-device
QA, plugin trust QA, or manual release QA.
Only start or restart the core with
`JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external` after owner-recorded external
QA reports and signed-distribution evidence have been collected, then query
`assemblywright release readiness` against that running core.
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
The Plugin tab also supports an explicitly selected local replacement manifest
and bounded redacted lifecycle history. A replacement is untrusted input: it
must match the installed plugin identity and pass manifest/provenance
validation. Preview returns the reviewed `current_lifecycle_contract_sha256`
and opaque `candidate_update_contract_sha256`; confirmed apply preserves that
exact pair, reloads the candidate, and rejects lifecycle or snapshot drift before
Assemblywright captures a new snapshot and resets execution to
disabled `metadata_only`. The operator must verify the new snapshot and
explicitly re-enable a compatible grant. This local workflow is not a
marketplace, publisher-trust verdict, malware analysis, OS sandbox, host-egress
policy, or plugin-trust QA.

The permission center now surfaces installed-plugin provenance status from
`/permissions/grants`, including unverified plugin counts and local integrity
state. `/permissions/policy-review` turns pending approvals and installed
plugin provenance/grant/network concerns plus unreviewed memory items and
deleted sensitive memory retained in local storage into explicit review items
without exposing memory values. `/memory/retention-plan` and
`assemblywright memory retention-plan` now provide the memory-specific, redacted
operator action list for active unreviewed memory and deleted sensitive memory
that is still retained locally; the plan is inspection-only and keeps
automation disabled. The Swift Memory tab renders the same redacted plan above
the item list. `/memory/index/status`, `/memory/index/rebuild`, the matching
`assemblywright memory index-*` commands, and the Swift Memory tab now govern an atomic,
versioned projection manifest rebuilt only from active canonical SQLite rows.
Public status is count-only. `assemblywright command --memory-context` and the disabled-
by-default Swift console toggle can use the current projection as a fail-closed
gate for deterministic local lexical retrieval. Retrieval is non-proactive and
local-model-only, admits reviewed active Public/Workspace/Personal records,
caps query/corpus/results/context, and frames ephemeral context as untrusted
data. Cloud adapters reject it before transport, and audit/route evidence never
contains memory values. These surfaces still do not grant broader marketplace
trust, malware safety, autonomous memory rewriting, vector/semantic retrieval,
purge automation, or OS-level network sandboxing.
Approval grant/deny decisions remain side-effect-free and atomically commit the
decision with a redacted decision audit in one immediate transaction. Audit
failure rolls the record back to pending so no unaudited grant chain can reach
execution authority; free-form actor and reason stay out of audit payloads.
Approved first-party and installed-plugin approval records require a separate
one-shot `/approvals/:id/execute` or
`assemblywright approvals execute <approval-id>` replay, which verifies the original
task, action, risk, scopes, input schema, current policy, and matching
approval_granted audit evidence before schema v13
atomically records a unique durable execution claim and redacted claim/policy
audit evidence. For installed-plugin invocations, schema v15 also stores a
private approval binding for the canonical input plus the exact manifest,
provenance, and execution-grant contract. The approval-required run response,
approval record, audit, and diagnostic surfaces omit the bound input and both
binding digests; successful execution output remains governed by the declared
output schema and existing response contract. Execution revalidates the binding
before it claims authority. Only the claimant invokes
the plugin; a duplicate or restarted
replay fails with conflict/HTTP 409. Terminal execution state, task state, and
terminal audit evidence commit together. Once claimed, an approval is consumed:
failure, cancellation, timeout, restart, or a persistence interruption can make
the effect ambiguous, so automatic retry is forbidden. Inspect the audit trail
and create a new approval when another attempt is appropriate. On core restart,
schema v16 projects any pre-existing unresolved claim into the redacted
`/approval-executions/attention` queue before serving IPC. The queue exposes
identifiers, timestamps, revision, and fixed effect/retry/redaction booleans,
never the action, approved input, reason, actor, or provenance digests. The
summary reports the true outstanding count separately from the bounded 100-item
page and explicitly marks truncation, so a large recovery backlog is never
understated. CLI and
the Swift Approval Center can acknowledge an observed revision explicitly with
`acknowledged_without_retry`; the CAS records review but never invokes a plugin,
changes or deletes the permanent claim, or creates another approval. The Swift
Approval Center suppresses duplicate submits and hides approvals that have
either claim or terminal execution evidence while presenting unresolved claims
in this separate recovery queue. Exact legacy raw-metadata audit
evidence remains compatible when its approval ID, task, action, status, policy
metadata, actor/reason, and non-execution fields match; missing or unrelated
evidence cannot be substituted and Assemblywright never fabricates a grant audit.
File-backed SQLite startup first acquires and retains a nonblocking exclusive
lease on the sibling owner-only `.owner.lock`. The lock is opened without
following symlinks, must be a regular single-link current-owner `0600` file,
and serializes backup, version inspection, migration, and the repository
lifetime. A second core fails before database open instead of racing recovery.
This lease coordinates cooperating Assemblywright repository owners only; it does not
OS-block a raw or noncooperating process from opening the SQLite file directly.
Current CLI and Swift clients attach a fresh `cancellation_id` to approved
execution. While that exact claimed run is active,
`POST /runtime/cancellations/:id` binds cancellation to its approval task,
discards late output at the acceptance boundary, and durably terminalizes the
claim/task as cancelled. This is cooperative and cannot reverse an external
effect that already occurred. The Approval Center retains the same handle while
Run Approved is active and replaces that control with Cancel Run.

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

For the explicit operator CLI compatibility path, start the local loopback
server and run CLI commands from a second terminal. This is not the default
app-supervised UDS path:

```sh
cargo run -p jarvis-cli -- serve
cargo run -p jarvis-cli -- health
```

`assemblywright health` and strict IPC commands such as `assemblywright command`,
pause/resume, scheduler, task/audit/activity/route, memory, approval,
diagnostics, installed-plugin, and permission-center operations require a
running core server. If the server is down, they exit non-zero with guidance
to start `assemblywright serve`, run the offline ephemeral `assemblywright smoke` check, or use
read-only fallback inspection commands such as `assemblywright release readiness`,
`assemblywright plugins list`, and `assemblywright tools list`.

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
explicit upgrade diagnostic, and representative schema v1-v13 fixtures preserve
critical rows through the current migration path. It does not replace installer
upgrade QA.

For operator-facing release QA over a repository-backed local core, run:

```sh
./scripts/release-operator-qa-smoke.sh
```

That script starts an explicit loopback compatibility core with an isolated
SQLite database and exercises command, audit, routes, memory
create/update/review/delete/restore, scheduler
attention/run-due, activity, permission review, diagnostics, emergency pause,
release readiness, and restart recovery. It is local CLI QA evidence, not a
clean-profile installed-app or live-device validation pass.

For branches that touch Swift supervision or core binary discovery, run the
focused packaged-supervision proof:

```sh
./scripts/packaged-supervision-proof.sh
```

That script builds `jarvis-cli`, places it in a temporary
`Assemblywright.app/Contents/Resources/bin/` layout, runs the Swift coverage against
the configured packaged-style executable, and runs `assemblywright smoke`.
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
versioned artifact names. The CLI also exposes `assemblywright --version`, and the
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
the distribution-shaped `Assemblywright.app`, optionally ad-hoc signs it when
`codesign` is available, creates an unsigned `/Applications` installer package,
and inspects the package payload for the app executable, bundled core, and
`Info.plist`.
The unsigned launch check additionally launches the release-built app
executable with an isolated temporary HOME, requires a non-secret app-only
marker emitted only after the Swift client completes authenticated health,
dry-run command, task/audit inspection, diagnostics, pause, blocked-command,
and resume checks over the default UDS, and verifies the resulting SQLite state
before socket cleanup. While that UDS is live, a same-EUID Python process with
the wrong code identity sends a valid-shaped request and must be closed or reset
before it can receive a framed `401`; the legitimate Swift route must remain
healthy. The ad-hoc app/core identifiers and designated requirements are also
checked. A separate explicit compatibility relaunch preserves the loopback
TCP/token CLI lane. It is also part of the default
`./scripts/release-local.sh` local release gate so distribution-layout launch
regressions fail the standard proof path.
The full `package-distribution.sh` lane owns Developer ID signing,
notarization, stapling, microphone entitlement packaging, and signed installer
package creation when Apple credentials are provided. It now also writes a
`Assemblywright-<version>-signed-provenance.json` report with signing identities,
notary submission IDs/log paths, staple validation output, Gatekeeper
assessment output, bundled core `assemblywright --version` output, and artifact
SHA-256 digests for the signed zip/pkg. The report also records the exact app
executable path/SHA-256 and its structured code Identifier, TeamIdentifier,
and CDHash, plus the bundled
`Contents/Resources/bin/jarvis-cli` path and SHA-256 digest. It
also asserts the stable app/core code identifiers. This static signed-artifact
evidence does not by itself exercise the Developer ID peer-identity route and
still does not replace notarization outcome, clean-profile installer run,
Finder launch, App Store validation, or live microphone/Speech/audio-output
validation.
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
The assertion requires the signed-provenance report and revalidates the
installed app executable with `codesign`, stapler, and Gatekeeper. Its
executable SHA-256, code identifier, TeamIdentifier, and CDHash must match the
signed candidate; the live report records that identity and the exact
signed-provenance path/SHA-256 for final cross-report validation.
The assertion requires explicit transcript handoff validation, structured
spoken-command observation fields with the installed app path matching the
expected `/Applications/Assemblywright.app` path, unless explicitly overridden with
`JARVIS_QA_INSTALLED_APP_PATH`, the observed transcript matching the spoken
test phrase, and expected command text matching observed command text,
`JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID` set to `task:<uuid>` or `audit:<uuid>`
from the live command/audit evidence, owner/device/profile, ordered UTC
timestamps, non-voice owner evidence notes for clean-profile, Finder launch,
notification, restart, and manual QA, voice evidence-note fields, and a bundled-core binding for the
installed `Contents/Resources/bin/jarvis-cli` path, `assemblywright <version>` output,
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
`NSMicrophoneUsageDescription` is `Assemblywright uses microphone input only when you explicitly start local voice capture.`, and
`NSSpeechRecognitionUsageDescription` is `Assemblywright uses speech recognition only to turn your spoken command into a local assistant request.`. The
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
`assemblywright release readiness --all-commands` includes the matching
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
validated. `assemblywright release readiness --all-commands` includes the matching
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
build version, voice permission usage strings, and exact executable code
identity to the installed app and signed provenance. Unlike
doctor/status inventory, the real `--bundle` path also validates the signed
provenance report, local app signature, app stapling ticket, installer signature, installer stapling ticket,
and app zip payload, then records SHA-256 digests for distribution artifacts and
QA reports before writing the manifest.
The full readiness runbook is ordered for release execution: local gates and the
unsigned launch check first, the Developer ID packaging/notarization command
before live-device QA evidence capture, plugin-trust QA before final bundling,
the evidence doctor assertion after both bundle paths, and the external
evidence-mode readiness check last.

With a repository-backed server running, `assemblywright release readiness`,
`assemblywright release evidence-status`,
`assemblywright tasks`, `assemblywright routes`, `assemblywright memory`, `assemblywright activity summary`,
`assemblywright activity watch`, `assemblywright scheduler`, `assemblywright diagnostics`, and
`assemblywright plugins` expose the current readiness evidence, durable state, status
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
The diagnostics health projection is intentionally distinct from `/health`:
it exposes pause state, update time, and `emergency_pause_reason_present`, but
the legacy reason field is only null or the fixed `redacted` compatibility
marker, never arbitrary emergency-pause reason text. Explicit health, pause, and
pause-status commands retain that reason for deliberate operator inspection and
must not be handled as shareable diagnostics exports. Core sentinel tests,
real-server CLI E2E, and Swift decode/presentation tests enforce the split.
Evidence-status items report present/missing/invalid inventory. Artifact paths
are presence-only checks except the app bundle, whose `Info.plist` bundle id,
short version, and build version must match the expected release metadata. JSON
reports receive semantic validation for signed-distribution provenance
version/bundle metadata, bundled core path/version/SHA-256 binding,
signing/notary/staple and Gatekeeper fields, required flags, SHA-256 digests,
signed-provenance zip/pkg/core digest matches against current artifact files,
signed-provenance app-executable digest/Identifier/TeamIdentifier/CDHash
matching against live-device QA, live-device QA metadata, plugin-trust non-future timestamps plus
`review_source: owner-asserted-manual-review`, and final bundle
path/digest semantics, child-report semantic validity, and
`validation_flags.local_signature_validation`. The status surface still does not
perform signing, notarization, stapling, installation, or manual QA.

## Docs

- [Brand system](docs/brand.md)
- [Architecture map](docs/architecture-map.md)
- [Production readiness sweep - 2026-06-11](docs/production-readiness-sweep-2026-06-11.md)
- [Production readiness sweep - 2026-06-10](docs/production-readiness-sweep-2026-06-10.md)
- [Plugin contract](docs/plugin-contract.md)
- [Safety rules](docs/safety-rules.md)
- [Build and test commands](docs/build-test-commands.md)
- [Release checklist](docs/release-checklist.md)
- [Knowledge-base facts](docs/knowledge-base/assemblywright-project-facts.md)

The architecture map includes both the implemented current-state diagram and
the end-goal production diagram, plus a phase table that separates verified
foundation work from future production assistant requirements.

## License

Assemblywright is licensed under the
[Apache License 2.0](LICENSE). Internal `jarvis-*`, `JARVIS_*`, Keychain,
storage, protocol, and code-signing identifiers remain compatibility contracts;
they do not represent the product name.
