# Safety Rules

Jarvis is designed for high autonomy with explicit boundaries. These rules are
release requirements, not optional UX guidance.

## Policy Defaults

- Local models are the default route.
- ChatGPT is the only approved cloud model and must be explicitly routed.
- Restricted, credential-adjacent, private personal, and sensitive system data
  cannot be sent to ChatGPT without explicit approval for that task.
- Risky actions fail closed when policy, identity, plugin validation, route
  checks, or permission state is uncertain.
- Planning and acting are separate. Generating a plan does not grant permission
  to execute side effects.

## Risk Tiers

| Tier | Meaning | Default behavior |
| --- | --- | --- |
| Low | Local, reversible, low-impact work | May run silently with audit logging |
| Notify | Low-risk but user-visible or state-changing | May run with visible status |
| Confirm | Meaningful side effects or sensitive context | Requires explicit approval |
| Block | Not allowed in current policy or product scope | Must not run |

## Required Controls

- Emergency pause stops new actions, pauses scheduled and event-driven jobs,
  cancels active non-critical tasks, and requires deliberate resume.
- Cancellation must propagate across tasks, tool calls, scheduled jobs, and
  proactive triggers.
- Degraded modes must be visible when local models, microphone access, TTS,
  ChatGPT, plugins, persistence, or IPC are unavailable.
- Model-originated tool calls must be constrained to runtime-derived
  first-party inventory advertised from the registered `PluginHost` manifests.
  Unknown plugin IDs, undeclared actions, non-object inputs, schema failures,
  and non-first-party requests must fail closed before execution and be
  surfaced as rejected tool results for bounded model recovery; oversized plans
  and malformed provider envelopes must fail the task. Native provider
  function-call formatting cannot bypass registry lookup or schema validation,
  and installed plugin records cannot become model-planned tools.
- Audit logs must explain model route, permission checks, tool calls,
  approvals, denials, files touched, external actions attempted, failures, and
  final state.
- Approval grant/deny decisions must not execute side effects. Approved
  first-party actions require a one-shot explicit replay that verifies the
  original action and scope contract and records side-effect audit evidence.
- Memory writes must have provenance, timestamp, category, sensitivity label,
  and review/delete controls.
- Scheduler jobs must remain inspectable and cancellable. Persisted scheduler
  metadata is not permission to execute proactive side effects; trigger
  execution still has to pass policy and visibility rules. Scheduled plugin
  actions must also be manifest-opted-in for proactive execution with
  `proactive_run`; non-opted-in scheduled plugin actions fail closed before side
  effects execute.
- Diagnostics exports must redact credentials, command bodies, scheduler
  commands, audit payloads, memory values, raw cancellation reasons, and other
  sensitive payloads.

## V1 Blocks

These are blocked in v1 unless `DESIGN.md` is revised and tests prove the new
policy:

- Full smart-home control.
- Autonomous external communication.
- Purchases, bookings, invites, or messages without approval.
- Multi-user account sync.
- Third-party plugin marketplace.
- Cloud-first routing.
- Plugin access outside declared scopes.
- Executing locally installed plugin metadata. Local installs begin as disabled
  manifest metadata with `execution_grant: metadata_only`. Any installed-plugin
  run request must fail closed, append audit evidence, and report
  `side_effect_executed: false` unless the manifest is a verified
  `local_subprocess` plugin with an explicit action-scoped grant:
  `subprocess_stdio` for non-network actions or `subprocess_stdio_network` for
  network-declaring actions. The network grant must not execute non-network
  actions. Enabled subprocesses must not inherit the app/core process
  environment; only the documented plugin metadata environment allowlist is
  exposed.

## Regression Tests

Safety regressions should fail release verification:

- High-risk actions bypassing approval.
- Cloud routing receiving restricted data without explicit approval.
- Plugin actions executing outside their manifest.
- Installed plugin run attempts that execute while `execution_enabled` is
  false, omit manifest/action/provenance validation, or skip audit evidence.
- Installed plugin network grants executing non-network actions, or stdio
  grants executing network-declaring actions.
- Installed subprocess plugin enablement or execution while provenance status
  is anything other than `matches_install_snapshot`.
- Installed subprocess plugin execution that inherits unrelated app/core
  environment variables or secrets.
- Local plugin manifests installing with invalid schema, blocked risk tier,
  missing proactive/memory/model permissions, unsafe source paths, or
  `first_party` source claims.
- Scheduled jobs running while emergency pause is active.
- Audit entries missing route, policy, approval, or action evidence.
- Diagnostics containing raw secrets.
- Diagnostics exposing command bodies, memory values, scheduler command text,
  audit payloads, or cancellation reason text.
