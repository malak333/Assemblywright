# Architecture Map

Jarvis v1 is a local-first macOS assistant foundation made of a native Mac shell
and a Rust core. The current repository contains the Rust workspace foundation;
the Swift shell is still a planned surface from `DESIGN.md`.

## Process Boundaries

```text
Jarvis.app (Swift/SwiftUI)
  - voice, command console, settings, prompts, activity, diagnostics
  - starts and supervises jarvis-core in v1
  - never becomes the agent brain

IPC boundary
  - initial options: loopback HTTP or Unix domain socket
  - contract must be versioned and tested before Swift depends on it

jarvis-core (Rust)
  - task lifecycle, model routing, memory, tools, plugins, scheduler
  - permission and risk decisions
  - append-only audit events
  - durable local state and migrations
```

## Workspace Layout

```text
.
|-- DESIGN.md
|-- README.md
|-- Cargo.toml
|-- crates/
|   |-- jarvis-core/
|   |   `-- src/
|   |       |-- lib.rs
|   |       `-- types.rs
|   `-- jarvis-cli/
|       `-- src/main.rs
`-- docs/
```

## Rust Core Responsibilities

- `jarvis-core`: Owns product-grade assistant state and contracts. Today it
  exposes the public foundation types for task records, audit entries,
  sensitivity labels, risk tiers, approval status, task status, and shared
  errors.
- `jarvis-cli`: Provides the current command entry point. Today `jarvis health`
  verifies that the binary is present and can initialize.

Future modules should keep the ownership boundary from `DESIGN.md`:

- Conversation runtime: sessions, turns, task state, streaming status, and
  cancellation.
- Model router: local-first decisions, explicit ChatGPT escalation, redaction,
  sensitivity checks, and route audit entries.
- Memory store: provenance, timestamps, sensitivity labels, and review/delete
  controls.
- Permission and risk engine: capability scopes plus risk tiers.
- Tool and plugin host: manifest validation, schema validation, policy checks,
  execution, timeouts, cancellation, and audit output.
- Scheduler and trigger engine: local, inspectable proactive routines.
- Audit log: append-only task, route, tool, approval, denial, and failure
  records.

## Data Ownership

- SQLite is the source of truth for structured state once persistence lands.
- macOS Keychain stores credentials and OAuth tokens.
- App-owned files store large artifacts, transcripts, diagnostics exports,
  plugin bundles, local model configs, and attachments.
- Any vector index is rebuildable and points back to canonical SQLite records.

## Current Implementation Status

The repository currently proves only the Rust workspace foundation and shared
type contracts. It does not yet contain the Swift app, IPC server, SQLite
migrations, plugin runtime, model router, memory store, or scheduler. New
readiness claims must stay inside that boundary until those pieces exist and
are tested.
