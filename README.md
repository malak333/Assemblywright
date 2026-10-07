# Assemblywright

> Orchestrated intelligence. Verified software.

Assemblywright gives you a Mac app for running a queue of software features on a
Windows machine. Describe what you want, work through the plan with ChatGPT/Codex,
then approve it and add it to the queue. A local coding model makes the changes;
your validation command and an independent Codex review check the result before
connected projects move through a GitHub pull request and merge. You can watch
progress, stop work, and resume from saved checkpoints. If you already use Claude
Code or Codex with CI, the reason to use Assemblywright is the workflow around the
coding: approved plans, a persistent feature queue, repair limits, and a recorded
history of validation, review, and publication in one place. It is useful when you
want to supervise a sequence of features without manually coordinating each handoff.

The Mac app is the control panel; Windows stores the queue and runs the work under
your existing Windows account. Setup currently requires both machines and the
configured model services. See the [workflow guide](docs/developer-build.md).

## Build and launch

Configure the app-owned Windows connection, local coding runtime, and Codex reviewer
using the [workflow guide](docs/developer-build.md), then build and launch:

```sh
./scripts/production-build.py --build
```

For later launches, run `./scripts/production-build.py` or open
`Open Assemblywright.command`. The launcher manages the configured background
Windows connection. Default connection values reflect the owner's two-machine
setup; see the workflow guide before using another environment.

The [production build guide](docs/production-build.md) covers release compilation,
packaging, retained state, and recovery. The launcher creates an ad-hoc signed
local app; Developer ID signing and notarized distribution require separate
credentials and evidence. The compatibility debug profile remains available through
`./scripts/developer-build.py --build`.

## Using the queue

1. Select an existing Windows project or create a new project folder.
2. Describe a feature and specify its validation command.
3. Brainstorm with ChatGPT/Codex, review the design documents, and approve them.
4. Add the approved feature to the queue and start execution.
5. Inspect the changed files, validation results, and independent Codex review.

Use Stop to interrupt work, Resume to continue from a saved checkpoint, or
Emergency Pause to halt the queue. Auto-run moves to the next feature after the
current one finishes successfully. Your tool permission settings apply across
projects, chats, and features. Project chat lets you discuss a project separately
from its queue. If a failed feature needs a manual repair, you review and approve
the proposed changes before they are applied. You can also enable Auto AI Repair
with a repair limit; repaired work still has to pass validation and a new
independent review.

For connected GitHub projects, the runner opens a pull request for the reviewed
changes, waits for required checks, merges, and confirms the result on GitHub
before advancing the queue. Models cannot choose a
publication destination, bypass review, or approve a merge.

See the [workflow guide](docs/developer-build.md),
[global permissions](docs/developer-global-permissions-design.md),
[repair contract](docs/developer-chat-repair-design.md),
[Auto AI Repair](docs/developer-auto-ai-repair-design.md), and
[GitHub publication](docs/developer-github-publication-design.md) for setup and
recovery details.

## Repository layout

| Path | Responsibility |
| --- | --- |
| [`apps/mac`](apps/mac) | SwiftUI app, native controls, and bridge helper. |
| [`crates/assemblywright-master`](crates/assemblywright-master) | Windows supervised runner and protected-service queue, policy, identity, and audit authority. |
| [`crates/assemblywright-protocol`](crates/assemblywright-protocol) | Typed wire contracts and compatibility fixtures. |
| [`crates/assemblywright-agent`](crates/assemblywright-agent) | Bounded Mac worker for the protected-service mode. |
| [`crates/assemblywright-core`](crates/assemblywright-core) | Local transport and read-only release evidence. |
| [`crates/assemblywright-cli`](crates/assemblywright-cli) | Read-only release readiness and evidence inspection. |
| [`scripts`](scripts) | Launchers, validation gates, packaging, and operator tooling. |
| [`docs`](docs) | Accepted designs, workflow guides, safety rules, and release evidence. |

## Protected-service mode

The default app uses the workflow above. A separate, more restricted service mode
is available through `ASSEMBLYWRIGHT_RUNTIME=protected-service`. It has its own
setup and safety requirements.

This mode provides device enrollment, encrypted connections, a saved queue, and
recovery after service restarts. Coding workers can only write or delete the files
specified in an approved task inside a private workspace. They cannot run shell
commands or tests, call models, use credentials, or access the network. Your
controls are checked against the current saved state so an outdated request cannot
silently change newer work.

Independent review is unavailable until its Codex adapter is configured. A limited
live test checks that this review connection works; it does not establish general
reviewer quality or readiness for deployment. Autonomous dispatch, changes to the
registered source checkout, and GitHub publication remain unavailable in this mode.
Activation requires six separately admitted live test records on Windows. Selecting
this mode does not bypass those requirements or change identity checks, permissions,
cancellation, Emergency Pause, or the audit history.

For the technical contracts and test evidence, read the
[architecture map](docs/architecture-map.md),
[Feature Conveyor design](docs/feature-conveyor-design.md), and
[distributed Developer Mode design](docs/distributed-developer-mode-design.md).

## Build and test

Use the pinned Rust toolchain in `rust-toolchain.toml` and Swift Package Manager
under `apps/mac`. Run the canonical local release gate for repository evidence:

```sh
./scripts/release-local.sh
```

Focused commands for development:

```sh
cargo test --workspace
swift test --disable-sandbox --package-path apps/mac
cargo run -p assemblywright-cli -- release readiness
```

The `assemblywright` CLI inspects readiness and evidence without performing release
side effects. See [build and test commands](docs/build-test-commands.md) for the
full command list and exact proof boundaries.

A green repository gate does not establish signing, notarization, clean-profile
installation, live-device reliability, host hardening, or unattended operation.
Those require separate release evidence.

## Documentation

- [Design](DESIGN.md) and [architecture map](docs/architecture-map.md)
- [Production build](docs/production-build.md) and [workflow guide](docs/developer-build.md)
- [Safety rules](docs/safety-rules.md)
- [Build and test commands](docs/build-test-commands.md)
- [Release checklist](docs/release-checklist.md)
- [Development agent workflow](docs/development-agent-workflow.md)
- [Knowledge-base facts](docs/knowledge-base/assemblywright-project-facts.md)
- [Brand system](docs/brand.md)

## License

[Apache License 2.0](LICENSE).
