# Release Checklist

Use this checklist before tagging or publishing any Jarvis release. Keep the
evidence local-first unless the user explicitly approves hosted infrastructure.

## Scope Check

- Confirm `DESIGN.md` still matches the implementation scope.
- Confirm release notes distinguish implemented Rust foundation and Swift shell
  scaffold behavior from planned real local model integration, approval UI,
  installed plugin execution, voice support, and packaging work.
- Confirm the current architecture map still matches the real module wiring,
  especially the fact that `/commands` invokes the runtime/fake local model
  path, records route/policy/plugin audit evidence for deterministic
  first-party plugin commands, and supports bounded fake-model planned
  first-party tool execution before any broader real-provider claim.
- Confirm the current-vs-target implementation phase table is up to date before
  using any production-readiness language. Release notes may claim foundation
  readiness only for verified Rust/Swift surfaces, not full assistant readiness.
- Confirm no Marvel branding, copyrighted visuals, or confusing product claims
  were introduced.

## Code Gate

- `./scripts/release-local.sh`

The script runs the full local gate below, including the opt-in ignored
release-proof test. Run individual commands only when diagnosing a failing
stage.

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo test --workspace -- --ignored`
- `cargo build --workspace`
- `cargo run -p jarvis-cli -- smoke`
- `cargo package --workspace --allow-dirty`
- `swift test --package-path apps/mac`
- `swift build --package-path apps/mac`
- Optional manual CLI/IPC smoke against a running local server:
  - Terminal 1: `cargo run -p jarvis-cli -- serve`
  - Terminal 2: `cargo run -p jarvis-cli -- health`
  - Terminal 2: `cargo run -p jarvis-cli -- command --dry-run "status check"`
  - Terminal 2: `cargo run -p jarvis-cli -- scheduler list`
  - Terminal 2: `cargo run -p jarvis-cli -- pause --reason "release smoke"`
  - Terminal 2: `cargo run -p jarvis-cli -- pause-status`
  - Terminal 2: `cargo run -p jarvis-cli -- resume`

## Safety Gate

- Confirm high-risk actions require approval or are blocked.
- Confirm cloud routing is local-first and ChatGPT-only when cloud use is
  approved.
- Confirm restricted and credential-adjacent data cannot route to cloud without
  explicit approval.
- Confirm emergency pause blocks IPC runtime command execution and cancels
  active scheduler jobs.
- Confirm runtime emergency pause and cancellation tests still cover active
  command cancellation.
- Confirm plugin manifests validate declared permissions, schemas, proactive
  behavior, memory/model access, timeout behavior, and cancellation behavior.
- Confirm local plugin installation accepts only validated manifest metadata
  with safe absolute source paths and stores installed records with
  `execution_enabled: false`.
- Confirm installed plugin metadata does not become executable; execution is
  still limited to deterministic first-party in-process plugins.
- Confirm persistent audit entries remain append-only in SQLite tests.
- Confirm route, policy, approval, action, and failure evidence stay covered
  before claiming an end-to-end assistant release. The current command path
  persists runtime, route, and deterministic first-party plugin audit evidence
  when repository backing is used. Bounded fake-model first-party tool calls are
  covered in runtime tests; real provider tool calls and user approval UI remain
  future gates.
- Confirm task, audit, memory, and plugin manifest inspection endpoints still
  require or use the correct repository/plugin backing and are covered by local
  smoke or focused IPC tests.
- Confirm scheduler job create/list/cancel state is restored and updated when
  repository backing is enabled. This is durable job metadata, not proof that
  proactive production triggers execute.
- Confirm diagnostics export remains redacted and does not include command
  bodies, scheduler commands, audit payloads, memory values, raw cancellation
  reasons, or credentials.
- Confirm the cross-process CLI E2E still covers command, first-party plugin
  execution, local plugin metadata install/list/get, audit, memory
  create/update/review/delete, scheduler schedule/get/list/cancel, diagnostics
  redaction, persistence restart, and emergency-pause
  blocking/resume behavior.
- Confirm the Swift shell remains described as a scaffold until a signed
  packaged app bundles/launches the Rust core, handles approval prompts, and
  passes packaged app smoke checks.

## Documentation Gate

- Architecture map is current.
- Current-vs-target implementation phase table is current.
- Plugin contract is current.
- Safety rules are current.
- Build/test commands are current.
- Knowledge-base notes capture durable workflow and proof-boundary facts.
- README points to the active design and command gate.
- Mermaid diagrams render in GitHub or the intended documentation viewer.

## Mac App Smoke Test

This is a future gate once a packaged `Jarvis.app` exists:

- Packaged app launches on a clean Mac user profile.
- App starts and supervises `jarvis-core`.
- Text command reaches the Rust core.
- Voice command parity is verified where microphone access is available.
- Activity view shows current task state.
- Audit entry is written for the command.
- Emergency pause stops new actions.
- App exits cleanly and restarts with recoverable state.

## Release Notes

Release notes must include:

- Version number.
- Summary of user-visible changes.
- Migration notes.
- Known limitations.
- Local verification commands and dates.
- Any manual checks that remain the user's responsibility.
