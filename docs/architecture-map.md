# Architecture Map

Jarvis is a local-first macOS assistant foundation. The current repository
contains a Rust workspace with the core contracts, loopback IPC server,
SQLite-backed repository primitives, policy/model-routing rules, in-process
plugin contracts, scheduler state, CLI client, and a first Swift/SwiftUI Mac
shell scaffold under `apps/mac`.

## Current Implementation

```mermaid
flowchart LR
    User["User or local test operator"] --> CLI["jarvis-cli"]
    User --> MacShell["JarvisMacApp SwiftUI scaffold"]
    MacShell --> MacCore["JarvisMacCore IPC client and console model"]
    MacCore -->|HTTP on configured core URL| IPC
    CLI -->|HTTP on 127.0.0.1 by default| IPC["jarvis-core::ipc Axum server"]

    IPC --> IpcState["IpcState"]
    IPC --> Inspection["Task, audit, memory, plugin inspection routes"]
    IpcState --> RuntimePath["Command endpoint runtime path"]
    IpcState --> Pause["Emergency pause state"]
    IpcState --> Scheduler["In-memory Scheduler"]
    IpcState --> RepoState["Optional SqliteRepository"]
    RuntimePath --> Router
    RuntimePath --> PluginHost

    Runtime["ConversationRuntime"] --> ModelExec["ModelExecutor trait"]
    ModelExec --> FakeLocal["FakeLocalModel"]
    Runtime --> RuntimeControl["RuntimeControl pause/cancel flags"]
    Runtime --> RuntimeAudit["Structured AuditEntry list"]
    Runtime --> RuntimeStore["RuntimeCommandStore persistence hook"]

    Router["ModelRouter"] --> Policy["PermissionEngine"]
    Router --> LocalRoute["Local route"]
    Router --> ChatGPTGate["ChatGPT route gate with redaction"]

    PluginHost["PluginHost"] --> ManifestValidation["Manifest and JSON schema validation"]
    PluginHost --> PluginPolicy["PermissionEngine policy check"]
    PluginHost --> FirstParty["fake_echo and fake_status plugins"]
    PluginHost --> TimeoutCancel["Timeout and cancellation handling"]

    Repo["SqliteRepository"] --> Tasks["tasks"]
    Repo --> Audit["append-only audit_entries"]
    Repo --> Memory["memory_items"]
    Repo --> StoredPause["emergency_pause"]

    Types["Shared contract types"] --> Runtime
    Types --> IPC
    Types --> Policy
    Types --> Repo
    Types --> PluginHost
```

The current IPC `/commands` endpoint invokes `ConversationRuntime` with the
deterministic `FakeLocalModel`, returns runtime steps, route metadata, plugin
results, and audit entries, and can persist task/audit state through
`SqliteRepository` when the state is constructed with repository backing. It
also records local-first `ModelRouter` evidence and can execute deterministic
first-party plugin commands through the policy engine. It does not yet support
autonomous model-generated tool calls, real model providers, or user approval UI.
Repository-backed IPC state also exposes task, audit, and memory inspection
endpoints, plus first-party plugin manifest listing, so the CLI and Swift shell
can inspect durable local state without reaching into SQLite directly.

## Current Command Flow

```mermaid
sequenceDiagram
    participant Client as CLI or Swift shell
    participant IPC as jarvis-core IPC
    participant Runtime as ConversationRuntime
    participant Router as ModelRouter
    participant Policy as PermissionEngine
    participant Plugins as PluginHost
    participant Store as Optional SqliteRepository

    Client->>IPC: POST /commands
    IPC->>Runtime: execute command with FakeLocalModel
    Runtime->>Store: create task and append runtime audit when configured
    Runtime-->>IPC: task, local route, steps, runtime audit
    IPC->>Router: record local-first route decision
    IPC->>Store: append model_route_selected when configured
    alt first-party plugin command
        IPC->>Policy: evaluate plugin scopes and risk
        IPC->>Store: append plugin_policy_evaluated when configured
        alt dry_run
            IPC->>Store: append plugin_dry_run when configured
        else allowed
            IPC->>Plugins: execute fake_echo or fake_status
            Plugins-->>IPC: schema-validated plugin result
            IPC->>Store: append plugin result audit when configured
        end
    end
    IPC-->>Client: command response with audit_entries and plugin_results
```

## Current Workspace Layout

```text
.
|-- Cargo.toml
|-- DESIGN.md
|-- README.md
|-- apps/
|   `-- mac/
|       |-- Package.swift
|       |-- Sources/
|       |   |-- JarvisMacApp/
|       |   `-- JarvisMacCore/
|       `-- Tests/JarvisMacCoreTests/
|-- crates/
|   |-- jarvis-cli/
|   |   `-- src/main.rs
|   `-- jarvis-core/
|       |-- src/
|       |   |-- ipc.rs
|       |   |-- lib.rs
|       |   |-- model.rs
|       |   |-- plugin.rs
|       |   |-- policy.rs
|       |   |-- router.rs
|       |   |-- runtime.rs
|       |   |-- scheduler.rs
|       |   |-- storage.rs
|       |   `-- types.rs
|       `-- tests/e2e_scaffold.rs
`-- docs/
```

## Implemented Rust Responsibilities

- `jarvis-core::types`: Stable shared records and enums for tasks, audit
  entries, sensitivity, risk, approval, task status, and errors.
- `jarvis-core::ipc`: Axum loopback HTTP API for `/health`, `/commands`,
  `/tasks`, `/audit`, `/memory`, `/plugins/manifests`, `/emergency-pause`, and
  `/scheduler/jobs`. The command endpoint runs the runtime with
  `FakeLocalModel`, records local-first route evidence, can execute
  deterministic first-party plugin commands through policy, returns
  route/step/plugin/audit evidence, and obeys emergency-pause state.
- `jarvis-core::runtime`: Command runtime scaffolding with max-step enforcement,
  runtime hooks, task cancellation, emergency-pause blocking/cancellation, model
  step audit entries, a fake local model path, and a persistence hook for
  SQLite-backed task/audit durability.
- `jarvis-core::model`: `ModelExecutor` trait, model request/response contracts,
  route metadata, and deterministic `FakeLocalModel` test implementation.
- `jarvis-core::router`: Local-first model route selection, ChatGPT opt-in gate,
  restricted-data blocking, approval delegation to `PermissionEngine`, and
  simple secret-token redaction before ChatGPT routing.
- `jarvis-core::policy`: Capability scopes, approval grants, risk-tier
  decisions, emergency-pause fail-closed behavior, confirmation requirements,
  and audit-required flags.
- `jarvis-core::plugin`: Plugin/action manifests, JSON schema validation,
  permission metadata, approval-required handling, proactive-action checks,
  timeout handling, cooperative cancellation signal, and two deterministic
  first-party test plugins.
- `jarvis-core::scheduler`: Inspectable in-memory scheduler jobs with manual,
  one-time, and interval trigger contracts plus cancellation support.
- `jarvis-core::storage`: SQLite schema migration version 1 for tasks,
  append-only audit entries, emergency pause, and memory items with provenance,
  sensitivity, review, and soft-delete fields.
- `jarvis-cli`: Local CLI for serving the IPC API with optional `--db-path`
  SQLite backing, calling health/command/pause/task/audit/memory/plugin
  endpoints, listing/scheduling/cancelling scheduler jobs over HTTP, and
  running `jarvis smoke` against ephemeral local servers.
- `apps/mac/JarvisMacCore`: Swift IPC client and command-console model that
  decode the Rust health/command/pause JSON contracts.
- `apps/mac/JarvisMacApp`: SwiftUI command-console scaffold with health status,
  transcript, send, pause/resume, and refresh controls.

## End-Goal Production Architecture

```mermaid
flowchart TB
    MacApp["Jarvis.app Swift/SwiftUI shell"] --> UX["Voice, text console, settings, activity, diagnostics"]
    UX --> IPCClient["Versioned IPC client"]
    IPCClient --> IPCServer["jarvis-core local IPC server"]

    IPCServer --> RuntimeProd["ConversationRuntime"]
    RuntimeProd --> PolicyProd["PermissionEngine"]
    RuntimeProd --> ModelRouterProd["ModelRouter"]
    RuntimeProd --> PluginHostProd["Policy-gated PluginHost"]
    RuntimeProd --> SchedulerProd["Scheduler and trigger engine"]
    RuntimeProd --> RepoProd["SqliteRepository"]

    ModelRouterProd --> LocalModels["Local model providers by default"]
    ModelRouterProd --> ChatGPT["ChatGPT only after enablement, approval, redaction, and audit"]

    PluginHostProd --> FirstPartyPlugins["First-party plugins"]
    PluginHostProd --> ThirdPartyPlugins["Installed local plugins with manifests"]
    PluginHostProd --> ToolSandbox["Declared scopes, schemas, timeouts, cancellation"]

    RepoProd --> TaskStore["Task lifecycle"]
    RepoProd --> AuditStore["Append-only audit log"]
    RepoProd --> MemoryStore["Memory with provenance and sensitivity"]
    RepoProd --> PauseStore["Emergency pause state"]

    MacKeychain["macOS Keychain"] --> RuntimeProd
    AppFiles["App-owned files"] --> RuntimeProd
    Diagnostics["Local diagnostics export"] --> MacApp

    PolicyProd --> ApprovalUI["User approval UI"]
    ApprovalUI --> MacApp
    PauseUI["Emergency pause control"] --> PolicyProd
```

Production readiness for that end-state still requires a packaged `.app`
release, real local model provider integration, approval UI, plugin
installation/runtime hardening, voice support, packaged app smoke tests, and
operational release evidence. The current repository proves only the implemented
Rust and Swift scaffold surfaces listed above.

## Data Ownership

- SQLite is the implemented structured-state backend for tasks, audit entries,
  emergency pause, and memory items.
- Audit entries are protected by SQLite triggers that reject update and delete
  operations.
- Memory items carry provenance, sensitivity, review timestamps, and soft-delete
  state.
- The end-goal architecture still expects macOS Keychain for credentials and
  app-owned files for large artifacts, transcripts, diagnostics exports, plugin
  bundles, local model configs, and attachments.
- Any future vector index should remain rebuildable from canonical records.

## Implemented SQLite Schema

```mermaid
erDiagram
    TASKS ||--o{ AUDIT_ENTRIES : "task_id"
    TASKS {
        text id PK
        text session_id
        text user_input
        text status
        text created_at
        text updated_at
    }
    AUDIT_ENTRIES {
        text id PK
        text task_id FK
        text event_type
        text summary
        text payload_json
        text created_at
    }
    MEMORY_ITEMS {
        text id PK
        text category
        text key
        text value
        text provenance
        text sensitivity
        text reviewed_at
        text deleted_at
    }
    EMERGENCY_PAUSE {
        integer singleton PK
        integer paused
        text reason
        text updated_at
        text updated_by
    }
    SCHEMA_MIGRATIONS {
        integer version PK
        text applied_at
    }
```

## Readiness Boundary

Current evidence supports a Rust foundation claim: the workspace has typed
contracts and tested scaffolding for IPC, policy, routing, runtime, storage,
plugins, scheduler, and CLI behavior, plus a first Swift command-console shell.
It does not support a claim that Jarvis is a finished voice assistant, packaged
Mac app, autonomous external-action agent, plugin marketplace, or production
cloud-integrated system.
