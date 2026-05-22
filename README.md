# Jarvis

Jarvis is a local-first macOS assistant foundation. The current repo implements
the Rust core described in [DESIGN.md](DESIGN.md): durable task/audit
primitives, policy-gated first-party plugin commands, bounded model-planned
first-party tool orchestration, local-first model routing evidence, opt-in
Ollama-compatible local HTTP and ChatGPT/OpenAI-compatible provider boundaries,
plugin contracts, metadata-only local plugin installation, scheduler state,
redacted diagnostics export, a loopback IPC surface, and CLI smoke paths for
the Swift shell scaffold and future packaged app.
It also includes the first buildable Swift/SwiftUI Mac shell scaffold under
`apps/mac`, with a tested IPC client, command-console state model,
activity/audit panel for command evidence, management surfaces, degraded-mode
handling, and a core supervisor abstraction.

## Current Scope

This repository is intentionally v1 foundation work, not a Marvel/JARVIS clone
and not an autonomous external-communication system. Risky side effects must be
blocked or require approval, and every meaningful decision should be auditable.
The current implementation should not be described as a finished production
assistant: distribution signing/notarization, live microphone validation,
third-party plugin trust, richer permission-center UX, and manual release QA
are still target architecture. The
default command path still uses `FakeLocalModel`; set
`JARVIS_LOCAL_MODEL_PROVIDER=ollama`, `JARVIS_LOCAL_MODEL`, and optionally
`JARVIS_OLLAMA_BASE_URL`/`JARVIS_LOCAL_MODEL_TIMEOUT_MS` to exercise the local
HTTP provider. ChatGPT execution is disabled by default and requires explicit
typed env opt-in with `JARVIS_CHATGPT_ENABLED=true`,
`JARVIS_OPENAI_API_KEY`, and optional `JARVIS_CHATGPT_MODEL`,
`JARVIS_OPENAI_BASE_URL`, and `JARVIS_CHATGPT_TIMEOUT_MS`; route policy still
blocks restricted data and sends only redacted route context. Local plugin
installation stores validated manifest metadata disabled by default; executable
local subprocess plugins require an explicit `subprocess_stdio` grant and still
run only through the constrained JSON stdin/stdout boundary.

## Production Work Protocol

This public repository is being advanced through isolated worktrees, topic
branches, and reviewable PR slices. During the autonomous production sweep,
agents should stay inside their assigned ownership, preserve unrelated edits,
and treat cross-process E2E plus the local release gate as the evidence bar.
Passing the local gate supports only the implemented Rust/Swift foundation
claim; it is not proof of a finished packaged assistant.

Phase 3 landed through separate worktrees for model route persistence, plugin
subprocess sandboxing, voice input controls, packaged app release smoke,
permission grants UX, docs architecture alignment, distribution packaging, and
Keychain launch credential injection. Follow-on slices continue the same
branch/PR discipline; release language should describe only the merged
repo-owned surfaces with recorded focused E2E or integration proof.

## Build

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
swift test --package-path apps/mac
swift build --package-path apps/mac
```

For the current IPC smoke path, start the local server and run CLI commands
from a second terminal:

```sh
cargo run -p jarvis-cli -- serve
cargo run -p jarvis-cli -- health
```

Use `cargo run -p jarvis-cli -- serve --db-path /tmp/jarvis.sqlite` when you
want manual IPC commands to persist task and audit state locally.

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
./scripts/package-distribution.sh --check
```

The full `package-distribution.sh` lane owns Developer ID signing,
notarization, stapling, and microphone entitlement packaging when Apple
credentials are provided. It still does not replace clean-profile Finder
launch, installer/App Store validation, or live microphone/Speech/audio-output
validation.

With a repository-backed server running, `jarvis tasks`, `jarvis memory`,
`jarvis scheduler`, `jarvis diagnostics`, and `jarvis plugins` expose the
current durable state, redacted diagnostics, first-party plugin manifests, and
disabled installed-plugin registry metadata over IPC.

## Docs

- [Architecture map](docs/architecture-map.md)
- [Plugin contract](docs/plugin-contract.md)
- [Safety rules](docs/safety-rules.md)
- [Build and test commands](docs/build-test-commands.md)
- [Release checklist](docs/release-checklist.md)
- [Knowledge-base facts](docs/knowledge-base/jarvis-project-facts.md)

The architecture map includes both the implemented current-state diagram and
the end-goal production diagram, plus a phase table that separates verified
foundation work from future production assistant requirements.
