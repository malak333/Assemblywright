# Development Agent Workflow

Assemblywright uses repository-scoped Codex agent definitions under `.codex/agents`.
They support development of Assemblywright; they are not loaded by the Assemblywright product
runtime and do not grant product capabilities or release readiness.

## Routing Matrix

| Work | Agent | Model | Sandbox |
| --- | --- | --- | --- |
| Cross-file exploration and evidence | `assemblywright-explorer` | Terra Medium | Read-only |
| Mechanical one/two-file edit | `assemblywright-quick-worker` | Luna Low | Workspace write |
| Normal multi-file implementation | `assemblywright-worker` | Sol Medium | Workspace write |
| Architecture, auth, permissions, routing, migrations, plugins, concurrency, release evidence | `assemblywright-high-risk-worker` | Sol High | Workspace write |
| Routine diff review | `assemblywright-reviewer` | Terra High | Read-only |
| Security-sensitive or trust-boundary review | `assemblywright-high-risk-reviewer` | Sol High | Read-only |

Sol uses the installed Codex model slug `gpt-5.6-sol`. Project configuration
caps open agent threads at four and nesting at one level. A parent should do known-target work
directly, use read-heavy parallelism when it reduces context or elapsed time,
and serialize overlapping writes.

## Operating Rules

- Assign an explicit objective and owned paths to every write agent.
- Use an isolated worktree for concurrent feature slices; never share an
  overlapping write set.
- Keep explorers and reviewers read-only through sandbox configuration, not
  prompt wording alone.
- Route high-risk work to the Sol High roles before implementation or review;
  do not wait for a cheaper role to fail.
- Require focused tests and exact command results in each implementation
  handoff. Require an independent reviewer for high-risk changes.
- Commit and push remain parent-controlled actions requiring an explicit user
  request. No publishing agent is installed.
- A green repository gate is not signing, notarization, clean-profile launch,
  live-device QA, plugin-trust QA, or owner-recorded external evidence.

## Validation

Run after changing `AGENTS.md`, `.codex/config.toml`, or any custom agent:

```sh
./scripts/validate-codex-workflow.sh
```

The validator parses every TOML file, verifies the expected model, reasoning,
and sandbox matrix, checks the concurrency limits, rejects dangerous policy
overrides, and confirms the root instructions reference every role.

## Feature And Phase Closeout

Every feature or phase closes as one auditable slice. Before publication:

1. Re-read the applicable accepted design, safety rules, requirements, and
   canonical build commands. Update implementation and documentation together;
   a stale or contradicted contract is blocking.
2. Review the conversation for durable repository facts. Add reusable
   architecture, failure, command, proof-boundary, or operator knowledge to
   `docs/knowledge-base/assemblywright-project-facts.md`; record explicitly when
   there is no new durable knowledge.
3. Apply the `unit-testing-test-generate` workflow when available. Cover the
   smallest testable units with success, rejection, empty, boundary, maximum,
   malformed-state, redaction, idempotence, cancellation, concurrency, and
   recovery cases that apply. Do not generate meaningless mocks or assertions.
4. Apply the `e2e-testing` workflow when available, but choose the E2E
   technology from the real product boundary. Playwright, screenshots, visual
   regression, and cross-browser matrices apply only to an actual browser
   surface. Rust/Swift APIs, native apps, processes, protocols, Windows
   services, and Mac/Windows paths require native cross-process, packaged-app,
   service, or live-device E2E.
5. Run focused tests, `./scripts/release-docs-drift-smoke.sh`,
   `git diff --check`, and `./scripts/release-local.sh`. Keep Windows-only,
   signing, notarization, clean-profile, and live-device evidence separate and
   name every unexecuted boundary.
6. Obtain an independent review proportional to risk and resolve every blocking
   finding. Security, authentication, routing, persistence, concurrency, and
   publication changes require the high-risk reviewer.
7. When the owner requested publication, commit without bypassing hooks, push
   to `main`, verify local `HEAD` equals `origin/main`, and wait for every
   required hosted gate. A local pass or successful push alone is not closeout.
8. For Windows-master or cross-device changes, fast-forward the authoritative
   Windows checkout to that exact published SHA. Rebuild and restart the
   `AssemblywrightMaster` service when its runtime inputs changed, then verify
   source parity, protocol/schema compatibility, service health, queue/pause
   state, and migration backup evidence when applicable. Record explicitly
   when a docs/test-only slice leaves the deployed binary unchanged.
