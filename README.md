# Assemblywright

> Orchestrated intelligence. Verified software.

Assemblywright is an owner-controlled developer-agent system. A macOS app provides
planning, queue controls, project chat, and review results. A Windows runner owns
the durable state and implements one feature at a time from an owner-approved feature queue.
Frontier models assist with planning and independent review; the configured local
coding runtime performs implementation under the existing Windows owner account.

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

## Supervised workflow

1. Select an existing Windows project or create a new project folder.
2. Describe a feature and specify its validation command.
3. Brainstorm with ChatGPT/Codex, review the design documents, and approve them.
4. Add the approved feature to the queue and start execution.
5. Inspect the changed files, validation results, and independent Codex review.

Stop, Emergency Pause, checkpointed Resume, and auto-run operate on Windows
processes and saved files. Global tool permissions apply across projects, chats,
and features. Project chat is separate from the queue; manual repair proposals
require approval of their exact displayed bytes. A separately enabled Auto AI
Repair policy permits bounded repairs while preserving validation and fresh
independent review requirements.

For connected GitHub projects, the publication workflow binds the reviewed commit
to a feature branch and pull request, waits for required checks, merges, and
verifies the remote base before advancing the queue. Models cannot choose a
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

The separate protected-service foundation is retained behind
`ASSEMBLYWRIGHT_RUNTIME=protected-service`. Its restrictions and activation gates
are distinct from the default supervised workflow.

It implements durable device lifecycle and queue contracts, enrollment and mTLS
identity, Windows service recovery, repository snapshots, isolated artifact
integration, validation gates, and revision-bound owner controls. The bounded
coding lane permits packet-bound deterministic writes/deletes in private
workspaces without shell, provider, test, credential, or network authority.

The default-unavailable owner-loopback independent-review gateway requires explicit
provisioning. The protected service implements an
independent-review gateway with a separately provisioned pinned Codex adapter
and a narrow live semantic proof controller. That proof does not establish general
reviewer quality or deployment readiness.

Autonomous dispatch, registered-source-checkout mutation, and live GitHub
publication authority remain unavailable in this mode. Activation requires
Windows to admit all six required live proof receipts. Selecting the mode does
not grant effect authority or relax identity, policy, cancellation, Emergency
Pause, or audit rules.

For the complete inventory and evidence boundaries, read the
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
