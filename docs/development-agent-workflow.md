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
