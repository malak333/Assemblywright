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
| Local-model first with ChatGPT as the only approved cloud model | Cloud-first routing, provider-agnostic cloud routing | Matches the privacy posture while allowing explicit escalation for harder reasoning tasks. |
| Capability scopes plus risk tiers | Simple allow/deny prompts, risk tiers only | High autonomy needs both explicit permission boundaries and per-action risk evaluation. |
| SQLite as primary structured storage | Flat files only, external database | SQLite is durable, inspectable, easy to migrate, and enough for single-user v1. |
| macOS Keychain for secrets | Store credentials in SQLite or config files | Secrets should use the platform credential store. |
| First-party plugins first | Third-party marketplace in v1 | The safety model and plugin contract need to prove themselves before third-party expansion. |
| Auditability as an architectural requirement | Best-effort logs after the fact | Jarvis must be able to explain why it acted, what data it used, and what permissions were involved. |

## Architecture

Jarvis v1 runs as two cooperating local processes.

### Jarvis.app

`Jarvis.app` is a Swift/SwiftUI macOS application. It owns the visible user experience, voice session, status controls, permission prompts, notifications, settings, activity history, memory management UI, plugin management UI, diagnostics export, and emergency pause.

The app should feel like a real Mac product: menu bar presence, command surface, settings, clear current activity, and predictable recovery from degraded modes.

### jarvis-core

`jarvis-core` is a Rust local service started and supervised by the Swift app. It owns durable execution: task planning, model routing, memory reads and writes, plugin execution, scheduled jobs, event triggers, risk policy evaluation, and audit logging.

The initial IPC boundary can be loopback HTTP or a Unix domain socket. A future version can move the core into a LaunchAgent if stronger background execution is needed.

Primary design rule: Swift should not become the agent brain, and Rust should not become the Mac UX layer.

## Rust Core Modules

### Conversation Runtime

Owns sessions, turns, tool-call orchestration, streaming status, task state, cancellation, and the current "what is Jarvis doing now" state.

### Model Router

Chooses between local models and ChatGPT. Local is default. ChatGPT requires an explicit route decision, logged reason, sensitivity check, and policy approval.

### Memory Store

Stores user preferences, project and workflow memory, personal operating context, and decision logs. Every memory item should have provenance, timestamps, category, sensitivity, and review/delete controls.

### Permission And Risk Engine

Combines capability scopes with risk tiers. It decides whether an action can run silently, requires notification, requires confirmation, or is blocked.

### Tool And Plugin Host

Loads first-party and later third-party capabilities behind explicit manifests. Tools declare permissions, risk class, input schema, output schema, audit behavior, timeout behavior, and cancellation behavior.

### Scheduler And Trigger Engine

Runs approved proactive routines and event-driven checks. v1 jobs should be local, inspectable, cancellable, and subject to the same policy rules as reactive commands.

### Audit Log

Stores an append-only record of prompts, model routes, tool calls, decisions, files touched, external actions attempted, approvals, denials, and failures.

## Swift Mac Shell Surfaces

### Command Console

A compact, always-available text and voice interface. It supports typed commands, spoken commands, streaming responses, current task state, cancel/pause, and escalation prompts.

### Voice Layer

Target production behavior handles wake/listen mode, speech-to-text,
text-to-speech, interruption, low-latency acknowledgement, and handoff to
background execution. The current Swift scaffold does not use microphone,
Speech, or AVFoundation APIs. It models voice actions and degraded states, then
hands a typed transcript to the same text command submit path so voice parity
can be tested before real capture exists.

### Activity And Audit View

Human-readable timeline of what Jarvis did, why, which model was used, what tools ran, and which permissions were involved.

### Memory Manager

UI to inspect, edit, disable, categorize, or delete remembered facts and preferences.

### Permission Center

Capability toggles, risk-tier rules, per-plugin scopes, approval history, and emergency pause/kill switch.

### Plugin Manager

Installed plugin list, requested permissions, enable/disable controls, logs, and version/update state.

### Settings And Model Routing

Local model configuration, ChatGPT configuration, default routing policy, privacy controls, voice settings, and diagnostics export.

## Command Data Flow

1. User speaks or types into `Jarvis.app`.
2. Swift captures the input, attaches UI/session context, and sends it to `jarvis-core`.
3. Rust creates a task record and runs policy prechecks.
4. The conversation runtime asks the model router for a model decision.
5. The selected model produces a plan, answer, or tool request.
6. Tool requests go through the permission and risk engine before execution.
7. The plugin host runs allowed tools and streams progress back to Swift.
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

Memory and document retrieval can use a local vector index. The index must be rebuildable and tied back to canonical SQLite records. It is not the source of truth.

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
- Audit fields it must emit.
- Timeout and cancellation behavior.

The initial runtime can be first-party in-process Rust modules, subprocess plugins over JSON-RPC, or WASM plugins. The first architectural commitment is the manifest and policy contract, not the final sandbox mechanism.

## Model Routing

- Default to local models for simple commands, personal context, memory operations, home/system context, and sensitive data.
- Use ChatGPT only through explicit policy for higher-reasoning tasks, coding help, research synthesis, complex planning, or when local models are insufficient.
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

Version and test shared schemas between Swift and Rust. Breaking the app/core API should fail loudly.

### Voice Loop Tests

Cover text input parity, wake/listen state transitions, interruption/cancel behavior, and degraded-mode behavior when mic or TTS permissions fail. Until the real voice adapter exists, these tests must stay explicit about the typed-transcript scaffold and must not imply actual speech recognition coverage.

### Safety Regression Tests

High-risk actions must never bypass approval. Cloud routing must never receive restricted data without explicit approval. Plugins must not execute outside declared scopes.

### Release Smoke Test

The packaged Mac app launches, starts the Rust core, handles a command, writes audit state, toggles emergency pause, and exits cleanly.

## Packaging And Operations

- `Jarvis.app` bundles or installs the `jarvis-core` Rust executable.
- The app supervises the core in v1; LaunchAgent support is deferred until needed.
- Diagnostics export produces redacted logs, config summaries, schema versions, plugin state, model status, and recent failure reports.
- SQLite migrations run predictably with backup or rollback behavior.
- Crash and failure reporting is local-first initially. External reporting is deferred and user-approved only.
- Releases use version numbers, changelog, migration notes, and smoke-test checklist.
- Repo docs should include build/test commands, architecture map, plugin contract, safety rules, and release checklist so agents can maintain the project.

## Initial Implementation Handoff Outline

Implementation should begin with the smallest product-grade foundation:

1. Create a repo structure with `apps/mac` for Swift and `crates/jarvis-core` for Rust.
2. Define the app/core IPC schema and a health-check command.
3. Implement core task records, audit logging, and emergency pause state.
4. Build a minimal Swift command console that starts the core and sends text commands.
5. Add fake local model and fake plugin implementations to prove routing, policy, logs, and UI activity.
6. Add SQLite migrations and app support directory layout.
7. Add the first release smoke test.

Current branch status: the repo structure, IPC health/command surface, durable
task/audit/emergency-pause/memory/scheduler schema, fake local model, first-party
plugin contracts, CLI smoke path, redacted diagnostics export, and buildable
Swift command/activity shell scaffold are implemented. The Swift app still does
not have signed packaged-app release smoke evidence, and the product still lacks
real model providers, approval UI, voice, installed plugin execution, packaged
app release smoke evidence, and proactive trigger execution. Swift supervision
is covered only as a scaffold for configured or packaged-style local core
binaries.

Before implementation, the design should be reviewed through a multi-agent brainstorming pass because Jarvis is high-autonomy, security-sensitive, and product-grade.
