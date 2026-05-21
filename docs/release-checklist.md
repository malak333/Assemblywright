# Release Checklist

Use this checklist before tagging or publishing any Jarvis release. Keep the
evidence local-first unless the user explicitly approves hosted infrastructure.

## Scope Check

- Confirm the release target is this public repository
  (`https://github.com/malak333/Jarvis`) and that the work is landing through a
  reviewable worktree/branch/PR slice.
- Confirm `DESIGN.md` still matches the implementation scope.
- Confirm release notes distinguish implemented Rust foundation and Swift shell
  scaffold behavior from the implemented opt-in Ollama-compatible local
  provider boundary, implemented opt-in ChatGPT/OpenAI-compatible provider
  boundary, metadata-only local plugin installation, and planned Swift approval
  UI, installed plugin execution, voice support, and packaging work.
- Confirm the current architecture map still matches the real module wiring,
  especially the fact that `/commands` invokes the configured routed
  `ModelExecutor` (`FakeLocalModel` by default, Ollama-compatible HTTP or
  ChatGPT/OpenAI-compatible HTTP only when explicitly enabled), records
  route/policy/plugin audit evidence for deterministic first-party plugin
  commands, and supports bounded fake-model planned first-party tool execution
  before any broader assistant claim.
- Confirm the current-vs-target implementation phase table is up to date before
  using any production-readiness language. Release notes may claim foundation
  readiness only for verified Rust/Swift surfaces, not full assistant readiness.
- Confirm no Marvel branding, copyrighted visuals, or confusing product claims
  were introduced.
- Confirm any autonomous sweep summary names the active ownership slices and
  states which evidence came from commands, tests, or manual checks. A
  six-agent sweep is coordination context, not proof of readiness.

## Code Gate

- `./scripts/release-local.sh`

The script runs the full local gate below, including the opt-in ignored
release-proof E2E test. Run individual commands only when diagnosing a failing
stage or when a PR needs focused evidence for one ownership slice.

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
- Confirm restricted data cannot route to cloud, and credential-adjacent or
  private cloud routes require explicit approval.
- Confirm local and ChatGPT provider errors and route evidence do not include
  raw command bodies, API keys, or unredacted endpoint credentials.
- Confirm emergency pause blocks IPC runtime command execution and cancels
  active scheduler jobs.
- Confirm scheduler due-job execution fails closed by activating emergency
  pause, cancelling remaining open scheduler jobs, and recording scheduler
  audit evidence when a due command is not accepted.
- Confirm runtime emergency pause and cancellation tests still cover active
  command cancellation.
- Confirm plugin manifests validate declared permissions, schemas, proactive
  behavior, memory/model access, timeout behavior, and cancellation behavior.
- Confirm local plugin installation accepts only validated manifest metadata
  with safe absolute source paths and stores installed records with
  `execution_enabled: false`.
- Confirm installed plugin metadata does not become executable; execution is
  still limited to deterministic first-party in-process plugins.
- Confirm installed plugin run attempts fail closed with manifest/version and
  action validation, `execution_enabled: false` semantics, durable audit
  evidence, and `side_effect_executed: false`.
- Confirm persistent audit entries remain append-only in SQLite tests.
- Confirm route, policy, approval, action, and failure evidence stay covered
  before claiming an end-to-end assistant release. The current command path
  persists runtime, route, and deterministic first-party plugin audit evidence
  when repository backing is used. Approval-required first-party command
  scaffolds persist inspectable pending approvals and record CLI/IPC grant or
  denial decisions without executing side effects. Bounded fake-model
  first-party tool calls and Ollama-compatible plus ChatGPT/OpenAI-compatible
  provider request/error behavior are covered in focused tests; Swift user
  approval UI remains a future gate.
- Confirm task, audit, memory, and plugin manifest inspection endpoints still
  require or use the correct repository/plugin backing and are covered by local
  smoke or focused IPC tests.
- Confirm approval inspection and grant/deny endpoints require repository
  backing, preserve fail-closed execution behavior, and stay covered by local
  IPC tests.
- Confirm scheduler job create/list/cancel and due-run execution state is
  restored and updated when repository backing is enabled. Due-run coverage
  proves explicit CLI/IPC runner behavior, including interval reschedule and
  fail-closed pause behavior, not background production trigger scheduling.
- Confirm diagnostics export remains redacted and does not include command
  bodies, scheduler commands, audit payloads, memory values, raw cancellation
  reasons, or credentials.
- Confirm the cross-process CLI E2E still covers command, plugin, audit,
  memory create/update/review/delete, scheduler schedule/get/list/cancel,
  scheduler run-due success/reschedule, scheduler fail-closed pause on
  non-accepted due jobs, diagnostics redaction, persistence restart, and
  emergency-pause blocking/resume behavior. Treat this as the minimum E2E
  expectation for the current Rust/CLI foundation; packaged Mac E2E remains a
  future release gate.
- Confirm local plugin metadata install/list/get coverage remains in that E2E
  path while installed plugin execution remains disabled.
- Confirm the Swift shell remains described as a scaffold until a signed
  packaged app bundles/launches the Rust core, handles approval prompts, and
  passes packaged app smoke checks.

## Documentation Gate

- Architecture map is current.
- Both architecture diagrams render: the current implementation diagram and the
  end-goal production diagram.
- Current-vs-target implementation phase table is current.
- Plugin contract is current.
- Safety rules are current.
- Build/test commands are current.
- Knowledge-base notes capture durable workflow and proof-boundary facts.
- Knowledge-base notes include public-repo status, worktree/branch/PR workflow,
  six-agent autonomous sweep expectations, E2E expectations, and proof
  boundaries without overclaiming production readiness.
- README points to the active design and command gate.
- Mermaid diagrams render in GitHub or the intended documentation viewer.

## Mac App Smoke Test

This is a future gate once a packaged `Jarvis.app` exists:

- Packaged app launches on a clean Mac user profile.
- App starts and supervises `jarvis-core`.
- Text command reaches the Rust core.
- Text-transcript voice command parity is verified through the scaffold.
- Real microphone voice command parity is verified only after the macOS
  Speech/AVFoundation adapter is implemented and available.
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
- Any blockers that prevent treating the run as full production assistant
  readiness rather than local foundation evidence.
