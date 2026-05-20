# Release Checklist

Use this checklist before tagging or publishing any Jarvis release. Keep the
evidence local-first unless the user explicitly approves hosted infrastructure.

## Scope Check

- Confirm `DESIGN.md` still matches the implementation scope.
- Confirm release notes distinguish implemented Rust foundation behavior from
  planned Swift shell, IPC, plugin, memory, scheduler, and packaging work.
- Confirm no Marvel branding, copyrighted visuals, or confusing product claims
  were introduced.

## Code Gate

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build --workspace`
- `cargo run -p jarvis-cli -- health`

## Safety Gate

- Confirm high-risk actions require approval or are blocked.
- Confirm cloud routing is local-first and ChatGPT-only when cloud use is
  approved.
- Confirm restricted and credential-adjacent data cannot route to cloud without
  explicit approval.
- Confirm emergency pause behavior is tested once implemented.
- Confirm plugin actions cannot run outside declared scopes once plugin APIs
  exist.
- Confirm audit logs include route, policy, approval, action, and failure
  evidence once persistence exists.

## Documentation Gate

- Architecture map is current.
- Plugin contract is current.
- Safety rules are current.
- Build/test commands are current.
- Knowledge-base notes capture durable workflow and proof-boundary facts.
- README points to the active design and command gate.

## Mac App Smoke Test

This is a future gate once `Jarvis.app` exists:

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
