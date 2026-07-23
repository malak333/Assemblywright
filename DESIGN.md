# Jarvis Design

## Understanding Summary

- Jarvis is a local-first Mac desktop assistant inspired by cinematic AI assistants, without copying Marvel branding, names, exact UI, or copyrighted visuals.
- Version 1 is a unified assistant shell, not a deep integration product yet.
- The core priorities are voice-first interaction, local model routing, memory, permissions, audit logs, proactive routines and triggers, and a plugin architecture.
- Jarvis should eventually support personal productivity, developer-agent workflows, and home/life automation, but v1 focuses on the foundation.
- Privacy posture is local-model first, with ChatGPT as the only approved cloud model.
- Autonomy target is high, bounded by capability scopes, risk tiers, audit logs, cancellation, and an emergency pause control.
- v1 is single-user only, but should be product-grade and maintainable by the user plus agents.

## Assumptions

- The first product target is a native-feeling macOS desktop app.
- Voice latency matters: Jarvis should acknowledge quickly and move longer tasks into background execution.
- Product-grade v1 includes packaging, diagnostics, migrations, durable local state, and release discipline.
- The UI can be polished and high-tech, but must remain practical, inspectable, and legally distinct from Marvel/JARVIS assets.
- Smart-home control, autonomous external communication, and multi-user sync are deferred or heavily gated in v1.
- ChatGPT usage is explicit, routed, minimized, policy-checked, and audited.
- Production-readiness claims are evidence-scoped. The repo now has local
  packaged-app, unsigned distribution-layout, release-smoke, permission UX,
  voice-adapter, recovery, diagnostics, and release-evidence mechanics, but a
  green local foundation gate is not the same as finished assistant readiness
  until Developer ID signing, notarization/stapling, clean-profile install and
  Finder launch, live voice/audio/notification QA, plugin-trust QA, and the
  final evidence bundle are owner-recorded and archived for the claimed
  surface.
- Live-device QA can clear voice readiness only when owner-recorded report
  fields pass semantic validation and, for repository-backed IPC readiness, the
  `command_result_evidence_id` resolves to existing task or task-associated
  audit evidence. The report must also bind the installed app executable's
  SHA-256, code identifier, TeamIdentifier, and CDHash to the exact signed
  provenance report used by the final evidence bundle.
- Architecture docs are release artifacts: keep both current-state and
  end-goal production diagrams aligned with any release-evidence flow change.

## Non-Goals For v1

- No full smart-home control yet. Design the plugin boundary, but avoid controlling real devices in the first core shell.
- No autonomous external communication. Jarvis may draft or prepare actions, but sending messages, inviting people, making purchases, or similar external actions require approval.
- No multi-user account sync. v1 is strictly single-user and local to one Mac.
- No third-party plugin marketplace. First-party plugins come first.
- No cloud-first assistant behavior. Local models are the default.

## Decision Log

| Decision | Alternatives Considered | Rationale |
| --- | --- | --- |
| Build v1 as a macOS desktop app | Web app plus local daemon, CLI/TUI first, cross-platform first | The product goal is a native Mac assistant with strong voice, UI, permissions, and system integration. |
| Use a hybrid Rust core plus Swift Mac shell | Pure Swift, pure Rust, web app plus daemon | Rust is a better fit for durable local agent infrastructure; Swift is a better fit for native macOS UX and Apple integrations. |
| Rust owns the assistant core | Swift-owned agent runtime | Keeps model routing, memory, tools, safety, plugins, scheduling, and audit logs in a portable, testable service. |
| Swift owns the human-facing shell | Rust UI, web UI | Gives the best path to native voice, macOS permissions, notifications, menus, settings, and polished UI. |
| App-supervised core first | LaunchAgent from day one | Reduces v1 complexity while preserving a path to stronger background reliability later. |
| Keep app-supervised scheduler automation explicit and bounded | Always run persisted schedules whenever the app launches, or leave all due execution manual | A persisted user toggle enables the existing audited Rust background loop only while `Jarvis.app` supervises the core. Fixed interval/limit ceilings, bounded stale recovery, cancellable redacted attention polling, and separately authorized notifications preserve user intent without claiming LaunchAgent or OS-wake reliability. |
| Local-model first with ChatGPT as the only approved cloud model | Cloud-first routing, provider-agnostic cloud routing | Matches the privacy posture while allowing explicit escalation for harder reasoning tasks. |
| Support both OpenAI API-key and logged-in Codex-account authentication inside the approved ChatGPT route | API-key only, unaudited general Codex agent execution | Account authentication avoids a second stored Platform key while retaining the same sensitivity gate, route evidence, configured cloud-approval policy, redacted request context, and failure audit. The Codex subprocess must ignore user/project rules, disable tool capabilities mechanically, minimize inherited environment, bound output, and fail closed when its constrained CLI contract is unavailable. |
| Let an operator skip repeated approval for ordinary cloud conversation and select the Codex model plus reasoning effort | Require a one-shot decision for every typed prompt, hard-code one model/effort, or remove cloud safety gates | Explicit cloud-provider selection plus an `Ask before every cloud prompt` control is sufficient for ordinary Public, Workspace, and Personal conversation. Private and Credential-adjacent routes keep command-scoped approval, Restricted routes remain blocked, proactive routes cannot consume the grant, and tool/action approval is unchanged. Model and effort are runtime-verified through health; Codex-account execution passes the selected effort through strict CLI config while its internal approval policy stays `never` and tool features stay disabled. |
| Capability scopes plus risk tiers | Simple allow/deny prompts, risk tiers only | High autonomy needs both explicit permission boundaries and per-action risk evaluation. |
| SQLite as primary structured storage | Flat files only, external database | SQLite is durable, inspectable, easy to migrate, and enough for single-user v1. |
| macOS Keychain for secrets | Store credentials in SQLite or config files | Secrets should use the platform credential store. |
| Keep Developer Mode device keys in device-only Keychain items and pair through a secret-free public exchange | Export a PKCS#12 bundle, persist PEM keys, print the one-time grant, or treat Tailscale membership as device identity | The Windows master keeps the ten-minute grant secret in one confirmed local pairing process while the Mac returns only a signed CSR. The Mac validates and installs the issued certificate against the staged key, invitation identity, endpoint, and CA fingerprint before an outbound TLS 1.3 session can authenticate. Tailscale supplies reachability only; the exporter-bound mTLS handshake supplies Jarvis device authority. |
| First-party plugins first | Third-party marketplace in v1 | The safety model and plugin contract need to prove themselves before third-party expansion. |
| App-owned security-scoped bookmarks for workspace roots | Put root paths in app child arguments, store plain paths, or let the model select roots | Native user selection establishes an explicit local grant; opaque IDs and bounded startup stdin keep app-selected paths out of argv, environment, model input, and audit while Rust remains the descriptor authority. Bookmark tests do not prove App Sandbox enforcement or child sandbox-extension inheritance. |
| App-supervised Unix-domain-socket IPC with Apple audit-token code identity, same-EUID, and per-launch bearer checks | Use loopback TCP by default, rely on socket filesystem permissions alone, persist every supervised credential, trust PID/path lookup, put transport authority in argv/environment, or silently reuse a legacy unauthenticated core | The default app launch creates an owner-only runtime directory and generation-random Unix socket, sends `ipc_transport:{kind:"unix_socket_peer_identity_v1",socket_path:"/absolute/path.sock",peer_code_requirement:"...",peer_identity_profile:"adhoc_exact|developer_id_hardened"}` plus a fresh 32-byte bearer only through bounded startup stdin, and requires `LOCAL_PEERTOKEN`/Security.framework requirement validation, current-EUID credentials, and the bearer before every request. Swift validates the connected core through the same audit-token mechanism. Exact `JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true` selects the explicitly weaker authenticated loopback TCP and owner-only token-file compatibility path. Ad-hoc requirements bind one exact build by cdhash and do not establish publisher identity; the Developer ID profile requires stable app/core identifiers, the same nonempty team, and hardened runtime. This is intended-process defense in depth, not device authentication, XPC, App Sandbox, notarization, or live-device proof. |
| Add a no-import Wasmi compute runtime before broader third-party execution | Treat subprocess grants as sufficient containment, enable WASI, or wait for an OS sandbox | A deliberately small `jarvis_json_v1` ABI can provide useful low-risk local computation while mechanically denying guest filesystem, environment, network, clock, and process authority. Wasmi confinement is a language-runtime boundary, not an OS sandbox or plugin trust system. |
| Commit approval decisions with their redacted audit evidence | Update the approval row and append audit evidence as separate writes, or treat the audit as best effort | A grant without its decision audit creates broken authority provenance, while a denial without evidence makes the safety history incomplete. Grant and denial now use one immediate transaction that rechecks pending state, updates the record, appends a metadata-only decision audit, and rolls everything back to pending if audit persistence fails. No decision path executes a side effect. |
| Consume approved execution authority with a durable schema-v13 claim before plugin entry | Treat an approved row or an `approval_executed` audit lookup as a lock, hold the repository mutex across plugin execution, or retry an interrupted approval automatically | The replay path requires matching `approval_granted` authority evidence, then validates the approved record, still-waiting task, exact action, current risk and scopes, current manifest, input schema, and current policy before an immediate transaction inserts the unique `approval_executions` claim and redacted claim audit. Every compatible audit must match approval ID, task, action, approved status, risk, sensitivity, scopes, `side_effect_executed:false`, and be timestamped at or after the decision. The current redacted shape additionally requires matching actor/reason-presence booleans and forbids raw actor/reason keys. The exact prior raw-metadata shape forbids the redaction/presence keys and requires actor and reason values to equal the record. An approved row without that chain fails before plugin entry and does not fabricate evidence. The claim permanently consumes the approval. Terminal execution state, task state, and terminal audits commit together. A process loss or persistence failure after the claim leaves an ambiguous effect boundary and never authorizes automatic retry; an operator must review the evidence and create a new approval if another attempt is appropriate. |
| Give diagnostics a dedicated redacted health projection | Reuse the full `/health` response inside diagnostics, redact arbitrary reason text by convention, or remove pause visibility entirely | Explicit health and pause-status surfaces retain the operator-entered emergency-pause reason, but `/diagnostics/export` uses a distinct type that can carry only pause state, update time, `emergency_pause_reason_present`, and the fixed `redacted` compatibility marker. This makes accidental raw-reason export structurally unavailable while preserving useful support evidence and the additive v1 response shape. |
| Give every interactive command an optional client-generated cancellation handle | Cancel by connection lifetime, cancel the newest task, wait until a task ID is returned, permit overlapping console submits, or reuse installed-plugin-only cancellation | Swift and CLI generate a bounded UUID before `POST /commands`; the Swift console serializes submissions before mutating its active handle. Rust registers the UUID for the full active request, binds it to only the created task, propagates it through provider and tool cancellation, and uses guard finalization as the result-acceptance linearization point. Authenticated cancellation reports `cancellation_requested` only while that exact handle is active and `not_found` otherwise. The 1,024 most recently consumed UUIDs remain bounded FIFO tombstones to reject recent reuse; clients still require fresh random UUIDs because tombstones are process-local and eventually evicted. Cancellation cannot undo an external effect that already happened. |
| Terminate installed subprocess process groups on cancellation and abnormal exit | Discard only the eventual result, kill only the direct child, or wait for the manifest timeout | Every installed subprocess starts as a dedicated Unix process group. Active cancellation and emergency pause, plus timeout, output-limit, input/output failure, and leader exit, close the invocation by signaling that group, escalating from TERM to KILL after a bounded grace, and reaping the leader before returning. Concurrent stdin/output workers are joined with a bound. This stops members that remain in the group but cannot contain a process that deliberately escapes with `setsid`/`setpgid`, undo an effect already issued, or establish an OS sandbox or egress boundary. |
| Auditability as an architectural requirement | Best-effort logs after the fact | Jarvis must be able to explain why it acted, what data it used, and what permissions were involved. |
| Bind live-device QA to the exact signed app executable and code identity | Treat bundle metadata or bundled-core identity as sufficient, or accept independent valid-looking reports | Signed provenance records the app executable path/SHA-256 plus Identifier, TeamIdentifier, and CDHash. Live-device QA rechecks the installed executable and the signed-provenance report, while final bundle, doctor, and Rust evidence-status validation require the two reports to agree. This prevents artifact mixing but remains point-in-time evidence, not continuous integrity or proof that installation preserved every byte. |

## Architecture

Jarvis v1 runs as two cooperating local processes.

### Jarvis.app

`Jarvis.app` is a Swift/SwiftUI macOS application. It owns the visible user experience, voice session, status controls, permission prompts, notifications, settings, activity history, memory management UI, plugin management UI, diagnostics export, and emergency pause.

The app should feel like a real Mac product: menu bar presence, command surface, settings, clear current activity, and predictable recovery from degraded modes.

The current shell implements that presence with a native SwiftUI
`MenuBarExtra`. It shares the app's existing supervisor and command/model state,
uses a stable scene identifier to reopen the main window, and limits its
lifecycle controls to refresh, start, stop, and quit. It is not a second core
owner or a hidden background agent: core startup remains app-supervised and all
production claims still require the packaged-app and manual release evidence
defined by the release checklist.
The Scheduler tab also owns a persisted, default-off automation toggle. Applying
it restarts only the app-supervised core with bounded scheduler and optional
stale-recovery flags. A single cancellable app-lifetime coordinator polls the
redacted attention projection and a repository-backed, bounded notification
outbox during each accepted active poll, before independently checking macOS
notification authorization. Rust transactionally
records an occurrence before execution, revision-escalates failures and stale
recovery, and requires compare-and-swap acknowledgement after Swift selects
either notification-center submission when already authorized or a suppression
intent when authorization is denied or not determined. Submission means the
adapter returned, not that macOS displayed the notification. This is an
at-least-once handoff, so a crash between submission and successful
acknowledgement or a concurrent consumer may repeat the stable request.
Lifecycle acceptance is
rechecked after asynchronous authorization lookup; the coordinator never
prompts from the background or enables trusted system wake.

### jarvis-core

`jarvis-core` is a Rust local service started and supervised by the Swift app. It owns durable execution: task planning, model routing, memory reads and writes, plugin execution, scheduled jobs, event triggers, risk policy evaluation, and audit logging.

### Developer Mode Mac bridge foundation

The default-inert Developer Mode bridge is a separate cross-device trust
boundary. A confirmed Windows-local pairing command creates a bounded public
invitation and retains the single-use grant secret only in that process. The
Mac generates a non-exported P-256 key in a device-only Keychain item, returns
only a signed CSR, and installs the issued client certificate and enrollment CA
only after their device identity, role, registry revision, key, endpoint, and CA
fingerprint match the staged invitation. Normal local Jarvis startup does not
read this material.

An explicitly invoked outbound bridge connection pins the enrolled CA and
expected master IP, presents the Keychain identity, requires TLS 1.3, derives
the fixed Jarvis TLS exporter, and sends the exact registered handshake on that
same connection. A missing exporter, expired or mismatched certificate,
unexpected trust chain, rejected registry revision, or non-accepted handshake
closes the connection and grants no distributed authority. Tailscale is the
private transport overlay, not an authentication or authorization boundary.
The signed bridge helper can now supervise one persistent authenticated
session, validate bounded remote health exactly, and reconnect with capped
fail-closed backoff while exposing only a redacted state snapshot. An exact,
default-off executable plus independently supplied Apple-team opt-in lets the
Swift app supervise that separately signed helper and render only Disabled, Starting,
Connected, Master Offline, Maintenance, or Stopped state. The app validates the
helper's Apple signature, independently pinned team requirement, exact
executable, and distinct Keychain access group before launch, then revalidates
the running child by PID and prevalidated CDHash before accepting output. It
clears the child environment, bounds the snapshot
queue, uses bounded TERM-to-KILL reaping, and fails closed on duplicate keys,
malformed, oversized, extra, or terminated output. `Jarvis.app`
never reads the bridge identity. This proves the Mac-side development
connection-supervision primitive. The live bridge E2E also
deliberately closes one accepted session and requires the next signed production
connection to receive a higher Windows epoch. Its separately invoked outage
mode coordinates an owner-controlled Windows service stop/start with the
production Swift lifecycle: the app must first observe Connected, fail closed
to Master Offline while the service is stopped, and return to Connected only
with a strictly higher authenticated epoch after restart. This is bounded
service-outage evidence, not a Tailscale/network-interface outage or long-run
background reliability proof. The helper is not yet bundled in the
distribution and this is not unattended background operation. The repository
now contains the next bounded control-plane foundation: schema-v3 Windows
metadata events with a server-issued durable stream cursor, an authenticated
Mac-local `jarvis-agent` Unix-socket relay, and an owner-only local cursor
store. Master enqueue, lease, terminal result, cancellation, disconnect,
expiry, and startup reconciliation append their metadata events in the same
SQLite transaction as the authoritative state change. The agent accepts only
one contiguous stream and rejects replay, gaps, or stream replacement. It must
be launched by its declared direct parent and receives socket, peer code
requirement, and a fresh 32-byte bearer only through bounded startup stdin.
This repository proof does not yet mean `Jarvis.app` launches the agent or that
the agent owns the enrolled outbound mTLS connection. Live MLX execution,
repository mutation, Git publication, Codex dispatch, bundled installation,
and unattended operation remain unimplemented.

Current app-supervised IPC defaults to a Unix domain socket, not a TCP listener.
Swift creates a current-owner `0700` runtime directory and a generation-random
socket leaf whose absolute path fits the platform limit. The strict v1 startup
document carries `ipc_transport:{kind:"unix_socket_peer_identity_v1",
socket_path:"...",peer_code_requirement:"...",peer_identity_profile:
"adhoc_exact|developer_id_hardened"}` and the fresh 32-byte bearer; neither
authority enters argv or child environment. Rust retrieves `LOCAL_PEERTOKEN`
from each accepted socket and uses Security.framework dynamic-code validation
against the supplied designated requirement before reading a frame. It also
requires `getpeereid` to match the current EUID and the bearer on every router
path. Swift applies the same audit-token/requirement validation to the connected
core before sending a request.

Authenticated app-supervised launches also pass the Swift app's direct process
identifier through the non-secret `--supervised-parent-pid` argument. The core
validates that identifier against its direct parent before opening SQLite and
then watches the relationship for the lifetime of the server. If the app exits
or is killed, the core drops the server and releases its socket and database
owner lease instead of surviving as an orphan that blocks the next launch.
Manual or externally supervised `jarvis serve` processes do not opt into this
parent binding and remain operator-owned.

Packaging assigns stable code identifiers `com.nobiletechnology.jarvis` and
`com.nobiletechnology.jarvis.core`; alternate package identifiers are rejected.
Artifact-producing packaging refuses to remove or replace the configured
distribution bundle while that exact app or bundled-core executable is active,
with quit-or-alternate-output guidance. This prevents the normal local release
workflow from invalidating the on-disk identity of a surviving app process; it
does not weaken runtime signature validation or claim to eliminate the narrow
process-inspection race.
The `adhoc_exact` profile admits
only exact cdhash designated requirements and proves local mechanics for one
build, not publisher identity. `developer_id_hardened` additionally requires
Apple-generic anchored Developer ID Application leaf/intermediate certificate
extensions, the expected stable identifiers, one matching nonempty team
identifier, and hardened-runtime flags. Missing, malformed, unsigned,
mixed-profile, wrong-code, or invalid
requirements fail closed.

Each connection carries exactly one four-byte big-endian length followed by one
strict JSON request, then the client half-closes its write side so trailing
input can be rejected before dispatch. The server returns the same framing for
one strict JSON response.
The versioned request admits only GET, POST, DELETE, or PATCH and exact fields
for method, path, nullable authorization/accept/content type, and standard
padded base64 body; the response returns exact version, status, nullable content
type, and padded base64 body. Frame/body, hard monotonic deadline, and in-flight
connection limits fail closed. Launch failure, stop, replacement, or observed exit clears the
matching bearer and removes only the validated socket leaf; unsafe, wrong-type,
or out-of-bounds paths fail without recursive cleanup.

Exact `JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true` deliberately selects the weaker
authenticated loopback TCP and owner-only token-file compatibility path instead
of UDS. `JARVIS_MAC_IPC_AUTH_FILE` is an optional absolute override only in that
mode. The supervisor removes both app-only variables and
`JARVIS_IPC_TOKEN_FILE` from the child environment. Explicit operator-launched
legacy servers remain available without authentication and reject an unexpected
Authorization header. The default UDS now proves audit-token-bound intended-code
checks for the evaluated signature profile; the compatibility path does not.
Repository ad-hoc evidence does not prove Developer ID publisher identity,
device authentication, XPC, App Sandbox, notarization, or live-device behavior.

Primary design rule: Swift should not become the agent brain, and Rust should not become the Mac UX layer.

## Rust Core Modules

### Conversation Runtime

Owns sessions, turns, tool-call orchestration, streaming status, task state, cancellation, and the current "what is Jarvis doing now" state.

### Model Router

Chooses between local models and ChatGPT. Local is default. ChatGPT requires an explicit route decision, logged reason, sensitivity check, and policy approval.

### Memory Store

Stores user preferences, project and workflow memory, personal operating context, and decision logs. Every memory item has provenance, timestamps, category, sensitivity, and review/delete controls. A versioned local projection manifest can be atomically rebuilt from active canonical records and inspected through count-only status.

### Permission And Risk Engine

Combines capability scopes with risk tiers. It decides whether an action can run silently, requires notification, requires confirmation, or is blocked.

### Tool And Plugin Host

Loads first-party and later third-party capabilities behind explicit manifests. Tools declare permissions, risk class, input schema, output schema, audit behavior, timeout behavior, and cancellation behavior. Model planning uses the registered first-party catalog by default. A reactive local-model command may explicitly opt in to a second runtime-derived catalog containing only currently executable, eligible installed `local_wasm` actions; cloud and proactive routes and every installed `local_subprocess` action remain excluded.

The first production-useful local capability is bounded workspace inspection.
It is absent unless the operator explicitly configures an allowlisted root;
model input selects only that opaque root identifier and a validated relative
path. Rust anchors traversal to an already-open directory descriptor, refuses
symlinks and secret/hidden/special/binary paths, caps listing and UTF-8 text
output, and audits metadata rather than file contents or absolute paths. These
results are local-model-only and must never be returned to a ChatGPT step.
Emergency pause and task cancellation dominate completion and suppress late
capability output. Deterministic `fake_*` plugins remain test fixtures and are
not part of production inventory.

The Swift app owns the production root-grant UX and durable security-scoped
bookmark metadata. It resolves the complete bookmark set fail closed before
each launch, retains balanced access for the supervised child lifetime, and
sends opaque IDs plus resolved paths through a strict bounded versioned stdin
envelope that may also carry one trusted-wake one-shot document. App-selected
paths never enter child argv or environment. The legacy CLI root flag remains
only as an explicit compatibility/operator surface.

Installed `local_wasm` plugins are a separate compute-only runtime. They must
hold the `wasm_compute` grant, preserve exact module-byte provenance, and export
`memory`, `jarvis_alloc`, and `jarvis_run` under the `jarvis_json_v1` ABI. The
runtime rejects every import, including WASI, environment, filesystem, network,
clock, and process imports. It caps the module at 4 MiB, request JSON at
256 KiB, output JSON at 1 MiB, linear memory at 16 MiB, table elements at zero,
and each invocation at 10 million fuel units. Only low-risk, non-proactive, compute-only actions with
no memory/model/network permissions are eligible. Emergency pause,
cooperative cancellation, timeout, and fuel exhaustion fail closed and discard
late output. Wasmi enforces this guest-language boundary but is not a macOS OS
sandbox, same-user IPC isolation, publisher or marketplace trust, malware
analysis, signing/notarization, or live-device evidence. Local subprocess
plugins remain a distinct repository-backed runner and truthfully report that
they are not OS sandboxed.

Installed execution never holds the repository mutex across guest work. It
snapshots and validates the installed record and exact current provenance under
the lock, releases the lock before starting either subprocess or Wasmi code,
then reacquires repository access only for redacted audit persistence. Pause and
cancellation are checked after unlock and again before output or completion
audit acceptance, preventing long-running plugins from blocking unrelated
repository operations or publishing a late success.
Model-catalog discovery snapshots at most 64 enabled `wasm_compute` candidates
under the repository mutex, performs provenance work after unlock, and accepts
only candidates whose database record remains unchanged. Source-tree provenance
is capped at 8,192 entries, 4,096 files, 64 levels, and 64 MiB.
Installed-plugin callers attach a unique cancellation identifier and request
it through local IPC. Wasmi observes it between fuel slices and before output
acceptance. Installed subprocesses run in dedicated Unix process groups; active
cancellation, emergency pause, timeout, bounded-I/O failure, and other abnormal
exits terminate the group with bounded TERM-to-KILL escalation and reap the
leader before returning. This stops descendants that remain in the group, but
does not contain a process that deliberately escapes it with `setsid`/`setpgid`;
that requires the planned OS-sandboxed helper. A subprocess effect issued before
termination remains possible and cannot be reversed.

Installed WASM model planning is disabled by default and requires the caller to
set the per-command `installed_wasm_tools` opt-in. The runtime advertises these
actions only after routing selects a reactive local-model provider and only
when the installed record is enabled with `wasm_compute`, its current exact
module provenance matches, and every action still satisfies the low-risk,
non-proactive, compute-only contract. It revalidates the same state immediately
before execution and rejects identifier collisions with first-party plugins.
The catalog exposes at most 16 actions, with a 1 KiB description, 16 KiB input
schema, and 64 KiB combined installed-tool catalog budget. It never exposes
module bytes, paths, hashes, publisher material, or subprocess configuration. Cloud routes,
proactive commands, commands without the opt-in, and all `local_subprocess`
plugins cannot advertise or execute an installed model-planned tool. The same
`PermissionEngine` sensitivity policy applies before Wasmi entry, so private,
credential-adjacent, and restricted commands stop for explicit confirmation.

### Scheduler And Trigger Engine

Runs approved proactive routines and event-driven checks. v1 jobs should be local, inspectable, cancellable, and subject to the same policy rules as reactive commands.

The packaged app can now explicitly enable that bounded loop. The setting is
persisted locally, defaults off, clamps polling and per-tick work, optionally
performs bounded stale-running recovery before serving, and takes effect only
after a deliberate supervised-core restart. This is background operation while
the app is running, not a LaunchAgent, an OS wake guarantee, or permission to
bypass proactive policy and plugin checks.

The macOS system-wake foundation is disabled by default. Swift keeps its P-256
private key in a device-only Keychain item. Normal app/core startup never reads
that key. Initial provisioning is an explicit user action: bootstrap bytes are
prepared while the current app-owned core remains healthy, then the supervisor
performs one bounded-stdin restart and discards the one-shot bytes. The
persisted Rust enrollment survives later normal restarts without wake
Keychain access or bootstrap stdin. Rust verifies session/challenge,
enrollment-generation, clock-skew, UUID-nonce, and durable-counter bindings,
then persists visible scheduling evidence before dispatching through the normal
proactive policy/plugin/pause funnel. Ambiguous started events are never
automatically repeated and require explicit resolve-without-retry review. Swift
allocates each counter above its Keychain value, current epoch milliseconds,
and Rust's durable replay high-water so Keychain loss or a backward clock cannot
strand an otherwise valid enrollment. Explicit key control is a separate
two-step workflow. Normal rotation requires an active-session, domain-separated
signature from the enrolled key. Lost-key recovery deliberately omits old-key
proof but requires a stronger destructive confirmation. App-supervised IPC
requires bearer possession while an explicitly operator-launched legacy route
does not; in either mode that phrase is accident prevention, not user, device,
OS-identity, or ownership authentication. In one immediate transaction Rust rejects
ambiguous dispatch, blocks accepted old-generation work, disables the rule,
advances generation, resets replay high-water, and stores only a short-lived
one-shot grant hash plus staged-key fingerprint. A single supervised stdin
restart can consume that grant and install the new public key disabled. Swift
keeps the old active key until status proves the candidate fingerprint and
target generation, and journals explicit resume/cancel reconciliation. The
workflow never auto-enables, rolls back, or retries. Manual SQLite or Keychain
mutation is not a recovery procedure. Prepare documents, signed proofs, and
returned one-time grant tokens are secret-bearing transport: they use bounded
stdin or in-process IPC and must flow directly into trusted device-only
Keychain journal code. That code constructs the distinct install document for
supervised stdin; the raw prepare response is not install input. Neither form
may enter argv, terminal output, shell history, logs, or intermediate files.

### Audit Log

Stores an append-only record of prompts, model routes, tool calls, decisions, files touched, external actions attempted, approvals, denials, and failures.

## Swift Mac Shell Surfaces

### Command Console

A compact, always-available text and voice interface. It supports typed commands, spoken commands, streaming responses, current task state, cancel/pause, and escalation prompts.

### Voice Layer

Target production behavior handles wake/listen mode, speech-to-text,
text-to-speech, interruption, low-latency acknowledgement, and handoff to
background execution. The current Swift shell has a protocol-backed
Speech/AVFoundation input adapter, a protocol-backed AVFoundation speech-output
adapter, visible degraded/interrupted states, and typed transcript handoff to
the same text command submit path. Automated tests use fakes for input/output
adapter behavior; live microphone, Speech permission, audio output, signed-app
validation, owner-recorded live-device voice and non-voice QA, final owner
release evidence, and repository-backed `task:<uuid>` or `audit:<uuid>`
command-result evidence remain release gates before `live_voice_loop` can clear
in evidence-aware readiness mode.

### Activity And Audit View

Human-readable timeline of what Jarvis did, why, which model was used, what tools ran, and which permissions were involved.

### Memory Manager

UI to inspect, edit, disable, categorize, or delete remembered facts and preferences.

### Permission Center

Capability toggles, risk-tier rules, per-plugin scopes, approval history, and emergency pause/kill switch.

Approval decisions and execution authority are separate. Approve or deny never
runs a side effect. An explicit approved execution first revalidates the task,
approved record, still-waiting task, exact action, current risk and scopes,
current manifest, input schema, and policy. Direct installed-plugin runs also
pass through the permission engine: low/notify actions may continue under their
explicit execution grant, while confirm-tier or sensitive invocations create a
pending approval without entering the runtime. Schema v15 binds that pending
invocation to canonical input held only in SQLite plus private input/contract
digests, the exact plugin/action, manifest and authorization-relevant provenance,
execution grant, task, scopes, risk, and sensitivity. The binding input and
digests are never returned by approval, audit, diagnostics, CLI, or Swift
surfaces. Explicit execution re-verifies every bound field, then the Schema v13
claim mechanism atomically writes a unique durable claim and redacted
`approval_execution_claimed` audit before plugin entry. The claim is permanent:
success, failure, cancellation, timeout, or an unresolved restart cannot reuse
the approval. Terminal execution state, task state, and terminal audit evidence
commit in one transaction. Because a post-claim interruption may have produced
an external effect, Jarvis reports the outcome as ambiguous and requires an
operator to review evidence and create a new approval rather than retrying
automatically.

At repository-backed core startup, schema v16 reconciles pre-existing claimed
executions into a separate redacted attention ledger before accepting IPC. It
does not re-enter the runtime or weaken the permanent replay guard. The queue
omits action text, invocation input, decision text, and provenance digests.
Its summary reports the true unacknowledged count, returned item count, fixed
page limit, and truncation state separately; the bounded page must never be
presented as the total backlog.
An operator may acknowledge only the exact observed revision with the explicit
`acknowledged_without_retry` disposition. That compare-and-swap increments the
attention revision and appends audit evidence atomically, but does not mutate
or delete the consumed execution claim and cannot create a replacement approval.

Every file-backed repository acquires a sibling owner lease before backup,
version inspection, or migration and retains it for the repository lifetime.
The Unix lock file is opened with no-follow and close-on-exec, must be a regular
single-link current-owner file with no group/other permissions, and uses a
nonblocking exclusive advisory lock. Concurrent cores fail closed before
opening SQLite; in-memory test repositories remain lease-free.
This is a cooperating-Jarvis ownership boundary, not mandatory locking against
raw SQLite clients or other noncooperating file writers.

CLI and Swift approved-execution clients generate a fresh cancellation UUID.
Rust registers the handle, binds it to the approved task, and activates it at
the claim boundary. Authenticated cancellation can target only that exact
active run; when it wins output acceptance, Jarvis discards late output and
atomically records cancelled claim and task state. This cooperative boundary
cannot reverse an external effect already performed. The Approval Center owns
that same handle while execution is active and presents a Cancel Run control.

### Plugin Manager

Installed plugin list, requested permissions, enable/disable controls, logs, and version/update state.

The current Swift Plugin Manager exposes the Rust-owned lifecycle authority:
apply an explicitly selected local replacement manifest, inspect redacted
lifecycle history, verify local provenance, explicitly choose a compatible
execution grant, enable after verification, and disable back to
`metadata_only`. The replacement candidate remains untrusted input. New local
installs require valid SemVer 2.0.0. An update must retain the installed plugin
identity, advance its semantic version, pass the same bounded manifest and
provenance validation as installation, match the
currently inspected lifecycle digest, capture a new snapshot, and reset the
record to disabled `metadata_only`; the new bytes require fresh provenance
verification and explicit re-enable. Confirmation captures the exact reviewed
grant and redacted lifecycle-contract digest; Rust applies the mutation only
when that digest still matches, preventing stale review from broadening authority. Each plugin
serializes lifecycle mutations, refreshes from the repository-backed inspection
endpoint after every outcome, and disables all lifecycle actions whenever that
registry is stale. Network-capable grant review shows exact declared hosts and
permissions, while subprocess confirmation remains explicit that the current
runner is not OS sandboxed and does not enforce host-level egress. Verification
and authority responses use the redacted inspection projection. Rust commits
each provenance/update audit and each authority mutation plus its redacted
`side_effect_executed:false` audit transactionally; audit failure rolls the
associated state change back. Lifecycle history is a bounded redacted view of
those transitions, not publisher identity, marketplace approval, malware
analysis, OS sandboxing, or host-egress proof.

A persisted pre-SemVer installed record may cross the version boundary once to
a valid SemVer candidate under the same identity, source-kind, lifecycle-CAS,
candidate-snapshot, disabled-authority, and atomic-audit checks. Once stored,
that candidate makes every later update strictly ordered by SemVer precedence.

The typed update contract separates review from mutation:
`POST /plugins/installed/:id/update/preview` validates the local candidate and
returns only a redacted current/candidate version summary plus the fact that
execution will be disabled and an opaque `candidate_update_contract_sha256`
aggregate integrity binding, and echoes the validated
`current_lifecycle_contract_sha256`. Clients must preserve and show that exact
reviewed token pair. `POST /plugins/installed/:id/update/apply` requires the
same candidate path, both preview tokens, and explicit confirmation; it must
not silently re-inspect or refresh either token. Rust reloads the candidate,
recomputes the binding from the exact snapshot, and rejects preview/apply drift
before mutation. The binding is not a raw manifest, source-tree, command, or
module provenance hash.
`GET /plugins/installed/:id/history` returns the bounded redacted lifecycle
ledger.

### Settings And Model Routing

Local model configuration, ChatGPT configuration with separate OpenAI API-key
and logged-in Codex-account authentication choices inside the same approved,
policy-checked cloud route, default routing policy, privacy controls, voice
settings, and diagnostics export. Codex-account execution is response-only at
the Jarvis boundary: shell, unified execution, code-host, app/plugin, browser,
computer, web-search, image-generation, multi-agent, and workspace-dependency
tool features are disabled before redacted context is sent.

The Model tab lets the operator choose whether every cloud prompt should require
approval. With that toggle off, ordinary Public, Workspace, and Personal
conversation uses the already explicit cloud-provider selection without a
repeated prompt. Private and Credential-adjacent commands still stop at the
cloud boundary, where Swift exposes a one-shot `Approve & Send` action for that
exact prompt. The retry carries an explicit command-scoped approval bit over
authenticated local IPC; Rust converts it into a cloud-model approval grant
only for a non-proactive request. Restricted content remains blocked, proactive
cloud execution cannot reuse the grant, and tool/action approvals are unchanged.
The same tab exposes the selected Codex-account model and compatible reasoning
effort; Rust passes the effort through the constrained CLI config while keeping
the CLI's internal approval policy fixed to `never` and its tool features
disabled.

The Model tab may perform one explicit local integration-maintenance action:
upgrading a Homebrew-managed Ollama formula after a visible confirmation. The
action is available only for a loopback Ollama endpoint, resolves Homebrew from
the fixed Apple Silicon or Intel installation locations, invokes it directly
without a shell or user-derived arguments, passes a minimal environment,
verifies the formula version before and after mutation, and restarts the Ollama
Homebrew service only when that service was already running. A remote endpoint,
missing Homebrew, non-formula Ollama installation, failed version check, or
failed command remains visible and fail-closed; the UI does not silently start
a stopped service or claim that an app-installed Ollama was upgraded.

Diagnostics do not embed the full health response. Their dedicated health
projection exposes `emergency_paused`, `emergency_pause_updated_at`, and
`emergency_pause_reason_present`; the legacy reason field is either null or the
fixed `redacted` marker, never arbitrary reason text. The explicit
`/health`, pause response, and pause-status operator surfaces retain their
existing reason contract and must not be treated as diagnostics exports.

## Command Data Flow

1. User speaks or types into `Jarvis.app`.
2. Swift captures the input, attaches UI/session context, and sends it to `jarvis-core`.
3. Rust creates a task record and runs policy prechecks.
4. The conversation runtime asks the model router for a model decision.
5. The selected model produces a plan, answer, or tool request.
6. Tool requests go through the permission and risk engine before execution.
7. Approval-required tools are revalidated, durably claimed exactly once, and
   only then enter the plugin host; allowed non-approval tools enter directly.
8. Memory writes are proposed, classified, and stored only if policy allows.
9. The audit log records the full chain: input, route, decisions, tools, outputs, approvals, and final result.
10. Swift displays and speaks the response, then exposes follow-up controls.

For proactive routines, the flow starts in the scheduler or trigger engine instead of the UI. Those jobs create visible task records, obey the same risk rules, and notify the app when user attention is needed.

## Safety And Error Handling

- Fail closed for risky actions. If permissions, policy, identity, plugin validation, or model route checks are uncertain, Jarvis blocks or asks.
- Separate planning from acting. Plans can be generated freely, but side-effecting actions pass through the risk engine.
- Support cancellation across tasks, tool calls, scheduled jobs, and proactive triggers.
- Keep state recoverable. Task state, memory writes, plugin changes, and configuration changes should be transactional or rollback-friendly.
- Show degraded modes clearly. If the local model is down, microphone permission is missing, ChatGPT is unavailable, or a plugin fails, the UI should say so.
- Provide an emergency pause control that stops new actions, pauses scheduled/event-driven jobs, cancels active non-critical tasks, and requires deliberate resume.
- Treat plugin containment as part of v1, even if only first-party plugins ship initially.

## Storage

### SQLite

SQLite is the primary store for tasks, sessions, audit entries, permissions, plugin registry, model-route records, scheduler jobs, memory metadata, and schema version state.

### Keychain

macOS Keychain stores API keys, OAuth tokens, model credentials, and sensitive integration credentials.

### File-Backed Artifacts

An app-owned support directory stores larger generated files, transcripts, exported diagnostics, plugin bundles, local model configs, and attachments.

### Vector Index

Memory and document retrieval can use a local index. The implemented governance
layer keeps a versioned, atomic, rebuildable projection tied to canonical
SQLite records and detects missing, stale, deleted, orphaned, or corrupt
entries. SQLite remains the source of truth. An explicit, disabled-by-default
command option can perform bounded deterministic lexical retrieval for a
selected local-model route. Only reviewed, active Public, Workspace, or
Personal records are eligible; proactive and cloud routes plus Private,
CredentialAdjacent, and Restricted records fail closed. The resulting context
is ephemeral, capped, framed as untrusted data, and excluded from public audit
and route evidence. Vector embeddings, approximate-nearest-neighbor search,
automatic retrieval, and autonomous memory rewrite/purge remain future work.

### Sensitivity Labels

Data categories should include public, workspace, personal, private, credential-adjacent, and restricted. These labels feed model routing, memory review, plugin access, and diagnostics export.

## Plugin Contract

A plugin declares:

- Name, version, author/source.
- Capabilities provided.
- Required permission scopes.
- Risk tier for each action.
- Input and output schema.
- Whether it can run proactively.
- Whether it can access memory.
- Whether it can call models.
- Whether it can access the network and, if so, exact allowed hostnames.
- Audit fields it must emit.
- Timeout and cancellation behavior.

The runtime supports first-party in-process Rust modules, constrained local
subprocess plugins over bounded JSON stdin/stdout, and no-import `local_wasm`
compute plugins using `jarvis_json_v1`. The manifest and policy contract stays
stable across runtimes; each runtime must expose its actual confinement status
without implying a stronger OS or trust boundary.

## Model Routing

- Default to local models for simple commands, personal context, memory operations, home/system context, and sensitive data.
- Use ChatGPT only through explicit policy for higher-reasoning tasks, coding help, research synthesis, complex planning, or when local models are insufficient.
- Treat both OpenAI API-key and Codex-account UI affordances as authentication
  choices inside the same approved, audited ChatGPT cloud route, not as
  unaudited provider bypasses. Codex-account execution must use strict config,
  mechanically disable agent/tool/integration surfaces including web search,
  minimize inherited environment, ignore user/project rules, monitor and cap
  the private response file, and fail closed if the CLI cannot honor those controls.
- Do not send restricted, credential-adjacent, private personal, or sensitive system data to ChatGPT without explicit approval for that task.
- Record the model route and reason in the audit log.
- Minimize cloud context before any ChatGPT call and redact obvious secrets.
- If local inference fails, ask to escalate to ChatGPT or continue in degraded local-only mode depending on sensitivity and settings.

## Testing Strategy

### Rust Core Unit Tests

Cover permission decisions, risk tiers, model routing, memory classification, scheduler rules, plugin manifests, audit log creation, and migration behavior.

### Rust Integration Tests

Exercise the end-to-end command pipeline using fake models and fake plugins. Prove task creation, routing, tool authorization, audit logging, cancellation, and error states.

### Swift App Tests

Cover UI state, permission prompts, settings behavior, memory manager views, activity timeline rendering, and emergency pause behavior.

### IPC Contract Tests

Version and test shared schemas between Swift and Rust. Cross-process coverage
must exercise the default UDS launch with audit-token designated-requirement,
peer-EUID, and bearer enforcement, including same-EUID wrong-code rejection
before request decoding,
strict framed request/response decoding, existing-route parity, bounds and
cleanup, restart invalidation, and the explicit TCP/token compatibility path.
The release-built app lane must also traverse authenticated health, dry-run
command, task/audit inspection, diagnostics, pause, blocked-command, and resume
through the app-owned Swift client on the default UDS before emitting a fixed
non-secret success marker. Failures suppress success and post-pause cleanup
makes a bounded best-effort resume attempt. Ad-hoc coverage proves exact-build
cdhash mechanics only; Developer ID, notarization, and live-device claims need
their separate signed evidence lanes.
Breaking the app/core API should fail loudly.

### Voice Loop Tests

Cover text input parity, wake/listen state transitions, speech-output state, interruption/cancel behavior, and degraded-mode behavior when mic or TTS permissions fail. Adapter tests use fakes and must stay explicit about what is covered; they do not imply live microphone, Speech permission, or live audio output coverage until those checks run against a signed app on a real device.

### Safety Regression Tests

High-risk actions must never bypass approval. Cloud routing must never receive restricted data without explicit approval. Plugins must not execute outside declared scopes.

### Release Smoke Test

The packaged Mac app launches, starts the Rust core, handles a command, writes audit state, toggles emergency pause, and exits cleanly.

## Packaging And Operations

- `Jarvis.app` bundles the release-built CLI executable at
  `Contents/Resources/bin/jarvis-cli`; the executable hosts the Rust
  `jarvis-core` library behind the local IPC contract.
- The app supervises the core in v1; LaunchAgent support is deferred until needed.
- Diagnostics export produces redacted logs, config summaries, schema versions,
  plugin state, model status, pause-reason presence rather than pause-reason
  text, and recent failure reports.
- SQLite migrations run predictably with file-backed preflight backup and
  restore-on-failure behavior before broader installer upgrade QA.
- Crash and failure reporting is local-first initially. External reporting is deferred and user-approved only.
- Releases use version numbers, changelog, migration notes, and smoke-test checklist.
- Repo docs should include build/test commands, architecture map, plugin
  contract, safety rules, release checklist, and durable knowledge-base facts so
  agents can maintain the project.
- Each feature or phase should update docs and knowledge-base facts, identify
  the relevant end-to-end coverage, add missing E2E coverage when the feature
  changes executable behavior, and clearly record any skipped or blocked gate.

## Historical Initial Implementation Handoff Outline

The project began with the smallest product-grade foundation:

1. Create a repo structure with `apps/mac` for Swift and `crates/jarvis-core` for Rust.
2. Define the app/core IPC schema and a health-check command.
3. Implement core task records, audit logging, and emergency pause state.
4. Build a minimal Swift command console that starts the core and sends text commands.
5. Add fake local model and fake plugin implementations to prove routing, policy, logs, and UI activity.
6. Add SQLite migrations and app support directory layout.
7. Add the first release smoke test.

Current implementation status: the repo structure, IPC health/command surface,
durable task/audit/emergency-pause/memory/scheduler schema, fake local model,
first-party plugin contracts, metadata-only local plugin installation, local
plugin provenance snapshot verification, CLI smoke path, operator-readable CLI
surfaces for command/ask, plugins, tools, tasks, routes, activity, readiness,
and evidence status with `--json`/`JARVIS_CLI_JSON=1` preserving exact payloads,
redacted diagnostics export, repository-backed activity summary, and buildable
Swift command/activity shell are implemented. The command runtime can route to fake local,
Ollama-compatible local HTTP, or explicitly enabled ChatGPT/OpenAI-compatible
HTTP providers. The IPC layer exposes bounded activity event streaming for
current task/audit progress, contract compatibility policy, and contract
feature metadata that names implemented surfaces, proof, and explicit
production boundaries. The Swift app includes approval decision controls,
management surfaces, memory classification plus create/update/review/delete/restore
controls over the existing Rust IPC contract, run activity summary,
voice input/output adapter controls, text-transcript command handoff,
permission policy review,
redacted scheduler attention handoff, scheduler trigger policy-review items,
adapter-backed scheduler notification controls for due, failed, and
emergency-pause-blocked attention items,
default-off app-supervised scheduler automation with bounded startup recovery
and authorized no-prompt attention polling,
and core supervision abstractions.
The opt-in Ollama adapter uses native NDJSON response transport with bounded
body, assembled-response, and metadata budgets. It quarantines all fragments
until a terminal `done:true` frame and the final response/tool envelope validate.
Runtime cancellation or emergency pause drops the in-flight provider future,
discards partial state, and wins a completion race before audit or tool
execution. Interactive clients now generate a UUID cancellation handle before
submitting a command; the core registers it for the complete active request,
binds it to only that task, and suppresses steps/tool results if cancellation
wins final acceptance. Swift exposes Cancel only while its submission is
active. Swift keeps the assistant transcript final-only and can inspect only
redacted post-validation transport metadata; raw-token UI streaming remains an
end-goal capability.
Installed subprocess cancellation now reaches the active worker rather than
only discarding its eventual response. Unit coverage proves pause/cancel,
TERM-ignoring in-group descendants, blocked stdin with oversized output, and
bounded reaping; authenticated cross-process approval coverage proves the
in-group fixture stops, the terminal audit remains effect-possible and
non-retryable, and the one-shot approval cannot replay after restart. This
remains process control, not protection from deliberate process-group escape,
effect rollback, OS sandboxing, or host-level egress enforcement.
Installed plugin publisher-origin claims can be operator-pinned after local
provenance matches the install snapshot and the supplied trusted origin exactly
matches the manifest author claim. Signed manifests can also be verified with
an Ed25519 `publisher_signature` against an explicit trusted public key after
local provenance matches; this is audit-backed trusted-key verification, not
marketplace approval or malware analysis. Installed-plugin inspection through
`/plugins/installed` and `/plugins/installed/:id` is redacted by default: local
paths, subprocess command paths, signature material, and provenance hashes are
omitted from the review surface.
Network-capable actions must request the `network` permission and declare
plain-hostname allowlists in `network_access`; policy review surfaces those
actions, and executable installed plugins with network-declaring actions must
be enabled with the explicit `subprocess_stdio_network` grant. OS-level network
sandbox enforcement and host-level egress filtering remain target architecture.
The Swift Plugin tab can update a matching installed record from an explicitly
selected local manifest, inspect redacted lifecycle history, verify the new
snapshot, explicitly select a manifest-compatible grant, enable only after
matching provenance, and disable without executing plugin code. Every update
resets execution to disabled `metadata_only`; it does not carry forward prior
verification or authority. The confirmed grant and lifecycle-contract
digest form a compare-and-set request, so changed or reinstalled records require
fresh inspection and confirmation. It refreshes authoritative server state
after every attempt, disables lifecycle actions while that state is stale, and
exposes declared network hosts plus the current lack of OS sandbox/egress
enforcement. Mutation responses remain redacted. Provenance verification audits
and grant-mutation audits commit transactionally in Rust, with focused storage,
Swift contract/model/presentation, and authenticated loopback-TCP compatibility
E2E coverage across enabled and disabled restarts. Default packaged IPC remains
the peer-identity-validated Unix-domain-socket transport.
The product still lacks
Apple-tool-validated signed/notarized/stapled release evidence, live microphone and audio-output validation,
marketplace/OS-network-sandbox plugin trust boundaries, richer
proactive trigger policy, and live OS notification validation. Swift supervision
remains unsigned production-wise, but local packaged-app smoke and unsigned
distribution-layout launch proof now cover configured/bundled core discovery;
signed/notarized app, clean-profile Finder launch, live-device QA, and manual
release QA remain external gates.

Historical phase-3 production sweeps used isolated worktrees, topic branches,
and reviewable PR slices for lanes such as model-route persistence, plugin
grant gating, voice adapters, packaged-app smoke, permission UX, scheduler
attention, notification controls, policy review, and architecture docs. Treat
those lane names as historical routing context unless a current checkout,
branch, or open PR proves that a lane is active. The workflow improves
reviewability, but it is not readiness evidence by itself.
Readiness language must be tied to checked-in code, documented diagrams,
knowledge-base updates, and the specific local/E2E checks that passed.

Before implementation, the design should be reviewed through a multi-agent brainstorming pass because Jarvis is high-autonomy, security-sensitive, and product-grade.
